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
    /// Suggested vault location for a new vault: a cross-platform `.DocVault`
    /// directory under the user's home. Pre-filled in the connect dialog so a
    /// first-run user can create a vault there with one click, or browse elsewhere.
    pub recommended_root: String,
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

/// Result of probing a directory before connecting. `status` is `"empty"` (a
/// new vault can be initialized here with a user-chosen backend), `"existing"`
/// (a recognized DocVault vault whose backend is already fixed by its
/// `config.toml`), or `"unrecognized"` (non-empty and not a vault). `backend`
/// carries the existing vault's backend string, present only when `status ==
/// "existing"`. The connect dialog uses this to lock the backend selector for an
/// existing vault rather than letting the user pick a backend that
/// `connect_vault_core` would silently ignore.
#[derive(Debug, Serialize)]
pub struct VaultProbe {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
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

/// A user-created project folder for grouping documents in the sidebar. The
/// DocVault backend has no folder concept, so projects are desktop-local
/// annotations (like tags): each vault root owns its own project list. `id` is a
/// client-generated stable identifier (UUID); `name` is the display label.
/// `parent_id` (None for a root project) supports nesting - a sub-project hangs
/// off its parent. Older state files that predate nesting omit `parent_id` and
/// deserialize as root projects (None), so no migration is needed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Desktop-local annotations for one vault, stored in `desktop-state.json`
/// (separate from any vault's own `config.toml`/DB). The DocVault backend never
/// persists local file paths or tags, so these live entirely on the desktop side
/// and are scoped by vault root - switching vaults swaps the slice.
///
/// `tags` maps document id -> tag list. `tracked` holds the source-file baseline
/// captured at import time, used by the modification tracker. `projects` is the
/// vault's project-folder list; `assignments` maps document id -> its single
/// project id (one-to-many: a document belongs to at most one project; an absent
/// key means unassigned). `sort_prefs` stores each project's persisted table sort
/// (keyed by project id, with `"__all__"` for the ungrouped "all documents" view).
///
/// `assignments` is deserialized tolerantly: a legacy multi-membership
/// `Vec<String>` value (pre-single-membership state files) is collapsed to its
/// first element, and a legacy single `String` value passes through, so old
/// desktop-state.json files load without a migration step.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesktopStateSlice {
    #[serde(default)]
    pub tags: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub tracked: Vec<TrackedFile>,
    #[serde(default)]
    pub projects: Vec<ProjectDef>,
    #[serde(default, deserialize_with = "deserialize_assignments")]
    pub assignments: BTreeMap<String, String>,
    #[serde(default)]
    pub sort_prefs: BTreeMap<String, SortPref>,
    /// Document ids soft-deleted to the recycle bin (desktop-local hide). The
    /// backend vault still holds these documents and all their history until the
    /// user permanently deletes them from the bin; this list only suppresses them
    /// from the document list. Restoring removes the id; permanent delete clears
    /// it (and unmanages the document in the backend).
    #[serde(default)]
    pub trashed: Vec<String>,
}

/// A persisted document-table sort for one project view. `key` is the column
/// ("name" / "owner" / "currentVersion" / "status" / "modification" / "updated");
/// `direction` is `"asc"` or `"desc"`. An empty `key` means "use the UI default".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SortPref {
    pub key: String,
    pub direction: String,
}

/// Deserialize `assignments` into `doc_id -> single project_id`, accepting the
/// legacy shapes older desktop-state.json files may carry: a multi-membership
/// `Vec<String>` is collapsed to its first element, and a single `String` passes
/// through. So files written before single-membership load transparently (the
/// dropped extra memberships are recoverable by re-assigning in the UI).
fn deserialize_assignments<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Many(Vec<String>),
        One(String),
    }

    let map: BTreeMap<String, StringOrVec> = BTreeMap::deserialize(deserializer)?;
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (doc_id, value) in map {
        let project_id = match value {
            // Legacy multi-membership: keep only the first project id.
            StringOrVec::Many(vec) => vec.into_iter().next(),
            StringOrVec::One(s) => Some(s),
        };
        if let Some(id) = project_id {
            out.insert(doc_id, id);
        }
    }
    Ok(out)
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
