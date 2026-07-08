use std::{env, path::PathBuf};

use directories::ProjectDirs;

use crate::config::read_config_file;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    pub root_dir: PathBuf,
    pub data_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub repo_dir: PathBuf,
    pub db_path: PathBuf,
    pub config_path: PathBuf,
}

impl VaultPaths {
    pub fn from_env() -> Self {
        let root_dir = env::var_os("DOCVAULT_ROOT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(default_root_dir);
        let config_path = absolute_path(root_dir.join("config.toml"));
        let config = read_config_file(&config_path).ok();
        let data_dir = env::var_os("DOCVAULT_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                config
                    .as_ref()
                    .map(|config| PathBuf::from(&config.storage.data_dir))
            })
            .unwrap_or_else(|| root_dir.join("data"));
        let db_path = env::var_os("DOCVAULT_DB_PATH")
            .map(PathBuf::from)
            .or_else(|| {
                config
                    .as_ref()
                    .map(|config| PathBuf::from(&config.database.path))
            })
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
    ProjectDirs::from("com", "LTHC", "DocVault")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".docvault"))
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
        sync::Mutex,
    };

    use crate::{BackupBackend, config::read_settings};

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn docvault_root_dir_overrides_default_root() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        set_env("DOCVAULT_ROOT_DIR", temp_dir.path());
        remove_env("DOCVAULT_DATA_DIR");
        remove_env("DOCVAULT_DB_PATH");

        let paths = VaultPaths::from_env();

        assert_eq!(paths.root_dir, absolute(temp_dir.path()));
        assert_eq!(paths.data_dir, absolute(temp_dir.path().join("data")));
        assert_eq!(paths.db_path, absolute(temp_dir.path().join("db.sqlite")));
        remove_docvault_env();
    }

    #[test]
    fn config_file_paths_are_used_by_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
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
        set_env("DOCVAULT_ROOT_DIR", &root);
        remove_env("DOCVAULT_DATA_DIR");
        remove_env("DOCVAULT_DB_PATH");

        let paths = VaultPaths::from_env();

        assert_eq!(paths.data_dir, absolute(data_dir));
        assert_eq!(paths.repo_dir, absolute(repo_dir));
        assert_eq!(paths.db_path, absolute(db_path));
        remove_docvault_env();
    }

    #[test]
    fn env_overrides_config_backend_and_restic_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("vault");
        let config_restic = temp_dir.path().join("config-restic");
        let env_restic = temp_dir.path().join("env-restic");
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
        set_env("DOCVAULT_BACKUP_BACKEND", "local-copy");
        set_env("DOCVAULT_RESTIC_PATH", &env_restic);
        set_env("DOCVAULT_RESTIC_PASSWORD", "from-env");

        let settings = read_settings(&paths).unwrap();

        assert_eq!(settings.backend, BackupBackend::LocalCopy);
        assert_eq!(settings.restic_path, env_restic);
        assert_eq!(settings.restic_password, "from-env");
        remove_docvault_env();
    }

    fn set_env(key: &str, value: impl AsRef<Path>) {
        unsafe {
            env::set_var(key, value.as_ref());
        }
    }

    fn remove_env(key: &str) {
        unsafe {
            env::remove_var(key);
        }
    }

    fn remove_docvault_env() {
        remove_env("DOCVAULT_ROOT_DIR");
        remove_env("DOCVAULT_DATA_DIR");
        remove_env("DOCVAULT_DB_PATH");
        remove_env("DOCVAULT_BACKUP_BACKEND");
        remove_env("DOCVAULT_RESTIC_PATH");
        remove_env("DOCVAULT_RESTIC_PASSWORD");
    }

    fn absolute(path: impl Into<PathBuf>) -> PathBuf {
        absolute_path(path.into())
    }

    fn config_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }
}
