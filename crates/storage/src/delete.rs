//! Deletion workflows: remove a whole document, or specific versions of one.
//! For the restic backend the snapshots are forgotten (and pruned) first so the
//! DB and repo stay consistent; local-copy archive directories are removed
//! best-effort afterwards.

use std::fs;
use std::sync::atomic::AtomicBool;

use docvault_types::Version;
use tracing::{info, warn};

use crate::{BackupBackend, DocumentRef, StorageError, StorageResult, VaultStorage};

impl VaultStorage {
    /// Delete a document and all of its versions. For the restic backend the
    /// version snapshots are forgotten (and pruned) first so the DB and repo
    /// stay consistent; a forget failure aborts the delete (the rows remain,
    /// the user can retry). The local-copy archive directory is removed
    /// best-effort afterwards. The on-disk source file is never touched -
    /// DocVault does not own the user's working copy.
    pub fn delete_document(
        &self,
        document_ref: &DocumentRef,
        cancel: &AtomicBool,
    ) -> StorageResult<()> {
        let document = self.resolve_document_ref(document_ref)?;
        let versions = self.versions_for_document(document.id.as_str())?;
        if self.settings.backend == BackupBackend::Restic {
            let snapshot_ids: Vec<String> = versions
                .iter()
                .filter_map(|version| version.snapshot_id.clone())
                .collect();
            if !snapshot_ids.is_empty() {
                self.restic_forget(&snapshot_ids, cancel)?;
            }
        }
        self.remove_document(document.id.as_str())?;
        // Best-effort: a missing/busy dir is logged, not fatal - the DB rows
        // (the source of truth for existence) are already gone.
        let version_dir = self.paths.versions_dir.join(document.id.as_str());
        if version_dir.exists()
            && let Err(error) = fs::remove_dir_all(&version_dir)
        {
            warn!(
                document_id = document.id.as_str(),
                path = %version_dir.display(),
                error = %error,
                "failed to remove local archive directory after delete"
            );
        }
        info!(document_id = document.id.as_str(), "document deleted");
        Ok(())
    }

    /// Delete specific versions of a document by id, keeping the document and its
    /// other versions. For the restic backend each version's snapshot is
    /// forgotten (and pruned) first - in one batched call - so the DB and repo
    /// stay consistent; a forget failure aborts the delete (the rows remain, the
    /// user can retry). The local-copy archive directory for each version is then
    /// removed best-effort. The on-disk source file, the document, and its other
    /// versions are untouched.
    ///
    /// The caller owns the subtree policy: this deletes exactly the given ids (a
    /// version plus its descendants, when the user confirmed), never reparenting
    /// or orphaning survivors. The current (checked-out) version is refused so
    /// the document's `current_version_id` pointer never dangles.
    pub fn delete_versions(
        &self,
        document_ref: &DocumentRef,
        version_ids: &[String],
        cancel: &AtomicBool,
    ) -> StorageResult<()> {
        if version_ids.is_empty() {
            return Ok(());
        }
        let document = self.resolve_document_ref(document_ref)?;
        if let Some(current) = &document.current_version_id
            && version_ids.iter().any(|id| id == current)
        {
            return Err(StorageError::CannotDeleteCurrentVersion {
                document_name: document.name.clone(),
                version_id: current.clone(),
            });
        }
        let versions = self.versions_for_document(document.id.as_str())?;
        // Resolve each requested id to its row; a stale/unknown id surfaces as
        // VersionNotFound rather than a silent no-op.
        let to_delete: Vec<Version> = version_ids
            .iter()
            .map(|id| {
                versions
                    .iter()
                    .find(|v| &v.id == id)
                    .cloned()
                    .ok_or_else(|| StorageError::VersionNotFound {
                        document_name: document.name.clone(),
                        version: id.clone(),
                    })
            })
            .collect::<StorageResult<Vec<_>>>()?;
        if self.settings.backend == BackupBackend::Restic {
            let snapshot_ids: Vec<String> = to_delete
                .iter()
                .filter_map(|version| version.snapshot_id.clone())
                .collect();
            if !snapshot_ids.is_empty() {
                self.restic_forget(&snapshot_ids, cancel)?;
            }
        }
        self.remove_versions(document.id.as_str(), version_ids)?;
        // Best-effort per-version archive cleanup (a missing/busy dir is logged,
        // not fatal - the DB rows, the source of truth, are already gone). The
        // version's archive lives at `versions_dir/<doc>/<v>/` (local-copy); for
        // the restic backend nothing is written under `versions_dir`, so the dir
        // simply does not exist and this is a no-op. A `pending` version whose
        // archive isn't materialized yet likewise has no dir here.
        for version in &to_delete {
            let version_dir = self
                .paths
                .versions_dir
                .join(document.id.as_str())
                .join(&version.id);
            if version_dir.exists()
                && let Err(error) = fs::remove_dir_all(&version_dir)
            {
                warn!(
                    document_id = document.id.as_str(),
                    version_id = version.id.as_str(),
                    path = %version_dir.display(),
                    error = %error,
                    "failed to remove local archive directory after version delete"
                );
            }
        }
        info!(
            document_id = document.id.as_str(),
            count = to_delete.len(),
            "versions deleted"
        );
        Ok(())
    }
}
