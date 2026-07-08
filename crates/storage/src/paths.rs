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
