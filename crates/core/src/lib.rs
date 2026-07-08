use std::path::{Path, PathBuf};

use docvault_storage::{DocumentRef, StorageError, StorageResult, VaultStorage};
use docvault_types::{CommitMetadata, Document, DocumentId, TrackedPath, TrackedScan, Version};

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
        current_version_id: None,
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

    pub fn commit_document(
        &self,
        source_path: impl AsRef<Path>,
        document_ref: DocumentRef,
        metadata: CommitMetadata,
    ) -> CoreResult<(Document, Version)> {
        let source_path = source_path.as_ref();
        if !docvault_ooxml::is_supported_ooxml(source_path) {
            return Err(CoreError::UnsupportedDocument(source_path.to_path_buf()));
        }

        Ok(self
            .storage
            .add_document_version(document_ref, source_path, metadata)?)
    }

    pub fn list_documents(&self) -> StorageResult<Vec<Document>> {
        self.storage.list_documents()
    }

    pub fn list_versions(&self, document_ref: &DocumentRef) -> StorageResult<Vec<Version>> {
        self.storage.list_versions(document_ref)
    }

    pub fn export_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: impl AsRef<Path>,
    ) -> StorageResult<PathBuf> {
        self.storage
            .export_version(document_ref, requested_version, output_path.as_ref())
    }

    pub fn checkout_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: Option<impl AsRef<Path>>,
    ) -> StorageResult<Option<PathBuf>> {
        self.storage.checkout_version(
            document_ref,
            requested_version,
            output_path.as_ref().map(AsRef::as_ref),
        )
    }

    pub fn current_version(&self, document_ref: &DocumentRef) -> StorageResult<Option<Version>> {
        self.storage.current_version(document_ref)
    }

    pub fn track_path(
        &self,
        path: impl AsRef<Path>,
        document_ref: Option<&DocumentRef>,
    ) -> StorageResult<TrackedPath> {
        self.storage.track_path(path.as_ref(), document_ref)
    }

    pub fn track_document_path(
        &self,
        path: impl AsRef<Path>,
        document_id: Option<&DocumentId>,
    ) -> StorageResult<TrackedPath> {
        self.storage.track_document_path(path.as_ref(), document_id)
    }

    pub fn scan_tracked_paths(&self, deep: bool) -> StorageResult<Vec<TrackedScan>> {
        self.storage.scan_tracked_paths(deep)
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
            .commit_document(
                "notes.txt",
                DocumentRef::Name("notes".to_owned()),
                CommitMetadata::default(),
            )
            .expect_err("txt files should be rejected");

        assert!(matches!(error, CoreError::UnsupportedDocument(_)));
    }
}
