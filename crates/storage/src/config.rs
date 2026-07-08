use std::{env, fs, path::PathBuf};

use docvault_types::VaultConfig;

use crate::{BackupBackend, StorageResult, VaultPaths};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResticConfig {
    pub repo_dir: PathBuf,
    pub restic_path: Option<PathBuf>,
}

impl ResticConfig {
    pub fn new(repo_dir: impl Into<PathBuf>) -> Self {
        Self {
            repo_dir: repo_dir.into(),
            restic_path: None,
        }
    }

    pub fn with_restic_path(mut self, restic_path: impl Into<PathBuf>) -> Self {
        self.restic_path = Some(restic_path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StorageSettings {
    pub(crate) backend: BackupBackend,
    pub(crate) restic_path: PathBuf,
    pub(crate) restic_password: String,
}

pub(crate) fn read_settings(paths: &VaultPaths) -> StorageResult<StorageSettings> {
    let config = read_config(paths)?;
    let backend = env::var("DOCVAULT_BACKUP_BACKEND")
        .ok()
        .unwrap_or_else(|| config.storage.backend.clone());
    let restic_path = env::var_os("DOCVAULT_RESTIC_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            config
                .storage
                .restic_path
                .clone()
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| bundled_or_system_restic(paths));
    let restic_password = env::var("DOCVAULT_RESTIC_PASSWORD")
        .ok()
        .unwrap_or_else(|| config.storage.restic_password.clone());

    Ok(StorageSettings {
        backend: BackupBackend::parse(&backend)?,
        restic_path,
        restic_password,
    })
}

fn read_config(paths: &VaultPaths) -> StorageResult<VaultConfig> {
    if paths.config_path.exists() {
        read_config_file(&paths.config_path)
    } else {
        Ok(VaultConfig::for_paths(
            paths.data_dir.clone(),
            paths.repo_dir.clone(),
            paths.db_path.clone(),
        ))
    }
}

pub(crate) fn read_config_file(path: &std::path::Path) -> StorageResult<VaultConfig> {
    let config = fs::read_to_string(path)?;
    Ok(toml::from_str(&config)?)
}

pub(crate) fn default_config(paths: &VaultPaths) -> StorageResult<String> {
    Ok(toml::to_string_pretty(&VaultConfig::for_paths(
        paths.data_dir.clone(),
        paths.repo_dir.clone(),
        paths.db_path.clone(),
    ))?)
}

fn bundled_or_system_restic(paths: &VaultPaths) -> PathBuf {
    let bundled = paths
        .root_dir
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("third_party")
        .join("restic")
        .join("0.19.1")
        .join(target_triple())
        .join(restic_binary_name());
    if bundled.exists() {
        bundled
    } else {
        PathBuf::from(restic_binary_name())
    }
}

fn target_triple() -> &'static str {
    if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

fn restic_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "restic.exe"
    } else {
        "restic"
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn stores_explicit_restic_path() {
        let config = ResticConfig::new(".docvault/repo").with_restic_path("tools/restic.exe");

        assert_eq!(config.restic_path, Some(PathBuf::from("tools/restic.exe")));
    }
}
