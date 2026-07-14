//! Developer/test reset helpers: wipe the desktop back to a known state for
//! manual testing and (future) end-to-end runs.
//!
//! Two Tauri commands:
//! - [`reset_vault`]: switch to an isolated test vault and empty it -> the
//!   "fresh install" state (no documents, no tags, no tracked sources).
//! - [`seed_demo_docs`]: reset, then synchronously import three sample Office
//!   docs (one per type) and write tags + source-file baselines, so the
//!   tag/filter/modification-tracking flows are all exercised.
//!
//! Both target an isolated `docvault-test-vault` under the app config dir, so a
//! reset never touches a vault the user connected manually. The heavy lifting
//! (purge / seed / slice write) is in pure helpers so it is unit-testable without
//! an `AppHandle`; the commands are thin wrappers that resolve paths via the app.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use docvault_core::DocVault;
use docvault_storage::DocumentRef;
use docvault_types::{CommitMetadata, Document};
use tauri::{AppHandle, Manager, State};

use crate::dto::{ConnectError, DesktopStateSlice, TrackedFile};
use crate::local_state::{canonical_key, load_file_at, probe_at, save_file_at, state_path};
use crate::prefs;
use crate::state::{self, AppState};

/// Files above this are not sha256-hashed when baselining, matching the
/// frontend's `MODIFICATION_HASH_THRESHOLD_BYTES`.
const HASH_THRESHOLD_BYTES: u64 = 50 * 1024 * 1024;

/// The three sample docs imported by [`seed_demo_docs`]: (filename, doc name).
const SEED_ENTRIES: &[(&str, &str)] = &[
    ("report_v1.docx", "Report"),
    ("slides_v1.pptx", "Slides"),
    ("table_v1.xlsx", "Table"),
];

// --- pure helpers (no AppHandle; unit-testable) ---

/// Where the sample Office docs live. `CARGO_MANIFEST_DIR` is the `src-tauri`
/// dir at compile time, so the repo-root `example_docs` is one level up. Only
/// meaningful in dev (or a build run on the compile host); commands check
/// presence and return a clear error when the dir is absent.
fn example_docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("example_docs")
}

/// Delete every entry under `root` (config/data/repo/db/...), keeping the root
/// directory itself. Creates the directory when missing. The caller drops the
/// open vault before calling so the DB file is not locked on Windows.
fn purge_vault_root(root: &Path) -> Result<(), String> {
    if root.exists() {
        for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
            } else {
                fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
        }
    } else {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Synchronously commit the three sample docs as new documents and return each
/// with the source path it was imported from. Uses the vault directly (not the
/// job runner) so seeding is atomic and the caller gets the new document ids.
fn seed_three_docs(
    vault: &DocVault,
    example_docs: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<(Document, PathBuf)>, String> {
    let mut out = Vec::with_capacity(SEED_ENTRIES.len());
    for (file, name) in SEED_ENTRIES {
        let path = example_docs.join(file);
        if !path.exists() {
            return Err(format!("example doc not found: {}", path.display()));
        }
        let metadata = CommitMetadata {
            author: Some("seed".to_owned()),
            note: None,
        };
        let (document, _version) = vault
            .commit_document(&path, DocumentRef::NewName((*name).to_owned()), metadata, cancel)
            .map_err(|e| e.to_string())?;
        out.push((document, path));
    }
    Ok(out)
}

/// Write the seeded slice (tags + tracked source baselines) for `root_key` into
/// the desktop-state file, replacing any prior slice for that root. Tags are
/// assigned by document name; each tracked entry captures a fresh probe so the
/// status reads "unchanged" until the source file is edited.
fn write_seed_slice(
    state_file: &Path,
    root_key: &str,
    docs: &[(Document, PathBuf)],
) -> Result<(), String> {
    let mut file = load_file_at(state_file)?;
    let mut tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (doc, _path) in docs {
        let doc_tags = match doc.name.as_str() {
            "Report" => vec!["draft".to_owned(), "important".to_owned()],
            "Slides" => vec!["draft".to_owned()],
            "Table" => vec!["important".to_owned()],
            _ => Vec::new(),
        };
        if !doc_tags.is_empty() {
            tags.insert(doc.id.as_str().to_owned(), doc_tags);
        }
    }
    let tracked = docs
        .iter()
        .map(|(doc, path)| {
            let probe = probe_at(path, HASH_THRESHOLD_BYTES);
            TrackedFile {
                document_id: doc.id.as_str().to_owned(),
                path: path.display().to_string(),
                size: probe.size,
                mtime_ms: probe.mtime_ms,
                sha256: probe.sha256,
            }
        })
        .collect();
    file.vaults
        .insert(root_key.to_owned(), DesktopStateSlice { tags, tracked });
    save_file_at(state_file, &file)
}

/// Remove the slice for `root_key` from the desktop-state file (used on reset so
/// the test vault starts with no tags/tracked entries). A missing file is a no-op.
fn clear_slice_at(state_file: &Path, root_key: &str) -> Result<(), String> {
    let mut file = load_file_at(state_file)?;
    if file.vaults.remove(root_key).is_some() {
        save_file_at(state_file, &file)?;
    }
    Ok(())
}

fn connect_err_to_string(e: ConnectError) -> String {
    match e {
        ConnectError::Other(msg) => msg,
        other => format!("{other:?}"),
    }
}

// --- AppHandle-bound wrappers + commands ---

fn app_config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

/// Core reset: drop the open vault (releases the DB file on Windows), purge the
/// test vault root, initialize a fresh local-copy vault there, persist it as the
/// active root, and clear its desktop-state slice. Shared by both commands.
fn reset_to_test_vault(app: &AppHandle, state: &AppState, root: &Path) -> Result<(), String> {
    // Drop the open vault before purging so its sqlite file is not locked.
    *state::lock_vault(&state.vault) = None;
    purge_vault_root(root)?;
    state::connect_vault_core(
        state,
        &root.display().to_string(),
        "local-copy",
        None,
    )
    .map_err(connect_err_to_string)?;
    prefs::save_root(app, root).map_err(|e| e.to_string())?;
    if let Some(state_file) = state_path(app) {
        clear_slice_at(&state_file, &canonical_key(root))?;
    }
    Ok(())
}

/// Reset the desktop to a fresh-install state: an empty isolated test vault with
/// no documents, tags, or tracked sources. Never touches a manually-connected
/// vault. The frontend reloads documents/config/state after this resolves.
#[tauri::command(rename_all = "snake_case")]
pub fn reset_vault(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let root = app_config_dir(&app)?.join("docvault-test-vault");
    reset_to_test_vault(&app, state.inner(), &root)
}

/// Reset, then import the three sample docs and write tags + source baselines so
/// the tag/filter/modification-tracking flows are all exercised. Synchronous
/// (does not go through the job runner) so the result is immediately observable.
#[tauri::command(rename_all = "snake_case")]
pub fn seed_demo_docs(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let root = app_config_dir(&app)?.join("docvault-test-vault");
    reset_to_test_vault(&app, state.inner(), &root)?;

    let example_docs = example_docs_dir();
    if !example_docs.is_dir() {
        return Err(format!(
            "example docs not found at {} (seed requires the repo's example_docs)",
            example_docs.display()
        ));
    }
    let cancel = AtomicBool::new(false);
    let docs = {
        let vault = state::lock_vault(&state.vault);
        let vault = vault
            .as_ref()
            .ok_or("vault not initialized after reset")?;
        seed_three_docs(vault, &example_docs, &cancel)?
    };
    if let Some(state_file) = state_path(&app) {
        write_seed_slice(&state_file, &canonical_key(&root), &docs)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_state::DesktopStateFile;
    use docvault_storage::{VaultPaths, VaultStorage};
    use docvault_types::DocumentId;

    /// `example_docs_dir()` if it actually exists on this machine, else `None`
    /// (tests that need real OOXML files skip themselves when absent).
    fn example_docs_available() -> Option<PathBuf> {
        let dir = example_docs_dir();
        if dir.is_dir() {
            Some(dir)
        } else {
            None
        }
    }

    #[test]
    fn purge_removes_all_entries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("config.toml"), "x").unwrap();
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data").join("f"), "y").unwrap();

        purge_vault_root(&root).unwrap();

        assert!(root.exists(), "root dir itself is preserved");
        assert!(
            fs::read_dir(&root).unwrap().next().is_none(),
            "all entries removed"
        );
    }

    #[test]
    fn purge_creates_root_when_missing() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("never-existed");

        purge_vault_root(&root).unwrap();

        assert!(root.exists());
    }

    #[test]
    fn seed_three_docs_imports_one_per_type() {
        let Some(example_docs) = example_docs_available() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let paths = VaultPaths::from_root(&root);
        let storage = VaultStorage::init(paths).unwrap();
        let vault = DocVault::new(storage);
        let cancel = AtomicBool::new(false);

        let docs = seed_three_docs(&vault, &example_docs, &cancel).unwrap();

        assert_eq!(docs.len(), 3);
        let names: Vec<String> = docs.iter().map(|(d, _)| d.name.clone()).collect();
        assert!(names.contains(&"Report".to_owned()));
        assert!(names.contains(&"Slides".to_owned()));
        assert!(names.contains(&"Table".to_owned()));
        assert_eq!(vault.list_documents().unwrap().len(), 3);
    }

    #[test]
    fn write_seed_slice_records_tags_and_tracked() {
        let temp = tempfile::tempdir().unwrap();
        let state_file = temp.path().join("desktop-state.json");
        let src = temp.path().join("report_v1.docx");
        fs::write(&src, b"fake-content").unwrap();
        let docs = vec![(
            Document {
                id: DocumentId::new("doc1"),
                name: "Report".to_owned(),
                current_version_id: None,
                created_at: 0,
            },
            src.clone(),
        )];

        write_seed_slice(&state_file, "/test-root", &docs).unwrap();

        let file = load_file_at(&state_file).unwrap();
        let slice = file.vaults.get("/test-root").expect("slice written");
        assert_eq!(
            slice.tags.get("doc1").unwrap(),
            &["draft".to_owned(), "important".to_owned()]
        );
        assert_eq!(slice.tracked.len(), 1);
        assert_eq!(slice.tracked[0].document_id, "doc1");
        assert_eq!(slice.tracked[0].path, src.display().to_string());
        assert!(slice.tracked[0].size > 0, "baseline size probed");
    }

    #[test]
    fn clear_slice_at_removes_only_the_target_root() {
        let temp = tempfile::tempdir().unwrap();
        let state_file = temp.path().join("desktop-state.json");
        // Pre-populate two roots.
        let mut file = DesktopStateFile::default();
        file.vaults.insert(
            "/a".to_owned(),
            DesktopStateSlice {
                tags: BTreeMap::new(),
                tracked: Vec::new(),
            },
        );
        file.vaults.insert(
            "/b".to_owned(),
            DesktopStateSlice {
                tags: BTreeMap::new(),
                tracked: Vec::new(),
            },
        );
        save_file_at(&state_file, &file).unwrap();

        clear_slice_at(&state_file, "/a").unwrap();

        let reloaded = load_file_at(&state_file).unwrap();
        assert!(!reloaded.vaults.contains_key("/a"));
        assert!(reloaded.vaults.contains_key("/b"));
    }

    /// The reset core (drop -> purge -> reconnect) yields an empty vault even
    /// when the previous vault held documents. Requires example_docs to seed the
    /// "before" state; skips when absent.
    #[test]
    fn reset_clears_existing_docs_via_purge_and_reconnect() {
        let Some(example_docs) = example_docs_available() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let state = AppState::new();

        // Initialize and seed three docs.
        state::connect_vault_core(&state, &root.display().to_string(), "local-copy", None).unwrap();
        {
            let vault = state::lock_vault(&state.vault);
            let vault = vault.as_ref().unwrap();
            let cancel = AtomicBool::new(false);
            seed_three_docs(vault, &example_docs, &cancel).unwrap();
            assert_eq!(vault.list_documents().unwrap().len(), 3);
        }

        // Reset core: drop the open vault, purge, reconnect.
        *state::lock_vault(&state.vault) = None;
        purge_vault_root(&root).unwrap();
        state::connect_vault_core(&state, &root.display().to_string(), "local-copy", None).unwrap();

        let vault = state::lock_vault(&state.vault);
        let vault = vault.as_ref().unwrap();
        assert_eq!(vault.list_documents().unwrap().len(), 0);
        assert!(root.join("config.toml").exists(), "fresh config written");
    }
}
