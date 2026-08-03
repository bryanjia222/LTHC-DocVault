mod archive;
mod commit;
mod config;
mod delete;
mod error;
mod paths;
mod repository;
mod restic;
mod sqlite;
mod versions;

use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use docvault_types::Document;
use rusqlite::Connection;

pub(crate) use config::StorageSettings;
pub use config::{ResticConfig, StorageOverrides, write_initial_config};
pub use error::{DatabaseError, ResticError, StorageError, StorageResult};
pub use paths::VaultPaths;

/// A cancellation flag that is never set. Used for restic calls that run
/// outside a job (vault init/open, startup recovery), where there is no job to
/// cancel.
pub static NEVER_CANCELLED: AtomicBool = AtomicBool::new(false);

/// A version whose durable intake copy exists but whose compressed archive has
/// not been finalized yet (the async commit path is still running or was
/// interrupted by a crash). See [`Version::archive_status`].
pub const ARCHIVE_STATUS_PENDING: &str = "pending";

/// A version whose archive is complete and is the source of truth for
/// exports/restores. The default for every pre-async-commit version.
pub const ARCHIVE_STATUS_ARCHIVED: &str = "archived";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentRef {
    Name(String),
    NewName(String),
    IdPrefix(String),
    NameAndIdPrefix { name: String, id_prefix: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupBackend {
    LocalCopy,
    Restic,
}

impl BackupBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalCopy => "local-copy",
            Self::Restic => "restic",
        }
    }

    pub(crate) fn parse(value: &str) -> StorageResult<Self> {
        match value {
            "local-copy" | "copy" => Ok(Self::LocalCopy),
            "restic" => Ok(Self::Restic),
            other => Err(StorageError::InvalidBackend(other.to_owned())),
        }
    }
}

/// The storage facade: SQLite rows (`sqlite`), archive/restore byte work
/// (`archive`), restic process orchestration (`restic`), and the workflow
/// orchestration split by domain into `repository` (construction + accessors),
/// `commit`, `versions`, and `delete`.
pub struct VaultStorage {
    pub(crate) paths: VaultPaths,
    pub(crate) settings: StorageSettings,
    pub(crate) connection: Connection,
    /// Best-effort `restic version` captured once at init/open. Empty for the
    /// local-copy backend or when restic is unavailable; avoids re-spawning on
    /// every config read.
    pub(crate) restic_version: String,
}

/// Current time as a Unix timestamp (seconds). Shared by the commit workflows;
/// `0` when the system clock is before the epoch.
pub(crate) fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn format_document_matches(matches: &[Document]) -> String {
    let mut output = String::from("matches:\n");
    for document in matches {
        output.push_str(&format!("  {}  {}\n", document.id.as_str(), document.name));
    }
    output.push_str("use --id <document_id> or name@<id-prefix>");
    output
}

#[cfg(test)]
mod tests;
