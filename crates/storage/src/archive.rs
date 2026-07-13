use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use docvault_types::{Document, Version};
use tracing::{debug, info};

use crate::{BackupBackend, ResticError, StorageError, StorageResult, VaultStorage};

#[derive(Debug, Clone)]
pub(crate) struct ArchiveReference {
    pub(crate) backend: BackupBackend,
    pub(crate) reference: String,
    pub(crate) snapshot_id: Option<String>,
}

impl VaultStorage {
    pub(crate) fn archive_source(
        &self,
        document: &Document,
        version_id: &str,
        source_path: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<ArchiveReference> {
        match self.settings.backend {
            BackupBackend::LocalCopy => self.archive_local_copy(document, version_id, source_path),
            BackupBackend::Restic => self.archive_restic(document, version_id, source_path, cancel),
        }
    }

    fn archive_local_copy(
        &self,
        document: &Document,
        version_id: &str,
        source_path: &Path,
    ) -> StorageResult<ArchiveReference> {
        debug!(
            document_id = document.id.as_str(),
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
            .join(document.id.as_str())
            .join(version_id);
        fs::create_dir_all(&version_dir)?;
        let archive_path = version_dir.join(source_name);
        fs::copy(source_path, &archive_path)?;
        let archive_reference = PathBuf::from(document.id.as_str())
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
        document: &Document,
        version_id: &str,
        source_path: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<ArchiveReference> {
        debug!(
            document_id = document.id.as_str(),
            version_id,
            source = %source_path.display(),
            "archiving source with restic backend"
        );
        self.ensure_restic_repo(cancel)?;
        let package_dir = self.restic_package_dir(document, version_id);
        reset_dir(&package_dir)?;
        docvault_ooxml::unpack_package(source_path, &package_dir)?;

        let snapshot_id = self.restic_backup(document, version_id, &package_dir, cancel)?;
        Ok(ArchiveReference {
            backend: BackupBackend::Restic,
            reference: format!("restic:{}:{version_id}", document.id.as_str()),
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
                self.restore_restic_version(document, version, &destination, cancel)?;
            }
        }
        Ok(destination)
    }

    fn restore_restic_version(
        &self,
        document: &Document,
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
            .join(document.id.as_str())
            .join(&version.id);
        reset_dir(&restore_root)?;
        self.restic_restore(snapshot_id, &restore_root, cancel)?;

        let restored_package = restore_root.join("package");
        docvault_ooxml::pack_package(restored_package, destination)?;
        Ok(())
    }

    pub(crate) fn restic_package_dir(&self, document: &Document, version_id: &str) -> PathBuf {
        self.paths
            .staging_dir
            .join("backup")
            .join(document.id.as_str())
            .join(version_id)
            .join("package")
    }
}

pub(crate) fn reset_dir(path: &Path) -> StorageResult<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}
