use serde::Serialize;

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
