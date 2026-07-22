mod blank_templates;

use std::{
    fs::{self, File},
    io::{self, Seek, Write},
    path::{Component, Path, PathBuf},
};

use docvault_types::{OoxmlManifest, OoxmlManifestEntry};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::{debug, info};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const SUPPORTED_EXTENSIONS: &[&str] = &["docx", "xlsx", "pptx"];

#[derive(Debug, Error)]
pub enum OoxmlError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("unsafe OOXML package entry: {0}")]
    UnsafeEntry(String),
    #[error("unsupported OOXML format: {0} (expected docx, xlsx, or pptx)")]
    UnsupportedFormat(String),
}

pub type OoxmlResult<T> = Result<T, OoxmlError>;

pub fn is_supported_ooxml(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
        .unwrap_or(false)
}

/// Content-based OOXML detection: true when `path` is a ZIP archive containing
/// a `[Content_Types].xml` entry (the OOXML package marker). Unlike
/// [`is_supported_ooxml`], this looks at the file's bytes rather than its
/// extension, so a Kingsoft `.wps`/`.et`/`.dps` that is really an OOXML
/// package (WPS can save in the OOXML format) is recognized as such, while a
/// legacy Kingsoft-binary or any non-ZIP file is not. Returns `false` for a
/// missing file or a non-ZIP file rather than erroring, so it can drive a
/// simple unpack-vs-raw-copy branch at archive time.
pub fn is_ooxml_package(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(mut archive) = ZipArchive::new(file) else {
        return false;
    };
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        if entry.name() == "[Content_Types].xml" {
            return true;
        }
    }
    false
}

pub fn unpack_package(
    source_path: impl AsRef<Path>,
    destination_dir: impl AsRef<Path>,
) -> OoxmlResult<()> {
    let source_path = source_path.as_ref();
    let destination_dir = destination_dir.as_ref();
    info!(
        source = %source_path.display(),
        destination = %destination_dir.display(),
        "unpacking OOXML package"
    );
    fs::create_dir_all(destination_dir)?;

    let file = File::open(source_path)?;
    let mut archive = ZipArchive::new(file)?;
    let entry_count = archive.len();
    for index in 0..entry_count {
        let mut entry = archive.by_index(index)?;
        let entry_name = entry.name().to_owned();
        let relative_path = safe_relative_path(&entry_name)?;
        let output_path = destination_dir.join(relative_path);

        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
        } else {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = File::create(&output_path)?;
            io::copy(&mut entry, &mut output)?;
        }
    }

    debug!(entries = entry_count, "OOXML package unpacked");
    Ok(())
}

pub fn pack_package(
    source_dir: impl AsRef<Path>,
    destination_path: impl AsRef<Path>,
) -> OoxmlResult<()> {
    let source_dir = source_dir.as_ref();
    let destination_path = destination_path.as_ref();
    info!(
        source = %source_dir.display(),
        destination = %destination_path.display(),
        "packing OOXML package"
    );
    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = File::create(destination_path)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    add_directory_to_zip(source_dir, source_dir, &mut writer, options)?;
    writer.finish()?;
    debug!("OOXML package packed");
    Ok(())
}

pub fn package_manifest(source_path: impl AsRef<Path>) -> OoxmlResult<OoxmlManifest> {
    let source_path = source_path.as_ref();
    info!(source = %source_path.display(), "generating OOXML package manifest");
    let file = File::open(source_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entries = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let entry_name = entry.name().to_owned();
        let relative_path = safe_relative_path(&entry_name)?;
        let path = relative_path.to_string_lossy().replace('\\', "/");
        let mut hasher = Sha256::new();
        let size = io::copy(&mut entry, &mut HashWriter(&mut hasher))?;
        entries.push(OoxmlManifestEntry {
            path,
            size,
            sha256: format!("{:x}", hasher.finalize()),
            content_type: None,
        });
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(OoxmlManifest { entries })
}

/// Single-entry manifest for a non-OOXML file (pdf, md, txt, a legacy
/// Kingsoft-binary `.wps`/`.et`/`.dps`, ...): the whole file is one logical
/// "part" with its size and sha256. Mirrors the per-entry shape of
/// [`package_manifest`] so the rest of the pipeline (which stores a manifest on
/// every version) works uniformly for OOXML and raw-binary documents.
pub fn file_manifest(path: impl AsRef<Path>) -> OoxmlResult<OoxmlManifest> {
    let path = path.as_ref();
    info!(source = %path.display(), "generating single-file manifest for non-OOXML document");
    let basename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .to_owned();
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let size = io::copy(&mut file, &mut hasher)?;
    Ok(OoxmlManifest {
        entries: vec![OoxmlManifestEntry {
            path: basename,
            size,
            sha256: format!("{:x}", hasher.finalize()),
            content_type: None,
        }],
    })
}

/// Content-aware manifest: the per-entry package manifest for an OOXML file,
/// or a single-entry whole-file manifest for anything else. Use this at commit
/// time so manifest computation never fails merely because the source is not an
/// OOXML package.
pub fn manifest_for(path: impl AsRef<Path>) -> OoxmlResult<OoxmlManifest> {
    let path = path.as_ref();
    if is_ooxml_package(path) {
        package_manifest(path)
    } else {
        file_manifest(path)
    }
}

/// Write a minimal but valid OOXML package for `format` (`"docx"`, `"xlsx"`, or
/// `"pptx"`, case-insensitive) to `destination_path`. The package contains just
/// enough parts for Word/Excel/PowerPoint to open it as an empty document and
/// no `docProps`, so no author/timestamp metadata is baked in. Used by the
/// desktop layer's "new blank document" flow. Returns
/// [`OoxmlError::UnsupportedFormat`] for any other format (plain-text formats
/// like txt/md are created by the caller with an empty write, not here).
///
/// `aspect_ratio` only affects pptx: `Some("16:9")` widens the slide to 16:9
/// (13.333in x 7.5in); `None` or any other value keeps the OOXML 4:3 default
/// (10in x 7.5in). Ignored for docx/xlsx.
pub fn create_empty_package(
    format: &str,
    aspect_ratio: Option<&str>,
    destination_path: impl AsRef<Path>,
) -> OoxmlResult<()> {
    let format = format.to_ascii_lowercase();
    let destination_path = destination_path.as_ref();
    info!(
        format = format.as_str(),
        aspect_ratio = ?aspect_ratio,
        destination = %destination_path.display(),
        "creating blank OOXML package"
    );
    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = File::create(destination_path)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    // docx/xlsx ship a static part set; pptx builds an owned one so its slide
    // size can vary with `aspect_ratio`. `write_package_from_parts` is generic
    // over the string type so both the static slices and the owned vec work.
    match format.as_str() {
        "docx" => write_package_from_parts(&mut writer, blank_templates::DOCX, options)?,
        "xlsx" => write_package_from_parts(&mut writer, blank_templates::XLSX, options)?,
        "pptx" => {
            let parts = blank_templates::pptx_parts(aspect_ratio);
            write_package_from_parts(&mut writer, &parts, options)?
        }
        other => return Err(OoxmlError::UnsupportedFormat(other.to_owned())),
    }
    writer.finish()?;
    debug!(format = format.as_str(), "blank OOXML package written");
    Ok(())
}

fn write_package_from_parts<W, S>(
    writer: &mut ZipWriter<W>,
    parts: &[(S, S)],
    options: SimpleFileOptions,
) -> OoxmlResult<()>
where
    W: Write + Seek,
    S: AsRef<str>,
{
    for (relative, contents) in parts {
        let zip_name = relative.as_ref().replace('\\', "/");
        writer.start_file(zip_name, options)?;
        writer.write_all(contents.as_ref().as_bytes())?;
    }
    Ok(())
}

fn add_directory_to_zip<W: Write + Seek>(
    base_dir: &Path,
    current_dir: &Path,
    writer: &mut ZipWriter<W>,
    options: SimpleFileOptions,
) -> OoxmlResult<()> {
    let mut entries = fs::read_dir(current_dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            add_directory_to_zip(base_dir, &path, writer, options)?;
        } else {
            let relative_path = path
                .strip_prefix(base_dir)
                .map_err(|_| OoxmlError::UnsafeEntry(path.display().to_string()))?;
            let zip_name = relative_path.to_string_lossy().replace('\\', "/");
            writer.start_file(zip_name, options)?;
            let mut file = File::open(&path)?;
            io::copy(&mut file, writer)?;
        }
    }

    Ok(())
}

struct HashWriter<'a>(&'a mut Sha256);

impl Write for HashWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn safe_relative_path(entry_name: &str) -> OoxmlResult<PathBuf> {
    let path = Path::new(entry_name);
    if path.is_absolute() {
        return Err(OoxmlError::UnsafeEntry(entry_name.to_owned()));
    }

    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            _ => return Err(OoxmlError::UnsafeEntry(entry_name.to_owned())),
        }
    }

    if output.as_os_str().is_empty() {
        Err(OoxmlError::UnsafeEntry(entry_name.to_owned()))
    } else {
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_zip_entry(path: &Path, entry_name: &str, contents: &[u8]) {
        let file = File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .start_file(entry_name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn detects_supported_office_documents() {
        assert!(is_supported_ooxml("report.docx"));
        assert!(is_supported_ooxml("sheet.XLSX"));
        assert!(is_supported_ooxml("deck.pptx"));
        assert!(!is_supported_ooxml("notes.txt"));
    }

    #[test]
    fn is_ooxml_package_reads_content_not_extension() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        // A real OOXML package: detected regardless of extension (a .wps that
        // is really OOXML must be recognized so it archives like Office).
        let source = root.join("source");
        fs::create_dir_all(source.join("word")).unwrap();
        fs::write(source.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(source.join("word").join("document.xml"), b"doc").unwrap();
        let ooxml_as_wps = root.join("kingsoft.wps");
        pack_package(&source, &ooxml_as_wps).unwrap();
        assert!(is_ooxml_package(&ooxml_as_wps));

        // A plain non-ZIP file and a missing path are not OOXML.
        let txt = root.join("notes.txt");
        fs::write(&txt, b"not a zip").unwrap();
        assert!(!is_ooxml_package(&txt));
        assert!(!is_ooxml_package(root.join("missing")));

        // A ZIP without [Content_Types].xml is not an OOXML package.
        let bare_zip = root.join("bare.zip");
        write_zip_entry(&bare_zip, "readme.txt", b"hello");
        assert!(!is_ooxml_package(&bare_zip));
    }

    #[test]
    fn file_manifest_hashes_whole_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("notes.txt");
        fs::write(&path, b"hello world").unwrap();

        let manifest = file_manifest(&path).unwrap();
        assert_eq!(manifest.entries.len(), 1);
        let entry = &manifest.entries[0];
        assert_eq!(entry.path, "notes.txt");
        assert_eq!(entry.size, "hello world".len() as u64);
        assert_eq!(entry.sha256.len(), 64);
    }

    #[test]
    fn manifest_for_dispatches_by_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        // OOXML -> per-entry package manifest.
        let source = root.join("source");
        fs::create_dir_all(source.join("word")).unwrap();
        fs::write(source.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(source.join("word").join("document.xml"), b"doc").unwrap();
        let docx = root.join("report.docx");
        pack_package(&source, &docx).unwrap();
        let ooxml_manifest = manifest_for(&docx).unwrap();
        assert!(
            ooxml_manifest
                .entries
                .iter()
                .any(|e| e.path == "word/document.xml"),
            "OOXML manifest lists package parts"
        );

        // Non-OOXML -> single-entry whole-file manifest.
        let txt = root.join("notes.txt");
        fs::write(&txt, b"plain text").unwrap();
        let raw_manifest = manifest_for(&txt).unwrap();
        assert_eq!(raw_manifest.entries.len(), 1);
        assert_eq!(raw_manifest.entries[0].path, "notes.txt");
    }

    #[test]
    fn packs_and_unpacks_package_contents() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let source = root.join("source");
        let unpacked = root.join("unpacked");
        let package = root.join("sample.docx");
        fs::create_dir_all(source.join("word")).unwrap();
        fs::write(source.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(source.join("word").join("document.xml"), b"document").unwrap();

        pack_package(&source, &package).unwrap();
        unpack_package(&package, &unpacked).unwrap();

        assert_eq!(
            fs::read(unpacked.join("[Content_Types].xml")).unwrap(),
            b"types"
        );
        assert_eq!(
            fs::read(unpacked.join("word").join("document.xml")).unwrap(),
            b"document"
        );
    }

    #[test]
    fn package_manifest_lists_entries_with_hashes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let source = root.join("source");
        let package = root.join("sample.docx");
        fs::create_dir_all(source.join("word")).unwrap();
        fs::write(source.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(source.join("word").join("document.xml"), b"document").unwrap();
        pack_package(&source, &package).unwrap();

        let manifest = package_manifest(&package).unwrap();

        assert_eq!(manifest.entries.len(), 2);
        assert_eq!(manifest.entries[0].path, "[Content_Types].xml");
        assert_eq!(manifest.entries[0].size, 5);
        assert_eq!(manifest.entries[0].sha256.len(), 64);
        assert_eq!(manifest.entries[1].path, "word/document.xml");
        assert_eq!(manifest.entries[1].size, 8);
    }

    #[test]
    fn rejects_parent_directory_zip_entries() {
        let temp_dir = tempfile::tempdir().unwrap();
        let package = temp_dir.path().join("evil.docx");
        write_zip_entry(&package, "../evil.xml", b"evil");

        let error = unpack_package(&package, temp_dir.path().join("unpacked")).unwrap_err();

        assert!(matches!(error, OoxmlError::UnsafeEntry(entry) if entry == "../evil.xml"));
        assert!(!temp_dir.path().join("evil.xml").exists());
    }

    #[test]
    fn rejects_absolute_zip_entries() {
        let temp_dir = tempfile::tempdir().unwrap();
        let package = temp_dir.path().join("absolute.docx");
        write_zip_entry(&package, "/evil.xml", b"evil");

        let error = unpack_package(&package, temp_dir.path().join("unpacked")).unwrap_err();

        assert!(matches!(error, OoxmlError::UnsafeEntry(entry) if entry == "/evil.xml"));
    }

    #[test]
    fn pack_unpack_preserves_nested_directory_structure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source = temp_dir.path().join("source");
        let unpacked = temp_dir.path().join("unpacked");
        let package = temp_dir.path().join("nested.pptx");
        fs::create_dir_all(source.join("ppt").join("slides").join("_rels")).unwrap();
        fs::create_dir_all(source.join("docProps")).unwrap();
        fs::write(source.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(
            source.join("ppt").join("slides").join("slide1.xml"),
            b"slide",
        )
        .unwrap();
        fs::write(
            source
                .join("ppt")
                .join("slides")
                .join("_rels")
                .join("slide1.xml.rels"),
            b"rels",
        )
        .unwrap();
        fs::write(source.join("docProps").join("core.xml"), b"core").unwrap();

        pack_package(&source, &package).unwrap();
        unpack_package(&package, &unpacked).unwrap();

        assert_eq!(
            fs::read(unpacked.join("[Content_Types].xml")).unwrap(),
            b"types"
        );
        assert_eq!(
            fs::read(unpacked.join("ppt").join("slides").join("slide1.xml")).unwrap(),
            b"slide"
        );
        assert_eq!(
            fs::read(
                unpacked
                    .join("ppt")
                    .join("slides")
                    .join("_rels")
                    .join("slide1.xml.rels")
            )
            .unwrap(),
            b"rels"
        );
        assert_eq!(
            fs::read(unpacked.join("docProps").join("core.xml")).unwrap(),
            b"core"
        );
    }

    #[test]
    fn create_empty_package_writes_valid_packages() {
        let temp_dir = tempfile::tempdir().unwrap();
        for (format, ext) in [("docx", "docx"), ("xlsx", "xlsx"), ("pptx", "pptx")] {
            let path = temp_dir.path().join(format!("blank.{ext}"));
            create_empty_package(format, None, &path).unwrap();

            // Structurally a valid OOXML package (ZIP + [Content_Types].xml).
            assert!(is_ooxml_package(&path), "{format} should be an OOXML package");

            // Contains the content-types marker and the format's root part.
            let manifest = package_manifest(&path).unwrap();
            let paths: Vec<&str> = manifest.entries.iter().map(|e| e.path.as_str()).collect();
            assert!(paths.contains(&"[Content_Types].xml"), "{format} lists [Content_Types].xml");
            let root = match format {
                "docx" => "word/document.xml",
                "xlsx" => "xl/workbook.xml",
                "pptx" => "ppt/presentation.xml",
                _ => unreachable!(),
            };
            assert!(paths.contains(&root), "{format} lists its root part {root}");
        }

        // Case-insensitive format match.
        let upper = temp_dir.path().join("blank.DOCX");
        create_empty_package("DOCX", None, &upper).unwrap();
        assert!(is_ooxml_package(&upper));

        // Unknown format -> UnsupportedFormat (plain-text formats are the
        // caller's responsibility, not this function).
        let err = create_empty_package("txt", None, temp_dir.path().join("blank.txt")).unwrap_err();
        assert!(matches!(err, OoxmlError::UnsupportedFormat(f) if f == "txt"));
    }

    #[test]
    fn create_empty_package_pptx_honors_aspect_ratio() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        // 16:9 -> screen16x9 with the widened 13.333in slide.
        let wide = root.join("wide.pptx");
        create_empty_package("pptx", Some("16:9"), &wide).unwrap();
        let unpacked = root.join("wide");
        unpack_package(&wide, &unpacked).unwrap();
        let presentation =
            fs::read_to_string(unpacked.join("ppt").join("presentation.xml")).unwrap();
        assert!(presentation.contains(r#"type="screen16x9""#), "16:9 slide type");
        assert!(presentation.contains(r#"cx="12192000""#), "16:9 slide width");

        // None -> 4:3 default (unchanged behavior).
        let standard = root.join("standard.pptx");
        create_empty_package("pptx", None, &standard).unwrap();
        let unpacked2 = root.join("standard");
        unpack_package(&standard, &unpacked2).unwrap();
        let presentation2 =
            fs::read_to_string(unpacked2.join("ppt").join("presentation.xml")).unwrap();
        assert!(presentation2.contains(r#"type="screen4x3""#), "4:3 slide type");
        assert!(presentation2.contains(r#"cx="9144000""#), "4:3 slide width");
    }
}
