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
}
