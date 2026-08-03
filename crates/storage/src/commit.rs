//! Commit workflows: the synchronous add/commit path, the async two-phase
//! commit (Phase A intake + `pending` row, Phase B archive via
//! [`archive::archive_pending_version`]), and startup crash recovery of
//! versions left `pending` by an interrupted Phase B.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use docvault_types::{CommitMetadata, Document, DocumentId, Version};
use rusqlite::params;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::sqlite;
use crate::{
    ARCHIVE_STATUS_ARCHIVED, ARCHIVE_STATUS_PENDING, DocumentRef, StorageError, StorageResult,
    VaultStorage, unix_timestamp,
};

impl VaultStorage {
    pub fn add_document_version(
        &self,
        document_ref: DocumentRef,
        source_path: &Path,
        metadata: CommitMetadata,
        cancel: &AtomicBool,
    ) -> StorageResult<(Document, Version)> {
        let now = unix_timestamp();
        info!(source = %source_path.display(), "adding document version");
        let document = match document_ref {
            DocumentRef::NewName(name) => {
                let document = Document {
                    id: DocumentId::new(Uuid::new_v4().to_string()),
                    name,
                    current_version_id: None,
                    created_at: now,
                };
                self.insert_document(&document)?;
                document
            }
            DocumentRef::Name(name) => match self.find_documents_by_name(&name)?.as_slice() {
                [] => {
                    if name.contains('@') {
                        return Err(StorageError::DocumentNotFound(name));
                    }
                    let document = Document {
                        id: DocumentId::new(Uuid::new_v4().to_string()),
                        name,
                        current_version_id: None,
                        created_at: now,
                    };
                    self.insert_document(&document)?;
                    document
                }
                [document] => document.clone(),
                matches => {
                    return Err(StorageError::AmbiguousDocumentName {
                        name,
                        matches: matches.to_vec(),
                    });
                }
            },
            other => self.resolve_document_ref(&other)?,
        };

        self.add_version_to_document(document, source_path, metadata, now, cancel)
    }

    pub fn add_document_version_to_name_or_create(
        &self,
        name: &str,
        source_path: &Path,
        metadata: CommitMetadata,
        cancel: &AtomicBool,
    ) -> StorageResult<(Document, Version)> {
        self.add_document_version(
            DocumentRef::Name(name.to_owned()),
            source_path,
            metadata,
            cancel,
        )
    }

    /// Phase A of the async commit: resolve the document (creating it if new),
    /// write a durable intake copy of the source (fsynced), then atomically
    /// insert the version row as `pending` and repoint the document's
    /// current-version pointer at it - all before any archiving. Returns the
    /// new document + version so the caller can materialize the library copy
    /// and spawn the Phase B archive job ([`Self::archive_pending_version`]).
    ///
    /// Crash safety: the intake copy is fsynced BEFORE the DB transaction
    /// commits, so a `pending` row always has its intake on disk (no commit
    /// data is lost to a crash). The version insert + current-pointer update
    /// run in one transaction, so the version either exists fully (visible and
    /// current) or not at all. The archive runs later and is idempotent, so a
    /// crash at any point is recovered on the next open by
    /// [`Self::recover_pending`].
    pub fn begin_commit(
        &self,
        document_ref: DocumentRef,
        source_path: &Path,
        metadata: CommitMetadata,
    ) -> StorageResult<(Document, Version)> {
        let now = unix_timestamp();
        info!(source = %source_path.display(), "beginning async document commit (phase A)");
        let document = match document_ref {
            DocumentRef::NewName(name) => {
                let document = Document {
                    id: DocumentId::new(Uuid::new_v4().to_string()),
                    name,
                    current_version_id: None,
                    created_at: now,
                };
                self.insert_document(&document)?;
                document
            }
            DocumentRef::Name(name) => match self.find_documents_by_name(&name)?.as_slice() {
                [] => {
                    if name.contains('@') {
                        return Err(StorageError::DocumentNotFound(name));
                    }
                    let document = Document {
                        id: DocumentId::new(Uuid::new_v4().to_string()),
                        name,
                        current_version_id: None,
                        created_at: now,
                    };
                    self.insert_document(&document)?;
                    document
                }
                [document] => document.clone(),
                matches => {
                    return Err(StorageError::AmbiguousDocumentName {
                        name,
                        matches: matches.to_vec(),
                    });
                }
            },
            other => self.resolve_document_ref(&other)?,
        };

        let number = self.next_version_number(document.id.as_str())?;
        let version_id = format!("v{number}");
        let manifest = docvault_ooxml::manifest_for(source_path)?;
        let original_filename = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StorageError::InvalidFileName(source_path.to_path_buf()))?
            .to_owned();

        // 1. Durable intake copy (fsynced) BEFORE the DB commit (WAL invariant:
        // a `pending` row => its intake bytes are on disk).
        let intake = self.intake_path(document.id.as_str(), &version_id, &original_filename);
        self.write_intake(source_path, &intake)?;

        // 2. Atomic transaction: insert the version row as `pending` (with a
        //    placeholder archive_reference filled in by Phase B) and repoint the
        //    current-version pointer. Either both land or neither does.
        let version = Version {
            id: version_id,
            document_id: document.id.clone(),
            number,
            original_filename,
            archive_reference: String::new(),
            backup_backend: self.settings.backend.as_str().to_owned(),
            snapshot_id: None,
            manifest,
            parent_version_id: document.current_version_id.clone(),
            author: metadata.author,
            note: metadata.note,
            created_at: now,
            archive_status: ARCHIVE_STATUS_PENDING.to_owned(),
        };
        let transaction = self.connection.unchecked_transaction()?;
        sqlite::insert_version_into(&transaction, &version)?;
        transaction.execute(
            "UPDATE documents SET current_version_id = ?1 WHERE id = ?2",
            params![&version.id, document.id.as_str()],
        )?;
        transaction.commit()?;

        let mut updated_document = document;
        updated_document.current_version_id = Some(version.id.clone());
        info!(
            document_id = updated_document.id.as_str(),
            version_id = version.id.as_str(),
            "document version committed (pending archive)"
        );
        Ok((updated_document, version))
    }

    /// Startup recovery: finish the Phase B archive for every `pending`
    /// version (each is idempotent, so a crash mid-archive is completed, not
    /// duplicated), then reclaim orphan intake copies. A pending version whose
    /// intake copy is missing (violating the WAL invariant) is left pending and
    /// logged - it cannot be archived without its source, but the rest of the
    /// vault stays usable. Returns the count of versions recovered (archived
    /// this run) so the caller can surface it.
    pub fn recover_pending(&self, cancel: &AtomicBool) -> StorageResult<usize> {
        let pending = self.pending_versions()?;
        if pending.is_empty() {
            // Still sweep orphans (e.g. intake left by a crash after the DB
            // row was archived but before the intake file was deleted).
            self.gc_intake();
            return Ok(0);
        }
        let mut recovered = 0;
        for version in &pending {
            match self.archive_pending_version(version, cancel) {
                Ok(()) => recovered += 1,
                Err(StorageError::IntakeMissing {
                    document_id,
                    version_id,
                }) => {
                    warn!(
                        document_id,
                        version_id,
                        "pending version has no intake copy; left pending for manual review"
                    );
                }
                Err(error) => return Err(error),
            }
        }
        self.gc_intake();
        if recovered > 0 {
            info!(recovered, "recovered pending versions on open");
        }
        Ok(recovered)
    }

    pub(crate) fn add_version_to_document(
        &self,
        document: Document,
        source_path: &Path,
        metadata: CommitMetadata,
        now: i64,
        cancel: &AtomicBool,
    ) -> StorageResult<(Document, Version)> {
        let number = self.next_version_number(document.id.as_str())?;
        let version_id = format!("v{number}");
        debug!(
            document_id = document.id.as_str(),
            version_id = version_id.as_str(),
            "archiving source for document version"
        );
        let manifest = docvault_ooxml::manifest_for(source_path)?;
        let archive =
            self.archive_source(document.id.as_str(), &version_id, source_path, cancel)?;
        let original_filename = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StorageError::InvalidFileName(source_path.to_path_buf()))?
            .to_owned();
        let version = Version {
            id: version_id,
            document_id: document.id.clone(),
            number,
            original_filename,
            archive_reference: archive.reference,
            backup_backend: archive.backend.as_str().to_owned(),
            snapshot_id: archive.snapshot_id,
            manifest,
            parent_version_id: document.current_version_id.clone(),
            author: metadata.author,
            note: metadata.note,
            created_at: now,
            archive_status: ARCHIVE_STATUS_ARCHIVED.to_owned(),
        };
        self.insert_version(&version)?;
        self.set_current_version(document.id.as_str(), &version.id)?;
        let mut updated_document = document;
        updated_document.current_version_id = Some(version.id.clone());
        info!(
            document_id = updated_document.id.as_str(),
            version_id = version.id.as_str(),
            backend = version.backup_backend.as_str(),
            "document version added"
        );
        Ok((updated_document, version))
    }
}
