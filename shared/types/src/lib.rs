use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub current_version_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub id: String,
    pub document_id: DocumentId,
    pub number: i64,
    pub original_filename: String,
    pub archive_reference: String,
    pub backup_backend: String,
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub manifest: OoxmlManifest,
    pub parent_version_id: Option<String>,
    pub author: Option<String>,
    pub note: Option<String>,
    pub created_at: i64,
    /// Lifecycle of the version's archive: `"archived"` (the archive is
    /// complete and is the source of truth) or `"pending"` (the durable intake
    /// copy exists but the compressed archive is still being written by the
    /// async commit path). Old serialized versions predate this field and
    /// default to `"archived"`. The async commit path inserts a row as
    /// `"pending"` and flips it to `"archived"` once the archive job finishes.
    #[serde(default = "default_archive_status")]
    pub archive_status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OoxmlManifest {
    pub entries: Vec<OoxmlManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OoxmlManifestEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitMetadata {
    pub author: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultConfig {
    pub storage: StorageConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl VaultConfig {
    pub fn for_paths(data_dir: PathBuf, repo_dir: PathBuf, db_path: PathBuf) -> Self {
        Self {
            storage: StorageConfig {
                // local-copy is the safe default: it needs no external binary,
                // so a fresh `init` (no config yet) works everywhere. Restic is
                // opt-in via an explicit backend choice (CLI `--backend restic`
                // or the desktop connect dialog), which writes the chosen
                // backend into config.toml before the vault is opened.
                backend: "local-copy".to_owned(),
                data_dir: config_path(data_dir),
                repo_dir: config_path(repo_dir),
                restic_path: None,
                restic_password: "docvault-local-development-password".to_owned(),
            },
            database: DatabaseConfig {
                path: config_path(db_path),
            },
            logging: LoggingConfig {
                level: "info".to_owned(),
                file: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_storage_backend")]
    pub backend: String,
    pub data_dir: String,
    pub repo_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restic_path: Option<String>,
    #[serde(default = "default_restic_password")]
    pub restic_password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: None,
        }
    }
}

fn default_storage_backend() -> String {
    "local-copy".to_owned()
}

fn default_restic_password() -> String {
    "docvault-local-development-password".to_owned()
}

/// Serde default for [`Version::archive_status`]: a version deserialized from
/// an older payload (before the async commit path) is treated as already
/// archived, since every pre-async version was archived synchronously.
fn default_archive_status() -> String {
    "archived".to_owned()
}

fn default_log_level() -> String {
    "info".to_owned()
}

fn config_path(path: PathBuf) -> String {
    path.display().to_string().replace('\\', "/")
}
