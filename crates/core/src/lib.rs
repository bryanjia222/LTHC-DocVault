use std::path::{Path, PathBuf};

use docvault_storage::{StorageError, StorageResult, VaultStorage};
use docvault_types::{Document, DocumentId, ImportMetadata, Version};

#[derive(Debug)]
pub enum CoreError {
    UnsupportedDocument(PathBuf),
    Storage(StorageError),
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedDocument(path) => {
                write!(f, "unsupported Office document: {}", path.display())
            }
            Self::Storage(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<StorageError> for CoreError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

pub fn register_document(name: impl Into<String>, source_path: impl Into<String>) -> Document {
    let name = name.into();
    Document {
        id: DocumentId::new(name.clone()),
        name,
        source_path: source_path.into(),
        created_at: 0,
    }
}

pub struct DocVault {
    storage: VaultStorage,
}

impl DocVault {
    pub fn new(storage: VaultStorage) -> Self {
        Self { storage }
    }

    pub fn import_document(
        &self,
        source_path: impl AsRef<Path>,
        name: &str,
        metadata: ImportMetadata,
    ) -> CoreResult<(Document, Version)> {
        let source_path = source_path.as_ref();
        if !docvault_ooxml::is_supported_ooxml(source_path) {
            return Err(CoreError::UnsupportedDocument(source_path.to_path_buf()));
        }

        Ok(self
            .storage
            .add_document_version(name, source_path, metadata)?)
    }

    pub fn list_documents(&self) -> StorageResult<Vec<Document>> {
        self.storage.list_documents()
    }

    pub fn list_versions(&self, document_name: &str) -> StorageResult<Vec<Version>> {
        self.storage.list_versions(document_name)
    }

    pub fn restore_version(
        &self,
        document_name: &str,
        requested_version: &str,
        output_path: impl AsRef<Path>,
    ) -> StorageResult<PathBuf> {
        self.storage
            .restore_version(document_name, requested_version, output_path.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_document_with_domain_id() {
        let document = register_document("report", "./report.docx");

        assert_eq!(document.id.as_str(), "report");
        assert_eq!(document.name, "report");
        assert_eq!(document.source_path, "./report.docx");
    }

    #[test]
    fn rejects_unsupported_documents() {
        let paths = docvault_storage::VaultPaths::new(
            std::env::temp_dir().join("docvault-core-reject"),
            std::env::temp_dir().join("docvault-core-reject/data"),
            std::env::temp_dir().join("docvault-core-reject/db.sqlite"),
        );
        let storage = VaultStorage::init(paths).unwrap();
        let vault = DocVault::new(storage);

        let error = vault
            .import_document("notes.txt", "notes", ImportMetadata::default())
            .expect_err("txt files should be rejected");

        assert!(matches!(error, CoreError::UnsupportedDocument(_)));
    }
}
