//! Desktop-local state (tags + tracked source files), persisted in
//! `desktop-state.json` in the Tauri app config dir - the same dir as
//! `desktop-prefs.json`. The DocVault backend never stores local file paths or
//! tags, so this file is the single home for desktop-only annotations.
//!
//! State is scoped by vault root: each vault root maps to its own
//! [`DesktopStateSlice`] (tags + tracked files), so switching vaults swaps the
//! active slice and two vaults never share tags. The root key is the
//! canonicalized vault root path (falls back to the raw display string when
//! canonicalization fails), so connecting the same vault via different path
//! spellings still resolves to one slice.
//!
//! File-stat / sha256 probing is split into pure helpers ([`stat_at`],
//! [`probe_at`]) so the two-tier modification detection (fast stat, full hash
//! only when stat changed and the file is small) is unit-testable without a
//! running app.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tracing::warn;

use crate::dto::{
    DesktopStateSlice, FileProbe, FileStat, ProjectDef, SortPref, TrackedFile, TrashedVersion,
};
use crate::state::{self, AppState};

/// On-disk shape: a versioned map of vault root -> slice. `version` is reserved
/// for future migration; today it is always written as 1.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct DesktopStateFile {
    #[serde(default)]
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) vaults: BTreeMap<String, DesktopStateSlice>,
}

// --- pure helpers (no AppHandle; unit-testable) ---

/// Read & deserialize the state file. A missing file is not an error - it yields
/// an empty (default) state, so first run is transparent.
pub(crate) fn load_file_at(path: &Path) -> Result<DesktopStateFile, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(crate::logging::log_error),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DesktopStateFile::default()),
        Err(e) => Err(crate::logging::log_error(e)),
    }
}

/// Serialize & write the state file, creating the parent dir if needed.
pub(crate) fn save_file_at(path: &Path, file: &DesktopStateFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(crate::logging::log_error)?;
    }
    let text = serde_json::to_string_pretty(file).map_err(crate::logging::log_error)?;
    fs::write(path, text).map_err(crate::logging::log_error)
}

/// The slice for `root` in `file`, or an empty default when absent. Pure: takes
/// the root key already canonicalized (or any stable string).
fn slice_for_root(file: &DesktopStateFile, root: &str) -> DesktopStateSlice {
    file.vaults.get(root).cloned().unwrap_or_default()
}

/// Canonicalize a vault root path into a stable map key. Falls back to the raw
/// display string when canonicalization fails (e.g. the path no longer exists),
/// so a transiently-unavailable vault still resolves to its stored slice.
pub(crate) fn canonical_key(path: &Path) -> String {
    fs::canonicalize(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

/// Stat a single path. Any metadata error (missing or inaccessible) is reported
/// as `exists: false` so the tracker surfaces "源文件缺失" rather than crashing
/// a batch poll.
fn stat_at(path: &Path) -> FileStat {
    match fs::metadata(path) {
        Ok(meta) => FileStat {
            path: path.display().to_string(),
            exists: true,
            size: meta.len(),
            mtime_ms: mtime_ms(&meta),
        },
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    path = %path.display(),
                    %error,
                    "tracked file stat failed; reporting as missing"
                );
            }
            FileStat {
                path: path.display().to_string(),
                exists: false,
                size: 0,
                mtime_ms: 0,
            }
        }
    }
}

/// Stat + (conditionally) hash a single path. `sha256` is computed only when the
/// file exists and its size is within `max_bytes`, so large files are never
/// hashed. A read failure on an existing small file yields `sha256: None` (the
/// tracker then treats a stat change as "modified" rather than crashing).
pub(crate) fn probe_at(path: &Path, max_bytes: u64) -> FileProbe {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    path = %path.display(),
                    %error,
                    "tracked file probe failed; reporting as missing"
                );
            }
            return FileProbe {
                exists: false,
                size: 0,
                mtime_ms: 0,
                sha256: None,
            };
        }
    };
    let size = meta.len();
    let mtime_ms = mtime_ms(&meta);
    let sha256 = if size <= max_bytes {
        compute_sha256(path)
    } else {
        None
    };
    FileProbe {
        exists: true,
        size,
        mtime_ms,
        sha256,
    }
}

/// File mtime as milliseconds since the Unix epoch (0 when unavailable).
fn mtime_ms(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Hex sha256 of a file's contents, streamed through a 64 KB buffer. `None` on
/// any read error.
fn compute_sha256(path: &Path) -> Option<String> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            warn!(path = %path.display(), %error, "failed to open tracked file for hashing");
            return None;
        }
    };
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = match file.read(&mut buf) {
            Ok(n) => n,
            Err(error) => {
                warn!(path = %path.display(), %error, "failed to read tracked file for hashing");
                return None;
            }
        };
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

// --- AppHandle-bound wrappers + commands ---

pub(crate) fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("desktop-state.json"))
}

/// The canonicalized root of the currently-open vault, or `None` when no vault
/// is open (onboarding). The desktop-state file is scoped by this key; the
/// preview cache reuses it to scope on-disk previews per vault.
pub(crate) fn current_vault_root(state: &State<AppState>) -> Option<String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref()?;
    Some(canonical_key(&vault.paths().root_dir))
}

/// Read the whole state file from disk.
fn load_file(app: &AppHandle) -> Result<DesktopStateFile, String> {
    let path = state_path(app)
        .ok_or_else(|| crate::logging::log_warn("app config directory unavailable"))?;
    load_file_at(&path)
}

/// Write the whole state file to disk.
fn save_file(app: &AppHandle, file: &DesktopStateFile) -> Result<(), String> {
    let path = state_path(app)
        .ok_or_else(|| crate::logging::log_warn("app config directory unavailable"))?;
    save_file_at(&path, file)
}

/// Return the current vault's desktop-local slice (tags + tracked files). An
/// empty slice is returned when no vault is open, so onboarding renders cleanly.
#[tauri::command(rename_all = "snake_case")]
pub fn get_desktop_state(
    app: AppHandle,
    state: State<AppState>,
) -> Result<DesktopStateSlice, String> {
    let file = load_file(&app)?;
    let root = current_vault_root(&state);
    Ok(slice_for_root(&file, root.as_deref().unwrap_or("")))
}

/// Replace the current vault's slice (tags, tracked, projects, assignments,
/// sort_prefs, trashed, trashed_versions) and persist. Refuses when no vault is
/// open, since there is no root to key the slice by.
#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)] // each arg maps 1:1 to the JS payload
pub fn set_desktop_state(
    app: AppHandle,
    state: State<AppState>,
    tags: BTreeMap<String, Vec<String>>,
    tracked: Vec<TrackedFile>,
    projects: Vec<ProjectDef>,
    assignments: BTreeMap<String, String>,
    sort_prefs: BTreeMap<String, SortPref>,
    trashed: Vec<String>,
    trashed_versions: Vec<TrashedVersion>,
) -> Result<(), String> {
    let root = current_vault_root(&state)
        .ok_or_else(|| crate::logging::log_warn("vault not initialized"))?;
    let mut file = load_file(&app)?;
    file.vaults.insert(
        root,
        DesktopStateSlice {
            tags,
            tracked,
            projects,
            assignments,
            sort_prefs,
            trashed,
            trashed_versions,
        },
    );
    save_file(&app, &file)
}

/// Fast batch stat (size + mtime only, no hashing) for the two-tier tracker's
/// polling pass. Each path resolves independently; a missing/inaccessible path
/// returns `exists: false` rather than failing the whole batch.
#[tauri::command(rename_all = "snake_case")]
pub fn stat_files(paths: Vec<String>) -> Result<Vec<FileStat>, String> {
    Ok(paths.into_iter().map(|p| stat_at(Path::new(&p))).collect())
}

/// Full probe (stat + sha256 when the file is within `max_bytes`) for a single
/// path. Used for the import-time baseline and the full-detection pass.
#[tauri::command(rename_all = "snake_case")]
pub fn probe_file(path: String, max_bytes: u64) -> Result<FileProbe, String> {
    Ok(probe_at(Path::new(&path), max_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A missing state file yields an empty default - first run is transparent.
    #[test]
    fn load_missing_file_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("desktop-state.json");
        let file = load_file_at(&path).unwrap();
        assert!(file.vaults.is_empty());
    }

    /// Writing then reading round-trips tags, tracked files, projects and
    /// assignments verbatim, and omits `sha256` when it is `None`.
    #[test]
    fn save_then_load_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested/desktop-state.json");
        let mut file = DesktopStateFile {
            version: 1,
            ..Default::default()
        };
        file.vaults.insert(
            "/vault".to_owned(),
            DesktopStateSlice {
                tags: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        "docA".to_owned(),
                        vec!["legal".to_owned(), "draft".to_owned()],
                    );
                    m
                },
                tracked: vec![TrackedFile {
                    document_id: "docA".to_owned(),
                    path: "/tmp/a.docx".to_owned(),
                    size: 10,
                    mtime_ms: 5,
                    sha256: None,
                }],
                projects: vec![ProjectDef {
                    id: "proj1".to_owned(),
                    name: "诉讼案".to_owned(),
                    parent_id: None,
                }],
                assignments: {
                    let mut m = BTreeMap::new();
                    m.insert("docA".to_owned(), "proj1".to_owned());
                    m
                },
                sort_prefs: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        "proj1".to_owned(),
                        SortPref {
                            key: "name".to_owned(),
                            direction: "asc".to_owned(),
                        },
                    );
                    m
                },
                trashed: vec!["docA".to_owned()],
                trashed_versions: Vec::new(),
            },
        );
        save_file_at(&path, &file).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("sha256"),
            "None sha256 should be omitted, got: {text}"
        );

        let reloaded = load_file_at(&path).unwrap();
        let slice = slice_for_root(&reloaded, "/vault");
        assert_eq!(slice.tags.get("docA").unwrap(), &["legal", "draft"]);
        assert_eq!(slice.tracked.len(), 1);
        assert_eq!(slice.tracked[0].path, "/tmp/a.docx");
        assert_eq!(slice.tracked[0].sha256, None);
        assert_eq!(slice.projects.len(), 1);
        assert_eq!(slice.projects[0].id, "proj1");
        assert_eq!(slice.projects[0].name, "诉讼案");
        assert_eq!(slice.assignments.get("docA").unwrap(), &"proj1".to_owned());
        let sort = slice.sort_prefs.get("proj1").unwrap();
        assert_eq!(sort.key, "name");
        assert_eq!(sort.direction, "asc");
        assert_eq!(slice.trashed, vec!["docA".to_owned()]);
    }

    /// Two vault roots keep independent slices - switching vaults never leaks
    /// tags, tracked files, projects or assignments from one into the other.
    #[test]
    fn slices_are_isolated_per_root() {
        let mut file = DesktopStateFile::default();
        file.vaults.insert(
            "/a".to_owned(),
            DesktopStateSlice {
                tags: {
                    let mut m = BTreeMap::new();
                    m.insert("docA".to_owned(), vec!["t1".to_owned()]);
                    m
                },
                tracked: Vec::new(),
                projects: vec![ProjectDef {
                    id: "pa".to_owned(),
                    name: "项目甲".to_owned(),
                    parent_id: None,
                }],
                assignments: {
                    let mut m = BTreeMap::new();
                    m.insert("docA".to_owned(), "pa".to_owned());
                    m
                },
                sort_prefs: BTreeMap::new(),
                trashed: Vec::new(),
                trashed_versions: Vec::new(),
            },
        );
        file.vaults.insert(
            "/b".to_owned(),
            DesktopStateSlice {
                tags: BTreeMap::new(),
                tracked: vec![TrackedFile {
                    document_id: "docB".to_owned(),
                    path: "/b.docx".to_owned(),
                    size: 1,
                    mtime_ms: 1,
                    sha256: None,
                }],
                projects: Vec::new(),
                assignments: BTreeMap::new(),
                sort_prefs: BTreeMap::new(),
                trashed: Vec::new(),
                trashed_versions: Vec::new(),
            },
        );

        let a = slice_for_root(&file, "/a");
        let b = slice_for_root(&file, "/b");
        let none = slice_for_root(&file, "/c");
        assert_eq!(a.tags.get("docA").unwrap(), &["t1"]);
        assert!(a.tracked.is_empty());
        assert_eq!(a.projects.len(), 1);
        assert_eq!(a.projects[0].id, "pa");
        assert_eq!(a.assignments.get("docA").unwrap(), &"pa".to_owned());
        assert!(b.tags.is_empty());
        assert_eq!(b.tracked.len(), 1);
        assert!(b.projects.is_empty());
        assert!(b.assignments.is_empty());
        assert!(
            none.tags.is_empty()
                && none.tracked.is_empty()
                && none.projects.is_empty()
                && none.assignments.is_empty()
                && none.sort_prefs.is_empty()
                && none.trashed.is_empty()
        );
    }

    /// `assignments` is now `docId -> single projectId`. Legacy state files may
    /// carry a multi-membership `Vec<String>` (or an even older single `String`);
    /// the tolerant deserializer collapses a `Vec` to its first element and passes
    /// a `String` through, so old files load transparently.
    #[test]
    fn assignments_coerce_legacy_shapes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("desktop-state.json");
        // docA uses a legacy single-String; docB a legacy multi-membership Vec -
        // both must load as the single (first) project id.
        let json = r#"{"version":1,"vaults":{"/v":{"tags":{},"tracked":[],"projects":[{"id":"p1","name":"P"}],"assignments":{"docA":"p1","docB":["p1","p2"]},"sort_prefs":{}}}}"#;
        fs::write(&path, json).unwrap();

        let file = load_file_at(&path).unwrap();
        let slice = slice_for_root(&file, "/v");
        assert_eq!(slice.assignments.get("docA").unwrap(), &"p1".to_owned());
        // Legacy Vec collapses to its first element ("p1"); the extra "p2" is dropped.
        assert_eq!(slice.assignments.get("docB").unwrap(), &"p1".to_owned());
    }

    /// `stat_at` reports exists/size/mtime for a real file and `exists: false`
    /// for a missing one.
    #[test]
    fn stat_at_reports_existing_and_missing() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("x.docx");
        fs::write(&file, b"hello").unwrap();

        let stat = stat_at(&file);
        assert!(stat.exists);
        assert_eq!(stat.size, 5);
        assert!(stat.mtime_ms > 0);

        let missing = stat_at(&temp.path().join("nope.docx"));
        assert!(!missing.exists);
        assert_eq!(missing.size, 0);
    }

    /// `probe_at` hashes a small file but skips hashing when the file is larger
    /// than `max_bytes`; a missing file reports `exists: false` with no digest.
    #[test]
    fn probe_at_hashes_small_skips_large() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("x.docx");
        fs::write(&file, b"hello").unwrap();

        let small = probe_at(&file, 1024);
        assert!(small.exists);
        assert_eq!(small.size, 5);
        let sha = small.sha256.expect("small file should be hashed");
        assert_eq!(sha.len(), 64, "sha256 hex is 64 chars");

        // Same content -> same digest.
        let again = probe_at(&file, 1024);
        assert_eq!(again.sha256.as_deref(), Some(sha.as_str()));

        // max_bytes below the file size -> no hashing.
        let large = probe_at(&file, 1);
        assert!(large.exists);
        assert_eq!(large.sha256, None);

        let missing = probe_at(&temp.path().join("nope.docx"), 1024);
        assert!(!missing.exists);
        assert_eq!(missing.sha256, None);
    }

    /// Different contents produce different digests.
    #[test]
    fn sha256_distinguishes_contents() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a.docx");
        let b = temp.path().join("b.docx");
        fs::write(&a, b"version one").unwrap();
        fs::write(&b, b"version two").unwrap();
        assert_ne!(probe_at(&a, 1024).sha256, probe_at(&b, 1024).sha256);
    }
}
