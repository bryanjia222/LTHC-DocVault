//! Developer/test reset helpers: wipe the desktop back to a known state for
//! manual testing and (future) end-to-end runs.
//!
//! One Tauri command, [`reset_to_stage`], drives a three-stage slider in the
//! dev Settings card:
//! - `fresh`: drop + purge the test vault and clear the saved root pref so the
//!   app returns to onboarding (first step = create or select a repo + backend).
//! - `initial`: re-initialize an empty vault with the chosen backend.
//! - `seeded`: `initial`, then synchronously import three sample Office docs
//!   and write tags + source-file baselines.
//!
//! All stages target an isolated `docvault-test-vault` under the app config
//! dir, so a reset never touches a vault the user connected manually. The
//! heavy lifting (purge / seed / slice write) is in pure helpers so it is
//! unit-testable without an `AppHandle`; the command is a thin wrapper that
//! resolves paths via the app.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use docvault_core::DocVault;
use docvault_storage::DocumentRef;
use docvault_types::{CommitMetadata, Document};
use tauri::{AppHandle, Manager, State};

use crate::dto::{ConnectError, DesktopStateSlice};
use crate::library::ensure_library_copies_for;
use crate::local_state::{canonical_key, load_file_at, save_file_at, state_path};
use crate::prefs;
use crate::state::{self, AppState};

/// The three sample docs imported by the `seeded` reset stage: (filename, doc name).
const SEED_ENTRIES: &[(&str, &str)] = &[
    ("report_v1.docx", "Report"),
    ("slides_v1.pptx", "Slides"),
    ("table_v1.xlsx", "Table"),
];

/// Default restic password for the dev test vault when the dev slider omits one.
/// Restic requires a password, but the dev flow makes it optional; this matches
/// the `docvault_types::VaultConfig::for_paths` dev default so a no-password
/// reset is consistent with a no-config default vault. Production `connect_vault`
/// never uses this - it requires an explicit password.
const DEV_RESTIC_PASSWORD: &str = "docvault-local-development-password";

// --- pure helpers (no AppHandle; unit-testable) ---

/// Where the sample Office docs live. `CARGO_MANIFEST_DIR` is the `src-tauri`
/// dir at compile time, so the repo-root `example_docs` is three levels up
/// (`src-tauri` -> `apps/desktop` -> `apps` -> repo root). Only meaningful in
/// dev (or a build run on the compile host); commands check presence and return
/// a clear error when the dir is absent.
fn example_docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
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

/// Synchronously commit the three sample docs as new documents and return them.
/// Uses the vault directly (not the job runner) so seeding is atomic. The library
/// copies + tracked baselines are written separately by [`write_seed_slice`]
/// (which calls `ensure_library_copies_for`), so only the `Document` is needed
/// here - the example-doc source paths are no longer tracked.
fn seed_three_docs(
    vault: &DocVault,
    example_docs: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<Document>, String> {
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
        out.push(document);
    }
    Ok(out)
}

/// Write the seeded slice (tags + tracked library copies) for `root_key` into
/// the desktop-state file, replacing any prior slice for that root. Library
/// copies are materialized and tracked via [`ensure_library_copies_for`] (the
/// tracked path is the library copy, not the example-doc source). Tags are
/// assigned by document name.
fn write_seed_slice(
    vault: &DocVault,
    state_file: &Path,
    root_key: &str,
    docs: &[Document],
) -> Result<(), String> {
    let mut file = load_file_at(state_file)?;
    let mut slice = DesktopStateSlice::default();
    ensure_library_copies_for(vault, &mut slice)?;
    let mut tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for doc in docs {
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
    slice.tags = tags;
    file.vaults.insert(root_key.to_owned(), slice);
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

/// Core stage reset. Drops the open vault (releases the DB file on Windows),
/// purges the test vault root, then applies the selected `stage`:
/// - `fresh`: clear the saved root pref (no vault) so the UI returns to
///   onboarding; clear the desktop-state slice.
/// - `initial`: initialize an empty vault with `backend` (+ restic password),
///   persist it as the active root, clear the slice.
/// - `seeded`: `initial`, then synchronously import the sample docs and write
///   their tags + tracked baselines.
fn reset_to_stage_core(
    app: &AppHandle,
    state: &AppState,
    root: &Path,
    stage: &str,
    backend: &str,
    restic_password: Option<String>,
) -> Result<(), String> {
    // Drop the open vault before purging so its sqlite file is not locked.
    *state::lock_vault(&state.vault) = None;
    purge_vault_root(root)?;
    state::set_open_error(state, None);

    let state_file = state_path(app);
    match stage {
        "fresh" => {
            // No vault: clear the saved root pref so the next status read is
            // uninitialized and the app shows onboarding (repo + backend pick).
            prefs::clear_root(app).map_err(|e| e.to_string())?;
            if let Some(state_file) = state_file {
                clear_slice_at(&state_file, &canonical_key(root))?;
            }
            Ok(())
        }
        "initial" | "seeded" => {
            // For the dev test vault a restic password is optional: fall back to
            // the dev default when blank so a reset need not prompt for one.
            let effective_password = if backend == "restic" {
                Some(
                    restic_password
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .unwrap_or(DEV_RESTIC_PASSWORD)
                        .to_owned(),
                )
            } else {
                restic_password
            };
            state::connect_vault_core(
                state,
                &root.display().to_string(),
                backend,
                effective_password,
            )
            .map_err(connect_err_to_string)?;
            prefs::save_root(app, root).map_err(|e| e.to_string())?;
            if let Some(state_file) = state_file {
                clear_slice_at(&state_file, &canonical_key(root))?;
            }
            if stage == "seeded" {
                seed_sample_docs(state, app, root)?;
            }
            Ok(())
        }
        other => Err(format!("unknown reset stage: {other}")),
    }
}

/// Seed the three sample docs into the currently-open test vault and write
/// their tags + tracked library baselines. The vault must already be open.
fn seed_sample_docs(state: &AppState, app: &AppHandle, root: &Path) -> Result<(), String> {
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
    if let Some(state_file) = state_path(app) {
        let vault = state::lock_vault(&state.vault);
        let vault = vault
            .as_ref()
            .ok_or("vault not initialized after reset")?;
        write_seed_slice(vault, &state_file, &canonical_key(root), &docs)?;
    }
    Ok(())
}

/// Reset the isolated test vault to a dev stage. Dev/test only - never touches
/// a manually-connected vault. The frontend reloads status/docs/config/state
/// after this resolves.
#[tauri::command(rename_all = "snake_case")]
pub fn reset_to_stage(
    app: AppHandle,
    state: State<AppState>,
    stage: String,
    backend: String,
    restic_password: Option<String>,
) -> Result<(), String> {
    let root = app_config_dir(&app)?.join("docvault-test-vault");
    reset_to_stage_core(&app, state.inner(), &root, &stage, &backend, restic_password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_state::DesktopStateFile;
    use docvault_storage::{VaultPaths, VaultStorage};

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

    /// A `local-copy` `config.toml` for `paths`. Used by tests that need a
    /// working vault without the external restic binary (`VaultConfig::for_paths`
    /// defaults to restic).
    fn local_copy_config(paths: &VaultPaths) -> String {
        format!(
            "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
            paths.data_dir.display().to_string().replace('\\', "/"),
            paths.repo_dir.display().to_string().replace('\\', "/"),
            paths.db_path.display().to_string().replace('\\', "/"),
        )
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

    /// `example_docs_dir()` must resolve to the repo-root `example_docs` (two
    /// levels up from `src-tauri`), and every seed file must be present. Guards
    /// against regressing the path back to one level up (`apps/desktop`), which
    /// silently skipped seeding via `example_docs_available` and broke stage 3.
    #[test]
    fn example_docs_dir_resolves_to_repo_root_with_seed_files() {
        let dir = example_docs_dir();
        assert!(
            dir.is_dir(),
            "example_docs not found at {} (expected repo-root example_docs)",
            dir.display()
        );
        for (file, _) in SEED_ENTRIES {
            assert!(dir.join(file).exists(), "missing seed file: {file}");
        }
    }

    #[test]
    fn seed_three_docs_imports_one_per_type() {
        let Some(example_docs) = example_docs_available() else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let paths = VaultPaths::from_root(&root);
        // Write a local-copy config so the vault does not default to restic
        // (`VaultConfig::for_paths` defaults to restic), which would need the
        // external binary and is unavailable in unit tests.
        fs::create_dir_all(&paths.root_dir).unwrap();
        fs::write(&paths.config_path, local_copy_config(&paths)).unwrap();
        let storage = VaultStorage::init(paths).unwrap();
        let vault = DocVault::new(storage);
        let cancel = AtomicBool::new(false);

        let docs = seed_three_docs(&vault, &example_docs, &cancel).unwrap();

        assert_eq!(docs.len(), 3);
        let names: Vec<String> = docs.iter().map(|d| d.name.clone()).collect();
        assert!(names.contains(&"Report".to_owned()));
        assert!(names.contains(&"Slides".to_owned()));
        assert!(names.contains(&"Table".to_owned()));
        assert_eq!(vault.list_documents().unwrap().len(), 3);
    }

    #[test]
    fn write_seed_slice_records_tags_and_library_path() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let paths = VaultPaths::from_root(&root);
        fs::create_dir_all(&paths.root_dir).unwrap();
        fs::write(&paths.config_path, local_copy_config(&paths)).unwrap();
        let vault = DocVault::new(VaultStorage::init(paths).unwrap());
        let cancel = AtomicBool::new(false);

        // Commit a real .docx so the document has a current version to materialize.
        let package_dir = temp.path().join("pkg").join("Report");
        fs::create_dir_all(package_dir.join("word")).unwrap();
        fs::write(package_dir.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(package_dir.join("word").join("document.xml"), b"v1").unwrap();
        let source = temp.path().join("report.docx");
        docvault_ooxml::pack_package(package_dir, &source).unwrap();
        let (doc, _ver) = vault
            .commit_document(
                &source,
                DocumentRef::NewName("Report".to_owned()),
                CommitMetadata::default(),
                &cancel,
            )
            .unwrap();
        let doc_id = doc.id.as_str().to_owned();
        let state_file = temp.path().join("desktop-state.json");

        write_seed_slice(&vault, &state_file, "/test-root", std::slice::from_ref(&doc)).unwrap();

        let file = load_file_at(&state_file).unwrap();
        let slice = file.vaults.get("/test-root").expect("slice written");
        assert_eq!(
            slice.tags.get(&doc_id).unwrap(),
            &["draft".to_owned(), "important".to_owned()]
        );
        assert_eq!(slice.tracked.len(), 1);
        assert_eq!(slice.tracked[0].document_id, doc_id);
        // The tracked path is the library copy, not the example-doc source.
        let expected = root.join("library").join(format!("Report-{doc_id}.docx"));
        assert_eq!(slice.tracked[0].path, expected.display().to_string());
        assert!(expected.exists(), "library copy materialized");
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
