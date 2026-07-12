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
