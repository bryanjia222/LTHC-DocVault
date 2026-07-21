use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use docvault_storage::{DocumentRef, StorageError, StorageResult, VaultStorage};
use docvault_types::{CommitMetadata, Document, DocumentId, Version};
use thiserror::Error;
use tracing::info;

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
        cancel: &AtomicBool,
    ) -> CoreResult<(Document, Version)> {
        let source_path = source_path.as_ref();
        info!(path = %source_path.display(), "starting document commit");

        let result =
            self.storage
                .add_document_version(document_ref, source_path, metadata, cancel)?;
        info!(
            document_id = result.0.id.as_str(),
            version_id = result.1.id.as_str(),
            "completed document commit"
        );
        Ok(result)
    }

    /// Phase A of the async commit: write a durable intake copy of the source
    /// (any document the picker admits — OOXML or raw binary — is accepted here),
    /// and atomically insert the version row as `pending` + repoint the
    /// current-version pointer. Returns the new document + version so the caller
    /// can materialize the library copy and spawn the Phase B archive
    /// ([`Self::archive_pending_version`]). No archive work happens here, so this
    /// is fast and synchronous.
    pub fn begin_commit(
        &self,
        source_path: impl AsRef<Path>,
        document_ref: DocumentRef,
        metadata: CommitMetadata,
    ) -> CoreResult<(Document, Version)> {
        let source_path = source_path.as_ref();
        info!(path = %source_path.display(), "starting document commit (phase A)");
        let result = self
            .storage
            .begin_commit(document_ref, source_path, metadata)?;
        info!(
            document_id = result.0.id.as_str(),
            version_id = result.1.id.as_str(),
            "completed document commit phase A (pending archive)"
        );
        Ok(result)
    }

    /// Phase B of the async commit: archive a `pending` version from its durable
    /// intake copy, finalize the DB row (`archive_reference` + `snapshot_id`,
    /// status -> `archived`), and reclaim the intake. Idempotent, so it is safe
    /// to re-run after a crash (recovery on open does exactly this).
    pub fn archive_pending_version(
        &self,
        version: &Version,
        cancel: &AtomicBool,
    ) -> CoreResult<()> {
        info!(
            document_id = version.document_id.as_str(),
            version_id = version.id.as_str(),
            "starting document commit phase B (archive)"
        );
        self.storage.archive_pending_version(version, cancel)?;
        info!(
            document_id = version.document_id.as_str(),
            version_id = version.id.as_str(),
            "completed document commit phase B (archive)"
        );
        Ok(())
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
        cancel: &AtomicBool,
    ) -> StorageResult<PathBuf> {
        self.storage.export_version(
            document_ref,
            requested_version,
            output_path.as_ref(),
            cancel,
        )
    }

    pub fn checkout_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: Option<impl AsRef<Path>>,
        cancel: &AtomicBool,
    ) -> StorageResult<Option<PathBuf>> {
        self.storage.checkout_version(
            document_ref,
            requested_version,
            output_path.as_ref().map(AsRef::as_ref),
            cancel,
        )
    }

    pub fn current_version(&self, document_ref: &DocumentRef) -> StorageResult<Option<Version>> {
        self.storage.current_version(document_ref)
    }

    /// Delete a document and all of its versions (forgetting restic snapshots
    /// for the restic backend). The user's on-disk source file is not touched.
    pub fn delete_document(
        &self,
        document_ref: &DocumentRef,
        cancel: &AtomicBool,
    ) -> CoreResult<()> {
        self.storage.delete_document(document_ref, cancel)?;
        Ok(())
    }

    /// Rename a document's display name. Does not touch the source file or any
    /// version's `original_filename`.
    pub fn rename_document(
        &self,
        document_ref: &DocumentRef,
        new_name: &str,
    ) -> StorageResult<()> {
        self.storage.rename_document(document_ref, new_name)
    }

    pub fn paths(&self) -> &docvault_storage::VaultPaths {
        self.storage.paths()
    }

    pub fn backend(&self) -> docvault_storage::BackupBackend {
        self.storage.backend()
    }

    /// On-disk size of the backup repository, in bytes (restic raw-data size or
    /// the local-copy archive footprint).
    pub fn repo_size(&self) -> StorageResult<u64> {
        self.storage.repo_size()
    }

    pub fn restic_path(&self) -> &Path {
        self.storage.restic_path()
    }

    /// Cached `restic version` string (empty for local-copy or unavailable).
    pub fn restic_version(&self) -> &str {
        self.storage.restic_version()
    }

    /// Look up a document's display name by id without scanning all documents.
    pub fn document_name(&self, id: &str) -> StorageResult<String> {
        self.storage.document_name(id)
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
    fn commits_raw_binary_documents() {
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

        // A plain text file is not an OOXML package: it must commit as raw
        // binary (no Office-only rejection) and round-trip byte-for-byte.
        let source = root.join("notes.txt");
        std::fs::write(&source, b"plain text, not Office").unwrap();

        let (document, version) = vault
            .commit_document(
                &source,
                DocumentRef::Name("notes".to_owned()),
                CommitMetadata::default(),
                &docvault_storage::NEVER_CANCELLED,
            )
            .expect("non-Office files should commit as raw binary");

        let restored = root.join("restored.txt");
        vault
            .export_version(
                &DocumentRef::Name("notes".to_owned()),
                version.id.as_str(),
                &restored,
                &docvault_storage::NEVER_CANCELLED,
            )
            .expect("export should succeed");
        assert_eq!(
            std::fs::read(&restored).unwrap(),
            b"plain text, not Office"
        );
        assert_eq!(document.current_version_id, Some(version.id));
    }
}
