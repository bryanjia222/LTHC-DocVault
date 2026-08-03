//! Vault construction, open, and accessors. The init/open lifecycle creates
//! the on-disk layout + SQLite connection, probes restic once, and runs the
//! startup GC/recovery; the accessors expose paths, backend, repo size, and the
//! cached restic version.

use std::fs;
use std::path::Path;

use rusqlite::Connection;
use tracing::{debug, info, warn};

use crate::config;
use crate::{
    BackupBackend, NEVER_CANCELLED, StorageOverrides, StorageResult, VaultPaths, VaultStorage,
};

impl VaultStorage {
    pub fn init(paths: VaultPaths) -> StorageResult<Self> {
        Self::init_with_overrides(paths, &StorageOverrides::default())
    }

    /// Initialize a vault, applying caller-supplied `overrides` on top of the
    /// on-disk `config.toml` (see [`StorageOverrides`]). Used by the desktop
    /// (to inject the bundled restic path) and the CLI (for `--restic-path`);
    /// [`VaultStorage::init`] is the no-override convenience for tests and
    /// config-only callers.
    pub fn init_with_overrides(
        paths: VaultPaths,
        overrides: &StorageOverrides,
    ) -> StorageResult<Self> {
        info!(root_dir = %paths.root_dir.display(), "initializing vault storage");
        fs::create_dir_all(&paths.root_dir)?;
        fs::create_dir_all(&paths.data_dir)?;
        fs::create_dir_all(&paths.staging_dir)?;
        fs::create_dir_all(&paths.versions_dir)?;
        fs::create_dir_all(&paths.cache_dir)?;
        fs::create_dir_all(&paths.repo_dir)?;
        if let Some(parent) = paths.db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if !paths.config_path.exists() {
            fs::write(&paths.config_path, config::default_config(&paths)?)?;
        }

        let settings = config::read_settings(&paths, overrides)?;
        let connection = Connection::open(&paths.db_path)?;
        let mut storage = Self {
            paths,
            settings,
            connection,
            restic_version: String::new(),
        };
        storage.migrate()?;
        if storage.settings.backend == BackupBackend::Restic {
            storage.ensure_restic_repo(&NEVER_CANCELLED)?;
            storage.restic_version = storage.capture_restic_version();
        }
        info!(root_dir = %storage.paths.root_dir.display(), "vault storage initialized");
        Ok(storage)
    }

    pub fn open(paths: VaultPaths) -> StorageResult<Self> {
        Self::open_with_overrides(paths, &StorageOverrides::default())
    }

    /// Open an existing vault, applying caller-supplied `overrides` on top of
    /// the on-disk `config.toml` (see [`StorageOverrides`]). Used by the desktop
    /// (bundled restic path) and the CLI (`--restic-path`); [`VaultStorage::open`]
    /// is the no-override convenience.
    pub fn open_with_overrides(
        paths: VaultPaths,
        overrides: &StorageOverrides,
    ) -> StorageResult<Self> {
        debug!(root_dir = %paths.root_dir.display(), "opening vault storage");
        let settings = config::read_settings(&paths, overrides)?;
        let connection = Connection::open(&paths.db_path)?;
        let mut storage = Self {
            paths,
            settings,
            connection,
            restic_version: String::new(),
        };
        storage.migrate()?;
        if storage.settings.backend == BackupBackend::Restic {
            storage.restic_version = storage.capture_restic_version();
        }
        // Reclaim staging left behind by a crash/interruption in a previous
        // session. No operation is in flight at open, so anything here is stale.
        storage.gc_staging();
        // Finish any commit whose archive (Phase B) a crash interrupted. The
        // intake copy is durable, so the data is safe; this just completes the
        // compress step idempotently. Best-effort: a failure here is logged but
        // does not block opening the vault - the version stays `pending` and is
        // retried on the next open.
        if let Err(error) = storage.recover_pending(&NEVER_CANCELLED) {
            warn!(error = %error, "startup recovery of pending versions failed; leaving them pending");
        }
        Ok(storage)
    }

    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub fn backend(&self) -> BackupBackend {
        self.settings.backend
    }

    /// On-disk size of the backup repository, in bytes.
    /// - restic: `restic stats --mode raw-data` (post-dedup + compression).
    /// - local-copy: total size of the archived version files under
    ///   `versions_dir` (the local-copy "repo" is just those copies).
    pub fn repo_size(&self) -> StorageResult<u64> {
        match self.settings.backend {
            BackupBackend::Restic => self.restic_raw_data_size(&NEVER_CANCELLED),
            BackupBackend::LocalCopy => Ok(dir_size(&self.paths.versions_dir)),
        }
    }

    pub fn restic_path(&self) -> &Path {
        &self.settings.restic_path
    }

    /// The cached `restic version` string (empty for local-copy or when restic
    /// is unavailable). Captured once at init/open.
    pub fn restic_version(&self) -> &str {
        &self.restic_version
    }
}

/// Recursively sum the sizes of all files under `path`. A missing directory
/// (or one that can't be read) contributes 0 rather than failing, so a repo
/// size read never errors on a transiently-unreadable subtree.
fn dir_size(path: &Path) -> u64 {
    fn walk(path: &Path) -> u64 {
        let mut total = 0u64;
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += walk(&path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
        total
    }
    walk(path)
}
