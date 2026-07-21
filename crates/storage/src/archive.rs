use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use docvault_types::{Document, Version};
use tracing::{debug, info, warn};

use crate::{
    BackupBackend, ResticError, StorageError, StorageResult, VaultStorage,
    ARCHIVE_STATUS_PENDING,
};

#[derive(Debug, Clone)]
pub(crate) struct ArchiveReference {
    pub(crate) backend: BackupBackend,
    pub(crate) reference: String,
    pub(crate) snapshot_id: Option<String>,
}

impl VaultStorage {
    pub(crate) fn archive_source(
        &self,
        document_id: &str,
        version_id: &str,
        source_path: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<ArchiveReference> {
        match self.settings.backend {
            BackupBackend::LocalCopy => self.archive_local_copy(document_id, version_id, source_path),
            BackupBackend::Restic => self.archive_restic(document_id, version_id, source_path, cancel),
        }
    }

    fn archive_local_copy(
        &self,
        document_id: &str,
        version_id: &str,
        source_path: &Path,
    ) -> StorageResult<ArchiveReference> {
        debug!(
            document_id,
            version_id,
            source = %source_path.display(),
            "archiving source with local copy backend"
        );
        let source_name = source_path
            .file_name()
            .ok_or_else(|| StorageError::InvalidFileName(source_path.to_path_buf()))?;
        let version_dir = self
            .paths
            .versions_dir
            .join(document_id)
            .join(version_id);
        fs::create_dir_all(&version_dir)?;
        let archive_path = version_dir.join(source_name);
        fs::copy(source_path, &archive_path)?;
        let archive_reference = PathBuf::from(document_id)
            .join(version_id)
            .join(source_name)
            .display()
            .to_string();
        Ok(ArchiveReference {
            backend: BackupBackend::LocalCopy,
            reference: archive_reference,
            snapshot_id: None,
        })
    }

    fn archive_restic(
        &self,
        document_id: &str,
        version_id: &str,
        source_path: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<ArchiveReference> {
        debug!(
            document_id,
            version_id,
            source = %source_path.display(),
            "archiving source with restic backend"
        );
        self.ensure_restic_repo(cancel)?;
        let tag = format!("docvault:{document_id}:{version_id}");
        // Idempotent: if a previous archive attempt (interrupted by a crash or
        // a re-run of recovery) already produced a snapshot for this tag, reuse
        // it. Restic tags each backup with `docvault:<doc>:<version>`, so this
        // is the deduplication key that keeps recovery from stacking duplicate
        // snapshots for the same version.
        if let Some(existing) = self.restic_snapshot_id_for_tag(&tag, cancel)? {
            info!(
                document_id,
                version_id, snapshot_id = existing.as_str(), "restic snapshot already exists for version; reusing"
            );
            return Ok(ArchiveReference {
                backend: BackupBackend::Restic,
                reference: format!("restic:{document_id}:{version_id}"),
                snapshot_id: Some(existing),
            });
        }
        let package_dir = self.restic_package_dir(document_id, version_id);
        reset_dir(&package_dir)?;
        if docvault_ooxml::is_ooxml_package(source_path) {
            docvault_ooxml::unpack_package(source_path, &package_dir)?;
        } else {
            // Non-OOXML file (pdf, md, txt, a legacy Kingsoft `.wps`/`.et`/`.dps`
            // binary, ...): store the whole file verbatim as the lone entry under
            // the package dir so restic captures it exactly as it captures an
            // unzipped Office package. Restore detects this via the absence of
            // `[Content_Types].xml` and copies the file back out instead of
            // re-zipping.
            let source_name = source_path
                .file_name()
                .ok_or_else(|| StorageError::InvalidFileName(source_path.to_path_buf()))?;
            fs::copy(source_path, package_dir.join(source_name))?;
        }

        let snapshot_id = self.restic_backup(document_id, version_id, &package_dir, cancel)?;
        // The unzipped package existed only so restic could capture it. Drop it
        // (and the per-version staging dir) now so staging doesn't accumulate a
        // full copy of every committed version.
        if let Some(version_staging) = package_dir.parent() {
            clean_dir_best_effort(version_staging);
        }
        Ok(ArchiveReference {
            backend: BackupBackend::Restic,
            reference: format!("restic:{document_id}:{version_id}"),
            snapshot_id: Some(snapshot_id),
        })
    }

    fn restore_destination(&self, version: &Version, output_path: &Path) -> StorageResult<PathBuf> {
        if output_path.extension().is_some() {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)?;
            }
            Ok(output_path.to_path_buf())
        } else {
            fs::create_dir_all(output_path)?;
            Ok(output_path.join(&version.original_filename))
        }
    }

    pub(crate) fn export_resolved_version(
        &self,
        document: &Document,
        version: &Version,
        output_path: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<PathBuf> {
        let destination = self.restore_destination(version, output_path)?;
        // A `pending` version's compressed archive is not finished yet, but its
        // durable intake copy (a verbatim Office package of the committed
        // source) is on disk. Serve exports/materializations from it so a
        // just-committed version is immediately openable/checkoutable while the
        // archive job is still running (or was interrupted by a crash).
        if version.archive_status == ARCHIVE_STATUS_PENDING {
            let intake = self.intake_path(
                version.document_id.as_str(),
                &version.id,
                &version.original_filename,
            );
            if !intake.exists() {
                return Err(StorageError::IntakeMissing {
                    document_id: version.document_id.as_str().to_owned(),
                    version_id: version.id.clone(),
                });
            }
            fs::copy(&intake, &destination)?;
            return Ok(destination);
        }
        info!(
            document_id = document.id.as_str(),
            version_id = version.id.as_str(),
            backend = version.backup_backend.as_str(),
            destination = %destination.display(),
            "restoring archived version"
        );
        match BackupBackend::parse(&version.backup_backend)? {
            BackupBackend::LocalCopy => {
                fs::copy(
                    self.paths.versions_dir.join(&version.archive_reference),
                    &destination,
                )?;
            }
            BackupBackend::Restic => {
                self.restore_restic_version(version, &destination, cancel)?;
            }
        }
        Ok(destination)
    }

    fn restore_restic_version(
        &self,
        version: &Version,
        destination: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<()> {
        let snapshot_id = version
            .snapshot_id
            .as_deref()
            .ok_or(ResticError::SnapshotMissing)?;
        let restore_root = self
            .paths
            .staging_dir
            .join("restore")
            .join(version.document_id.as_str())
            .join(&version.id);
        reset_dir(&restore_root)?;
        self.restic_restore(snapshot_id, &restore_root, cancel)?;

        let restored_package = restore_root.join("package");
        Self::materialize_restored_package(&restored_package, &version.original_filename, destination)?;
        // The restored package was only needed to re-zip into the destination;
        // drop it so staging doesn't leak across exports/checkouts.
        clean_dir_best_effort(&restore_root);
        Ok(())
    }

    pub(crate) fn restic_package_dir(&self, document_id: &str, version_id: &str) -> PathBuf {
        self.paths
            .staging_dir
            .join("backup")
            .join(document_id)
            .join(version_id)
            .join("package")
    }

    /// Turn a restic-restored `package` directory back into a single destination
    /// file. An OOXML archive was stored unzipped (its parts, including
    /// `[Content_Types].xml`, are the dir's contents) and is re-zipped; a
    /// raw-binary archive was stored verbatim as one file and is copied straight
    /// out. The presence of `[Content_Types].xml` is the content-based signal
    /// (no DB schema change needed) that distinguishes the two.
    pub(crate) fn materialize_restored_package(
        restored_package: &Path,
        original_filename: &str,
        destination: &Path,
    ) -> StorageResult<()> {
        if restored_package.join("[Content_Types].xml").exists() {
            docvault_ooxml::pack_package(restored_package, destination)?;
        } else {
            let restored_file = restored_package.join(original_filename);
            fs::copy(&restored_file, destination)?;
        }
        Ok(())
    }

    /// Reclaim orphan staging left behind by crashed or interrupted
    /// archive/restore operations. Safe to run at startup because no operation
    /// is in flight then: anything under `staging/backup` or `staging/restore`
    /// is by definition stale. Intake (used by the async commit path) lives
    /// outside these dirs and is preserved.
    pub fn gc_staging(&self) {
        for sub in ["backup", "restore"] {
            clean_dir_best_effort(&self.paths.staging_dir.join(sub));
        }
    }

    /// The durable intake path for a pending version:
    /// `data/intake/<docId>/<versionId>/<original_filename>`. The filename is
    /// preserved so the intake copy is a verbatim, openable Office package.
    pub(crate) fn intake_path(
        &self,
        document_id: &str,
        version_id: &str,
        original_filename: &str,
    ) -> PathBuf {
        self.paths
            .intake_dir
            .join(document_id)
            .join(version_id)
            .join(original_filename)
    }

    /// Copy `source` to its intake path and fsync it, so the bytes are on disk
    /// before the `pending` DB row is committed. This is the write-ahead leg of
    /// the crash-safety contract: once the DB says a version is pending, its
    /// intake copy is guaranteed durable, so no commit data is ever lost to a
    /// crash between the copy and the DB write (or during the archive job).
    pub(crate) fn write_intake(&self, source: &Path, intake: &Path) -> StorageResult<()> {
        if let Some(parent) = intake.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, intake)?;
        // Open read+write (not read-only) before fsyncing: on Windows,
        // FlushFileBuffers requires GENERIC_WRITE, so a read-only handle fails
        // with ERROR_ACCESS_DENIED. Opening with write access does not modify
        // the file - sync_all just flushes its data to disk.
        let file = fs::OpenOptions::new().read(true).write(true).open(intake)?;
        file.sync_all()?;
        Ok(())
    }

    /// Phase B of the async commit: archive a `pending` version from its
    /// durable intake copy, then finalize the DB row (`archive_reference` +
    /// `snapshot_id`, status -> `archived`) and reclaim the intake. Idempotent
    /// - restic reuses an existing snapshot for the tag, and the local-copy
    /// copy overwrites - so re-running after a crash never duplicates work.
    /// The intake copy is the source of truth until the archive is finalized;
    /// it is deleted only after the DB row is flipped to `archived`, so a crash
    /// between archiving and the DB update leaves the intake in place for the
    /// next recovery.
    pub fn archive_pending_version(
        &self,
        version: &Version,
        cancel: &AtomicBool,
    ) -> StorageResult<()> {
        let document_id = version.document_id.as_str();
        let intake =
            self.intake_path(document_id, &version.id, &version.original_filename);
        if !intake.exists() {
            return Err(StorageError::IntakeMissing {
                document_id: document_id.to_owned(),
                version_id: version.id.clone(),
            });
        }
        info!(
            document_id,
            version_id = version.id.as_str(),
            "archiving pending version from intake"
        );
        let archive = self.archive_source(document_id, &version.id, &intake, cancel)?;
        self.set_version_archived(
            document_id,
            &version.id,
            &archive.reference,
            archive.snapshot_id.as_deref(),
        )?;
        // The archive is now the source of truth; the intake copy is
        // reclaimable. Best-effort: gc_intake also sweeps it on the next open.
        if let Err(error) = fs::remove_file(&intake) {
            warn!(
                path = %intake.display(),
                error = %error,
                "failed to delete intake copy after archive; will be reclaimed by gc_intake"
            );
        }
        info!(
            document_id,
            version_id = version.id.as_str(),
            "pending version archived"
        );
        Ok(())
    }

    /// Reclaim orphan intake copies whose version row is no longer `pending`:
    /// versions that were archived (intake deletion raced with a crash), and
    /// versions whose document was deleted (the row is gone). Intake for a
    /// still-pending version - an archive in flight, or one waiting for the
    /// next recovery - is preserved. Safe at startup: no archive is in flight
    /// then except the recovery that runs immediately before this.
    pub fn gc_intake(&self) {
        let Ok(doc_entries) = fs::read_dir(&self.paths.intake_dir) else {
            return;
        };
        for doc_dir in doc_entries.flatten() {
            let Some(doc_id) = doc_dir.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(version_entries) = fs::read_dir(doc_dir.path()) else {
                continue;
            };
            for version_dir in version_entries.flatten() {
                let Some(version_id) = version_dir.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                let still_pending = self
                    .is_version_pending(&doc_id, &version_id)
                    .unwrap_or(false);
                if !still_pending {
                    if let Err(error) = fs::remove_dir_all(version_dir.path()) {
                        warn!(
                            path = %version_dir.path().display(),
                            error = %error,
                            "failed to remove orphan intake directory"
                        );
                    }
                }
            }
            // Drop the per-document intake dir once it is empty.
            if fs::read_dir(doc_dir.path())
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false)
            {
                let _ = fs::remove_dir(doc_dir.path());
            }
        }
    }
}

pub(crate) fn reset_dir(path: &Path) -> StorageResult<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}

/// Remove a directory best-effort: a missing or busy dir is logged, not
/// propagated, so a cleanup failure can never fail an otherwise-successful
/// archive/restore.
fn clean_dir_best_effort(path: &Path) {
    if !path.exists() {
        return;
    }
    if let Err(error) = fs::remove_dir_all(path) {
        warn!(
            path = %path.display(),
            error = %error,
            "failed to clean up staging directory"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_restored_package_copies_raw_binary_verbatim() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        // A restic-restored raw-binary "package" dir: the single committed file
        // stored verbatim, no [Content_Types].xml.
        let restored_package = root.join("package");
        fs::create_dir_all(&restored_package).unwrap();
        fs::write(restored_package.join("notes.txt"), b"raw bytes").unwrap();

        let destination = root.join("out").join("notes.txt");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        VaultStorage::materialize_restored_package(&restored_package, "notes.txt", &destination)
            .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"raw bytes");
    }

    #[test]
    fn materialize_restored_package_rezips_ooxml_package() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        // A restic-restored OOXML package dir: unzipped parts including
        // [Content_Types].xml at the root.
        let restored_package = root.join("package");
        fs::create_dir_all(restored_package.join("word")).unwrap();
        fs::write(restored_package.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(restored_package.join("word").join("document.xml"), b"doc").unwrap();

        let destination = root.join("out.docx");
        VaultStorage::materialize_restored_package(&restored_package, "report.docx", &destination)
            .unwrap();
        assert!(
            docvault_ooxml::is_ooxml_package(&destination),
            "OOXML package re-zipped on restore"
        );
    }
}
