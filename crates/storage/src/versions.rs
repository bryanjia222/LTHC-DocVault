//! Version workflows: export / checkout / current-pointer reads, plus the
//! document-level queries (`create_document`, `list_documents`, `list_versions`,
//! `document_name`) and the metadata edits (`rename_document`,
//! `set_version_note`). `resolve_requested_version` (private) turns a
//! `"current"`/`"latest"`/explicit-id string into a `Version` row.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use docvault_types::{Document, DocumentId, Version};
use tracing::info;
use uuid::Uuid;

use crate::{DocumentRef, StorageError, StorageResult, VaultStorage};

impl VaultStorage {
    pub fn export_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<PathBuf> {
        let document = self.resolve_document_ref(document_ref)?;
        let version = self.resolve_requested_version(&document, requested_version)?;
        info!(
            document_id = document.id.as_str(),
            version_id = version.id.as_str(),
            output = %output_path.display(),
            "exporting document version"
        );
        self.export_resolved_version(&document, &version, output_path, cancel)
    }

    pub fn checkout_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: Option<&Path>,
        cancel: &AtomicBool,
    ) -> StorageResult<Option<PathBuf>> {
        let document = self.resolve_document_ref(document_ref)?;
        let version = self.resolve_requested_version(&document, requested_version)?;
        info!(
            document_id = document.id.as_str(),
            version_id = version.id.as_str(),
            "checking out document version"
        );
        self.set_current_version(document.id.as_str(), &version.id)?;
        output_path
            .map(|output_path| {
                self.export_resolved_version(&document, &version, output_path, cancel)
            })
            .transpose()
    }

    pub fn current_version(&self, document_ref: &DocumentRef) -> StorageResult<Option<Version>> {
        let document = self.resolve_document_ref(document_ref)?;
        let Some(current_version_id) = document.current_version_id else {
            return Ok(None);
        };
        self.find_version(document.id.as_str(), &current_version_id)
    }

    /// Rename a document's display name. Does not touch the on-disk source file
    /// or any version's `original_filename` (historical).
    pub fn rename_document(&self, document_ref: &DocumentRef, new_name: &str) -> StorageResult<()> {
        let document = self.resolve_document_ref(document_ref)?;
        self.set_document_name(document.id.as_str(), new_name)?;
        info!(
            document_id = document.id.as_str(),
            new_name, "document renamed"
        );
        Ok(())
    }

    /// Update a version's note (its commit message). `None` clears it. The
    /// archive and every other version field are untouched. A missing version
    /// (or one that does not belong to the resolved document) is surfaced as
    /// [`StorageError::VersionNotFound`] rather than a silent no-op.
    pub fn set_version_note(
        &self,
        document_ref: &DocumentRef,
        version_id: &str,
        note: Option<&str>,
    ) -> StorageResult<()> {
        let document = self.resolve_document_ref(document_ref)?;
        if self
            .find_version(document.id.as_str(), version_id)?
            .is_none()
        {
            return Err(StorageError::VersionNotFound {
                document_name: document.name,
                version: version_id.to_owned(),
            });
        }
        self.update_version_note(document.id.as_str(), version_id, note)?;
        info!(
            document_id = document.id.as_str(),
            version_id, "version note updated"
        );
        Ok(())
    }

    pub fn create_document(&self, name: &str, now: i64) -> StorageResult<Document> {
        let document = Document {
            id: DocumentId::new(Uuid::new_v4().to_string()),
            name: name.to_owned(),
            current_version_id: None,
            created_at: now,
        };
        self.insert_document(&document)?;
        Ok(document)
    }

    pub fn list_documents(&self) -> StorageResult<Vec<Document>> {
        self.list_all_documents()
    }

    pub fn list_versions(&self, document_ref: &DocumentRef) -> StorageResult<Vec<Version>> {
        let document = self.resolve_document_ref(document_ref)?;
        self.versions_for_document(document.id.as_str())
    }

    /// Look up a single document's display name by id, without scanning all
    /// documents. `DocumentIdNotFound` when no document has that exact id.
    pub fn document_name(&self, id: &str) -> StorageResult<String> {
        self.document_name_by_id(id)
    }

    fn resolve_requested_version(
        &self,
        document: &Document,
        requested_version: &str,
    ) -> StorageResult<Version> {
        let resolved = if requested_version == "current" {
            match document.current_version_id.as_deref() {
                Some(version_id) => self.find_version(document.id.as_str(), version_id)?,
                None => None,
            }
        } else {
            self.find_version(document.id.as_str(), requested_version)?
        };
        resolved.ok_or_else(|| StorageError::VersionNotFound {
            document_name: document.name.clone(),
            version: requested_version.to_owned(),
        })
    }
}
