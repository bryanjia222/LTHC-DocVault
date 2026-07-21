use std::{env, path::PathBuf};

use directories::UserDirs;

use crate::config::read_config_file;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    pub root_dir: PathBuf,
    pub data_dir: PathBuf,
    pub staging_dir: PathBuf,
    /// Durable intake copies for the async commit path: when a commit begins,
    /// the source is copied here (fsynced) before the DB row is written, so a
    /// `pending` version row always has its intake file on disk (the WAL
    /// contract that guarantees no data loss across a crash). Lives outside
    /// `staging_dir` so [`VaultStorage::gc_staging`] (which reclaims stale
    /// backup/restore staging) never touches in-flight intake.
    pub intake_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub repo_dir: PathBuf,
    pub db_path: PathBuf,
    pub config_path: PathBuf,
}

impl VaultPaths {
    /// The default vault root used when no explicit root is configured: a
    /// `.DocVault` directory under the user's home (`~/.DocVault` on macOS/Linux,
    /// `C:\Users\<user>\.DocVault` on Windows). Cross-platform, user-owned, and
    /// conventionally hidden (leading dot), so it is also the recommended location
    /// pre-filled in the first-run connect dialog. The desktop app falls back to
    /// this when the user has not yet chosen a vault.
    pub fn default_root() -> PathBuf {
        default_root_dir()
    }

    /// Build paths for an explicit root, reading data/repo/db locations from
    /// the vault's `config.toml` when present (falling back to root-relative
    /// defaults). No environment variables are consulted: the desktop app uses
    /// this so the on-disk config is the single source of truth.
    pub fn from_root(root_dir: impl Into<PathBuf>) -> Self {
        let root_dir = root_dir.into();
        let config_path = absolute_path(root_dir.join("config.toml"));
        let config = read_config_file(&config_path).ok();
        let data_dir = config
            .as_ref()
            .map(|config| PathBuf::from(&config.storage.data_dir))
            .unwrap_or_else(|| root_dir.join("data"));
        let db_path = config
            .as_ref()
            .map(|config| PathBuf::from(&config.database.path))
            .unwrap_or_else(|| root_dir.join("db.sqlite"));
        let repo_dir = config
            .as_ref()
            .map(|config| PathBuf::from(&config.storage.repo_dir))
            .unwrap_or_else(|| root_dir.join("repo"));
        Self::new_with_repo(root_dir, data_dir, repo_dir, db_path)
    }

    pub fn new(
        root_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
        db_path: impl Into<PathBuf>,
    ) -> Self {
        let root_dir = root_dir.into();
        let repo_dir = root_dir.join("repo");
        Self::new_with_repo(root_dir, data_dir, repo_dir, db_path)
    }

    pub fn new_with_repo(
        root_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
        repo_dir: impl Into<PathBuf>,
        db_path: impl Into<PathBuf>,
    ) -> Self {
        let root_dir = absolute_path(root_dir.into());
        let data_dir = absolute_path(data_dir.into());
        let repo_dir = absolute_path(repo_dir.into());
        let db_path = absolute_path(db_path.into());
        Self {
            staging_dir: data_dir.join("staging"),
            intake_dir: data_dir.join("intake"),
            versions_dir: data_dir.join("versions"),
            cache_dir: root_dir.join("cache"),
            repo_dir,
            config_path: root_dir.join("config.toml"),
            root_dir,
            data_dir,
            db_path,
        }
    }
}

pub(crate) fn absolute_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map(|current_dir| current_dir.join(&path))
            .unwrap_or(path)
    }
}

fn default_root_dir() -> PathBuf {
    UserDirs::new()
        .map(|dirs| dirs.home_dir().join(".DocVault"))
        .unwrap_or_else(|| PathBuf::from(".docvault"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use crate::{BackupBackend, StorageOverrides, config::read_settings};

    use super::*;

    /// `from_root` with no config on disk derives data/db/repo relative to the
    /// root (no environment consulted).
    #[test]
    fn from_root_uses_root_relative_defaults_without_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("vault");

        let paths = VaultPaths::from_root(&root);

        assert_eq!(paths.root_dir, absolute(&root));
        assert_eq!(paths.data_dir, absolute(root.join("data")));
        assert_eq!(paths.db_path, absolute(root.join("db.sqlite")));
        assert_eq!(paths.repo_dir, absolute(root.join("repo")));
    }

    /// `from_root` reads data/repo/db locations from the vault's `config.toml`
    /// when present, so an existing vault keeps its configured layout.
    #[test]
    fn from_root_reads_paths_from_config_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("vault");
        let data_dir = temp_dir.path().join("configured-data");
        let repo_dir = temp_dir.path().join("configured-repo");
        let db_path = temp_dir.path().join("configured-db.sqlite");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("config.toml"),
            format!(
                "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
                config_path(&data_dir),
                config_path(&repo_dir),
                config_path(&db_path)
            ),
        )
        .unwrap();

        let paths = VaultPaths::from_root(&root);

        assert_eq!(paths.data_dir, absolute(data_dir));
        assert_eq!(paths.repo_dir, absolute(repo_dir));
        assert_eq!(paths.db_path, absolute(db_path));
    }

    /// Explicit [`StorageOverrides`] win over the on-disk config for every
    /// field. This is the replacement for the former `DOCVAULT_*` env-var
    /// overrides: the same override semantics, but passed explicitly by the
    /// caller instead of read from the process environment.
    #[test]
    fn read_settings_overrides_win_over_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("vault");
        let config_restic = temp_dir.path().join("config-restic");
        let override_restic = temp_dir.path().join("override-restic");
        let paths = VaultPaths::new_with_repo(
            &root,
            temp_dir.path().join("data"),
            temp_dir.path().join("repo"),
            temp_dir.path().join("db.sqlite"),
        );
        fs::create_dir_all(&root).unwrap();
        fs::write(
            &paths.config_path,
            format!(
                "[storage]\nbackend = \"restic\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\nrestic_path = \"{}\"\nrestic_password = \"from-config\"\n\n[database]\npath = \"{}\"\n",
                config_path(&paths.data_dir),
                config_path(&paths.repo_dir),
                config_path(&config_restic),
                config_path(&paths.db_path)
            ),
        )
        .unwrap();

        let overrides = StorageOverrides {
            backend: Some(BackupBackend::LocalCopy),
            restic_path: Some(override_restic.clone()),
            restic_password: Some("from-override".to_owned()),
        };
        let settings = read_settings(&paths, &overrides).unwrap();

        assert_eq!(settings.backend, BackupBackend::LocalCopy);
        assert_eq!(settings.restic_path, override_restic);
        assert_eq!(settings.restic_password, "from-override");
    }

    /// With default (all-`None`) overrides, `read_settings` reads every field
    /// from the on-disk config - including a configured `restic_path`.
    #[test]
    fn read_settings_uses_config_when_overrides_absent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("vault");
        let config_restic = temp_dir.path().join("config-restic");
        let paths = VaultPaths::new_with_repo(
            &root,
            temp_dir.path().join("data"),
            temp_dir.path().join("repo"),
            temp_dir.path().join("db.sqlite"),
        );
        fs::create_dir_all(&root).unwrap();
        fs::write(
            &paths.config_path,
            format!(
                "[storage]\nbackend = \"restic\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\nrestic_path = \"{}\"\nrestic_password = \"from-config\"\n\n[database]\npath = \"{}\"\n",
                config_path(&paths.data_dir),
                config_path(&paths.repo_dir),
                config_path(&config_restic),
                config_path(&paths.db_path)
            ),
        )
        .unwrap();

        let settings = read_settings(&paths, &StorageOverrides::default()).unwrap();

        assert_eq!(settings.backend, BackupBackend::Restic);
        assert_eq!(settings.restic_path, config_restic);
        assert_eq!(settings.restic_password, "from-config");
    }

    fn absolute(path: impl Into<PathBuf>) -> PathBuf {
        absolute_path(path.into())
    }

    fn config_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }
}
