use std::path::{Path, PathBuf};

use docvault_storage::{DocumentRef, StorageError, StorageResult, VaultStorage};
use docvault_types::{CommitMetadata, Document, DocumentId, Version};
use thiserror::Error;
use tracing::{error, info};

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unsupported Office document: {}", .0.display())]
    UnsupportedDocument(PathBuf),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub type CoreResult<T> = Result<T, CoreError>;

pub fn register_document(name: impl Into<String>) -> Document {
    let name = name.into();
    Document {
        id: DocumentId::new(name.clone()),
        name,
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
        info!(path = %source_path.display(), "starting document commit");
        if !docvault_ooxml::is_supported_ooxml(source_path) {
            error!(path = %source_path.display(), "unsupported Office document");
            return Err(CoreError::UnsupportedDocument(source_path.to_path_buf()));
        }

        let result = self
            .storage
            .add_document_version(document_ref, source_path, metadata)?;
        info!(
            document_id = result.0.id.as_str(),
            version_id = result.1.id.as_str(),
            "completed document commit"
        );
        Ok(result)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_document_with_domain_id() {
        let document = register_document("report");

        assert_eq!(document.id.as_str(), "report");
        assert_eq!(document.name, "report");
    }

    #[test]
    fn rejects_unsupported_documents() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let paths =
            docvault_storage::VaultPaths::new(root, root.join("data"), root.join("db.sqlite"));
        std::fs::create_dir_all(&paths.root_dir).unwrap();
        std::fs::write(
            &paths.config_path,
            format!(
                "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
                paths.data_dir.display().to_string().replace('\\', "/"),
                paths.repo_dir.display().to_string().replace('\\', "/"),
                paths.db_path.display().to_string().replace('\\', "/")
            ),
        )
        .unwrap();
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
