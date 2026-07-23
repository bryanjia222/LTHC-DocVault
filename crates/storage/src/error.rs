use std::{io, path::PathBuf};

use docvault_ooxml::OoxmlError;
use docvault_types::Document;
use thiserror::Error;

use crate::format_document_matches;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("OOXML error: {0}")]
    Ooxml(#[from] OoxmlError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML decode error: {0}")]
    TomlDecode(#[from] toml::de::Error),
    #[error("TOML encode error: {0}")]
    TomlEncode(#[from] toml::ser::Error),
    #[error(
        "document not found: {0}\nRun `docvault list` to see documents, or use `docvault commit <path> --name {0}` to create it."
    )]
    DocumentNotFound(String),
    #[error(
        "document id not found: {0}\nRun `docvault list --format table` and retry with a longer `--id <id-prefix>`."
    )]
    DocumentIdNotFound(String),
    #[error(
        "document name is ambiguous: {name}\n{}",
        format_document_matches(matches)
    )]
    AmbiguousDocumentName {
        name: String,
        matches: Vec<Document>,
    },
    #[error(
        "document id prefix is ambiguous: {prefix}\n{}",
        format_document_matches(matches)
    )]
    AmbiguousDocumentIdPrefix {
        prefix: String,
        matches: Vec<Document>,
    },
    #[error("document reference mismatch: requested name {requested_name}, matched {} ({})", matched.name, matched.id.as_str())]
    DocumentReferenceMismatch {
        requested_name: String,
        matched: Box<Document>,
    },
    #[error(
        "version {version} not found for document {document_name}\nRun `docvault versions {document_name}` to see available versions. Use `latest` for the highest version number or `current` for the current pointer."
    )]
    VersionNotFound {
        document_name: String,
        version: String,
    },
    /// The caller asked to delete a document's current (checked-out) version,
    /// which would leave the document's `current_version_id` pointer dangling.
    /// The UI blocks this too; this is the backend's defensive guard.
    #[error(
        "cannot delete the current version {version_id} of document {document_name}; switch to another version before deleting it"
    )]
    CannotDeleteCurrentVersion {
        document_name: String,
        version_id: String,
    },
    #[error("invalid file name: {}", .0.display())]
    InvalidFileName(PathBuf),
    #[error("invalid backup backend: {0}")]
    InvalidBackend(String),
    /// The restic backend was selected but no password was supplied. Restic
    /// requires a repository password; `write_initial_config` rejects an empty
    /// one so the failure surfaces at config time rather than as a cryptic
    /// restic error later.
    #[error("restic backend requires a non-empty restic_password")]
    ResticPasswordRequired,
    /// A `pending` version row exists but its durable intake copy is gone, so
    /// the archive cannot be (re)completed. This violates the WAL invariant
    /// (intake fsynced before the DB row) and should be unreachable unless the
    /// intake dir was deleted out-of-band. Recovery leaves the row pending and
    /// surfaces this so the user knows a version is stranded.
    #[error(
        "intake copy missing for pending version {version_id} of document {document_id}; the archive cannot be completed"
    )]
    IntakeMissing {
        document_id: String,
        version_id: String,
    },
    #[error(transparent)]
    Restic(#[from] ResticError),
}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        DatabaseError::from(value).into()
    }
}

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Error)]
pub enum ResticError {
    #[error("restic command failed ({command}): {stderr}")]
    Failed { command: String, stderr: String },
    #[error("restic backup did not return a snapshot id")]
    SnapshotMissing,
    #[error("restic command cancelled")]
    Cancelled,
    #[error("restic command timed out")]
    TimedOut,
}
