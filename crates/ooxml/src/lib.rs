use std::{
    fs::{self, File},
    io::{self, Read, Seek, Write},
    path::{Component, Path, PathBuf},
};

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
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            writer.write_all(&buffer)?;
        }
    }

    Ok(())
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
}
