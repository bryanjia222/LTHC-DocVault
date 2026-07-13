use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use docvault_types::{Document, Version};

/// A document joined with its versions. Returned to the UI so the document list
/// can render the nested version tree without a round-trip per row. Carries raw
/// `docvault_types`; all formatting (bytes, dates, status) happens client-side.
#[derive(Debug, Serialize)]
pub struct DocumentWithVersions {
    pub document: Document,
    pub versions: Vec<Version>,
}

#[derive(Debug, Serialize)]
pub struct ConfigDto {
    pub backend: String,
    pub data_dir: String,
    pub repo_dir: String,
    pub db_path: String,
    pub restic_path: String,
    pub log_level: String,
    pub log_file: String,
    pub restic_version: String,
}

#[derive(Debug, Serialize)]
pub struct VaultStatusDto {
    pub initialized: bool,
    pub root_dir: String,
    /// Error from the last failed attempt to open an already-initialized vault.
    /// Absent when the vault is open or no open has been attempted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_error: Option<String>,
}

/// Result of a `connect_vault` call. `mode` is `"initialized"` (a new vault was
/// created in an empty directory) or `"opened"` (an existing recognized vault
/// was attached). `backend` is the effective backend of the now-active vault.
#[derive(Debug, Serialize)]
pub struct ConnectOutcome {
    pub mode: String,
    pub backend: String,
    pub root_dir: String,
}

/// Structured error for `connect_vault`, serialized to the UI so it can map
/// each case to a localized message.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum ConnectError {
    /// A job is still running; switching would yank the vault out from under it.
    JobsRunning,
    /// The chosen directory is non-empty but is not a recognizable DocVault vault.
    Unrecognized,
    /// The restic backend was selected without supplying a password.
    ResticPasswordRequired,
    /// Any other failure (init/open IO error, invalid backend, ...). Carries the
    /// backend's verbatim message.
    Other(String),
}

/// Desktop-local annotations for one vault, stored in `desktop-state.json`
/// (separate from any vault's own `config.toml`/DB). The DocVault backend never
/// persists local file paths or tags, so these live entirely on the desktop side
/// and are scoped by vault root - switching vaults swaps the slice.
///
/// `tags` maps document id -> tag list. `tracked` holds the source-file baseline
/// captured at import time, used by the modification tracker.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesktopStateSlice {
    #[serde(default)]
    pub tags: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub tracked: Vec<TrackedFile>,
}

/// A tracked source file: the path the user last committed for a document, plus
/// the size/mtime/sha256 snapshot captured right after that commit. The tracker
/// compares a fresh probe against this baseline to detect external edits.
/// `sha256` is omitted for files above the hash threshold (too large to hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedFile {
    pub document_id: String,
    pub path: String,
    pub size: u64,
    pub mtime_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sha256: Option<String>,
}

/// Fast stat result for a single path (no content hashing). `exists` is false
/// for any path that cannot be stat'd (missing or inaccessible).
#[derive(Debug, Serialize)]
pub struct FileStat {
    pub path: String,
    pub exists: bool,
    pub size: u64,
    pub mtime_ms: u64,
}

/// Full probe of a single path: stat plus a sha256 digest, computed only when
/// the file exists and its size is within `max_bytes` (so large files are not
/// hashed on every poll). `sha256` is `None` otherwise.
#[derive(Debug, Serialize)]
pub struct FileProbe {
    pub exists: bool,
    pub size: u64,
    pub mtime_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}
