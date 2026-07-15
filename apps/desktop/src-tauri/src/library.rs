//! Library / managed-folder model. The desktop owns a per-document working
//! copy (the "library copy") at `<vault_root>/library/<docId>.<ext>`, which
//! always mirrors the document's current version. Users edit it via the editor
//! the tool launches (`open`); `commit-modified` archives it; `checkout`
//! overwrites it; and a missing copy is rebuilt from the archive on demand -
//! the automated replacement for the old manual relink flow.
//!
//! All file-writing here reuses the crate primitives (`export_version` with the
//! `"current"` sentinel) - no storage/core changes. Helpers take a locked
//! `&DocVault` (the caller holds the mutex), so they never re-lock.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use docvault_core::DocVault;
use docvault_storage::{DocumentRef, NEVER_CANCELLED};
use tauri::{AppHandle, State};
use tracing::warn;

use crate::dto::{DesktopStateSlice, TrackedFile};
use crate::local_state::{canonical_key, load_file_at, probe_at, save_file_at, state_path};
use crate::state::{self, AppState};

/// Files above this are not sha256-hashed when baselining, matching the
/// frontend's `MODIFICATION_HASH_THRESHOLD_BYTES` (and `devtools`).
const HASH_THRESHOLD_BYTES: u64 = 50 * 1024 * 1024;

// --- pure helpers (take a locked &DocVault; unit-testable) ---

/// The library directory: `<vault_root>/library`. Created on first write.
pub(crate) fn library_dir(vault: &DocVault) -> PathBuf {
    vault.paths().root_dir.join("library")
}

/// The extension of a document's current version (lowercased), derived from the
/// version's `original_filename`. Errors when the document has no current
/// version or the filename carries no extension.
fn ext_for_doc(vault: &DocVault, doc_id: &str) -> Result<String, String> {
    let version = vault
        .current_version(&DocumentRef::IdPrefix(doc_id.to_owned()))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no current version for document {doc_id}"))?;
    Path::new(&version.original_filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or_else(|| format!(
            "no extension in original filename: {}",
            version.original_filename
        ))
}

/// The deterministic library path for a document:
/// `<vault_root>/library/<docId>.<ext>`. Stable across renames (docId-based)
/// and collision-free.
pub(crate) fn library_path_for_doc(vault: &DocVault, doc_id: &str) -> Result<PathBuf, String> {
    let ext = ext_for_doc(vault, doc_id)?;
    Ok(library_dir(vault).join(format!("{doc_id}.{ext}")))
}

/// Materialize a document's current version to `path` by exporting the
/// `"current"` version. The single operation that (re)creates a library copy.
/// Caller holds the vault lock.
fn materialize_at(
    vault: &DocVault,
    doc_id: &str,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    vault
        .export_version(&DocumentRef::IdPrefix(doc_id.to_owned()), "current", path, cancel)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Core per-vault migration / auto-rebuild: for every document, ensure a library
/// copy exists (materialize the current version if missing) and that the tracked
/// entry points at the library path with a fresh baseline. Pure: takes the slice
/// by `&mut` (no file I/O), so it is unit-testable and reusable by `seed`.
///
/// A correctly-tracked document whose library copy is present is left untouched
/// (its baseline - and thus any "modified" state - is preserved). Only missing
/// copies (rebuilt), stale paths (repointed), and untracked docs get a fresh
/// baseline. Best-effort per document: a materialize failure is logged and that
/// document is skipped rather than aborting the whole pass.
pub(crate) fn ensure_library_copies_for(
    vault: &DocVault,
    slice: &mut DesktopStateSlice,
) -> Result<(), String> {
    let docs = vault.list_documents().map_err(|e| e.to_string())?;
    for doc in &docs {
        let doc_id = doc.id.as_str();
        let lib_path = match library_path_for_doc(vault, doc_id) {
            Ok(p) => p,
            Err(_) => continue, // no current version / no ext - skip
        };
        let needs_baseline = !lib_path.exists();
        if needs_baseline {
            if let Err(error) = vault.export_version(
                &DocumentRef::IdPrefix(doc_id.to_owned()),
                "current",
                &lib_path,
                &NEVER_CANCELLED,
            ) {
                warn!(doc_id, error = %error, "ensure_library_copies: materialize failed");
                continue;
            }
        }
        let lib_path_str = lib_path.display().to_string();
        match slice.tracked.iter_mut().find(|t| t.document_id == doc_id) {
            // Correctly tracked and the file is present - preserve baseline
            // (keep any pending "modified" state intact).
            Some(t) if t.path == lib_path_str && !needs_baseline => continue,
            Some(t) => {
                let probe = probe_at(&lib_path, HASH_THRESHOLD_BYTES);
                t.path = lib_path_str;
                t.size = probe.size;
                t.mtime_ms = probe.mtime_ms;
                t.sha256 = probe.sha256;
            }
            None => {
                let probe = probe_at(&lib_path, HASH_THRESHOLD_BYTES);
                slice.tracked.push(TrackedFile {
                    document_id: doc_id.to_owned(),
                    path: lib_path_str,
                    size: probe.size,
                    mtime_ms: probe.mtime_ms,
                    sha256: probe.sha256,
                });
            }
        }
    }
    Ok(())
}

/// Delete `<library>/<docId>.*` (any extension). Best-effort: a missing library
/// dir or file is not an error. Pure so it is testable without an `AppHandle`.
pub(crate) fn remove_library_copy_at(library_dir: &Path, doc_id: &str) -> Result<(), String> {
    let prefix = format!("{doc_id}.");
    if let Ok(entries) = fs::read_dir(library_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
            {
                let _ = fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

/// Open the library copy (the current-version mirror) in the editor. Materializes
/// it first if missing. This is the editable working copy - edits flow back via
/// commit-modified.
fn open_current_copy(vault: &DocVault, doc_id: &str) -> Result<(), String> {
    let path = library_path_for_doc(vault, doc_id)?;
    if !path.exists() {
        materialize_at(vault, doc_id, &path, &NEVER_CANCELLED)?;
    }
    open::that(&path)
        .map(|_| ())
        .map_err(|e| format!("failed to open editor: {e}"))
}

/// The extension (lowercased) of a specific version's `original_filename`.
fn ext_for_version(vault: &DocVault, doc_id: &str, version_id: &str) -> Result<String, String> {
    let versions = vault
        .list_versions(&DocumentRef::IdPrefix(doc_id.to_owned()))
        .map_err(|e| e.to_string())?;
    let version = versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| format!("version {version_id} not found for document {doc_id}"))?;
    Path::new(&version.original_filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or_else(|| format!("no extension in original filename: {}", version.original_filename))
}

/// Clear the read-only attribute from `path` so it can be overwritten/deleted.
/// The desktop is Windows-only; `set_readonly(false)` clears the
/// FILE_ATTRIBUTE_READONLY bit (the clippy Unix world-writable concern does not
/// apply here).
#[allow(clippy::permissions_set_readonly_false)]
fn clear_readonly(path: &Path) -> Result<(), String> {
    let mut perms = fs::metadata(path)
        .map_err(|e| e.to_string())?
        .permissions();
    perms.set_readonly(false);
    fs::set_permissions(path, perms).map_err(|e| e.to_string())
}

/// Export a non-current version to a read-only temp file and return its path,
/// without touching the library copy or the current-version pointer. The temp
/// file is marked read-only so the editor flags any save attempt (view-only
/// review of an older version). The temp path is reused across opens; a prior
/// read-only copy is cleared first so the export can overwrite it. Pure of the
/// `open::that` side effect, so it is unit-testable.
fn materialize_readonly_temp(
    vault: &DocVault,
    doc_id: &str,
    version_id: &str,
) -> Result<PathBuf, String> {
    let ext = ext_for_version(vault, doc_id, version_id)?;
    let temp_path = std::env::temp_dir().join(format!("docvault-{doc_id}-{version_id}.{ext}"));
    if temp_path.exists() {
        clear_readonly(&temp_path)?;
        fs::remove_file(&temp_path).map_err(|e| e.to_string())?;
    }
    vault
        .export_version(
            &DocumentRef::IdPrefix(doc_id.to_owned()),
            version_id,
            &temp_path,
            &NEVER_CANCELLED,
        )
        .map_err(|e| e.to_string())?;
    let mut perms = fs::metadata(&temp_path)
        .map_err(|e| e.to_string())?
        .permissions();
    perms.set_readonly(true);
    fs::set_permissions(&temp_path, perms).map_err(|e| e.to_string())?;
    Ok(temp_path)
}

/// Open a non-current version read-only in a temp file, without touching the
/// library copy or the current-version pointer.
fn open_version_readonly(vault: &DocVault, doc_id: &str, version_id: &str) -> Result<(), String> {
    let temp_path = materialize_readonly_temp(vault, doc_id, version_id)?;
    open::that(&temp_path)
        .map(|_| ())
        .map_err(|e| format!("failed to open editor: {e}"))
}

// --- AppHandle-bound commands ---

/// The deterministic library path for a document, as a display string. The
/// frontend uses this to know which file checkout should write and which file
/// to track.
#[tauri::command(rename_all = "snake_case")]
pub fn library_path(document_id: String, state: State<AppState>) -> Result<String, String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    let path = library_path_for_doc(vault, &document_id)?;
    Ok(path.display().to_string())
}

/// Open a version of the document in the OS default editor. The current version
/// (or when `version` is None/"current") opens the library copy - the editable
/// working copy, rebuilt from the archive if missing, whose edits flow back via
/// commit-modified. A specific non-current version is exported to a read-only
/// temp file for view-only review, leaving the library copy and the
/// current-version pointer untouched. `version` is a version id (the frontend's
/// `label`); omit it (or pass "current") for the current version.
#[tauri::command(rename_all = "snake_case")]
pub fn open_library_copy(
    document_id: String,
    version: Option<String>,
    state: State<AppState>,
) -> Result<(), String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    let doc_ref = DocumentRef::IdPrefix(document_id.clone());
    let current = vault
        .current_version(&doc_ref)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no current version for document {document_id}"))?;
    match version.as_deref() {
        None | Some("current") => open_current_copy(vault, &document_id),
        Some(v) if v == current.id => open_current_copy(vault, &document_id),
        Some(v) => open_version_readonly(vault, &document_id, v),
    }
}

/// Remove a document's library copy (any extension). Called on delete so the
/// DocVault-owned working file does not linger. Best-effort.
#[tauri::command(rename_all = "snake_case")]
pub fn remove_library_copy(document_id: String, state: State<AppState>) -> Result<(), String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    remove_library_copy_at(&library_dir(vault), &document_id)
}

/// Ensure every document has a library copy and a tracked entry pointing at it.
/// Run on init/connect after `load_desktop_state`: materializes missing copies
/// (migration of pre-library docs + rebuild of deleted copies) and repoints
/// stale tracked paths. Idempotent.
#[tauri::command(rename_all = "snake_case")]
pub fn ensure_library_copies(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    let root_key = canonical_key(&vault.paths().root_dir);
    let Some(state_file) = state_path(&app) else {
        return Ok(()); // no app config dir - nothing to persist (best-effort)
    };
    let mut file = load_file_at(&state_file)?;
    let slice = file.vaults.entry(root_key).or_default();
    ensure_library_copies_for(vault, slice)?;
    save_file_at(&state_file, &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::DesktopStateSlice;
    use docvault_storage::{VaultPaths, VaultStorage};
    use docvault_types::CommitMetadata;
    use std::fs;
    use std::path::Path;

    /// Build a real local-copy vault under `root` for library tests.
    fn vault_at(root: &Path) -> DocVault {
        let paths = VaultPaths::from_root(root);
        fs::create_dir_all(&paths.root_dir).unwrap();
        fs::write(&paths.config_path, local_copy_config(&paths)).unwrap();
        let storage = VaultStorage::init(paths).unwrap();
        DocVault::new(storage)
    }

    fn local_copy_config(paths: &VaultPaths) -> String {
        format!(
            "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
            cfg(&paths.data_dir),
            cfg(&paths.repo_dir),
            cfg(&paths.db_path),
        )
    }

    fn cfg(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    /// Pack a minimal .docx and commit it as a new document; return its id.
    fn commit_docx(vault: &DocVault, root: &Path, name: &str, contents: &[u8]) -> String {
        let package_dir = root.join("pkg").join(name);
        fs::create_dir_all(package_dir.join("word")).unwrap();
        fs::write(package_dir.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(package_dir.join("word").join("document.xml"), contents).unwrap();
        let source = root.join("sources").join(format!("{name}.docx"));
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        docvault_ooxml::pack_package(package_dir, &source).unwrap();
        let (doc, _ver) = vault
            .commit_document(
                &source,
                DocumentRef::NewName(name.to_owned()),
                CommitMetadata::default(),
                &NEVER_CANCELLED,
            )
            .unwrap();
        doc.id.as_str().to_owned()
    }

    /// Commit another version to an existing document (by id); return the new
    /// version id. The new version becomes current; the prior one is archived.
    fn commit_version(vault: &DocVault, root: &Path, doc_id: &str, name: &str, contents: &[u8]) -> String {
        let package_dir = root.join("pkg").join(name);
        fs::create_dir_all(package_dir.join("word")).unwrap();
        fs::write(package_dir.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(package_dir.join("word").join("document.xml"), contents).unwrap();
        let source = root.join("sources").join(format!("{name}.docx"));
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        docvault_ooxml::pack_package(package_dir, &source).unwrap();
        let (_doc, ver) = vault
            .commit_document(
                &source,
                DocumentRef::IdPrefix(doc_id.to_owned()),
                CommitMetadata::default(),
                &NEVER_CANCELLED,
            )
            .unwrap();
        ver.id.as_str().to_owned()
    }

    /// `library_path_for_doc` returns `<root>/library/<id>.docx` for a committed
    /// docx, deriving the extension from the current version's filename.
    #[test]
    fn library_path_is_docid_plus_ext_under_library_dir() {
        let temp = tempfile::tempdir().unwrap();
        let vault = vault_at(temp.path());
        let doc_id = commit_docx(&vault, temp.path(), "report", b"v1");

        let path = library_path_for_doc(&vault, &doc_id).unwrap();
        assert_eq!(path, temp.path().join("library").join(format!("{doc_id}.docx")));
    }

    /// `materialize_at` writes a real .docx library copy from the current version.
    #[test]
    fn materialize_at_writes_library_copy() {
        let temp = tempfile::tempdir().unwrap();
        let vault = vault_at(temp.path());
        let doc_id = commit_docx(&vault, temp.path(), "report", b"v1");

        let lib_path = library_path_for_doc(&vault, &doc_id).unwrap();
        assert!(!lib_path.exists());
        materialize_at(&vault, &doc_id, &lib_path, &NEVER_CANCELLED).unwrap();
        assert!(lib_path.exists(), "library copy should exist after materialize");
        assert!(lib_path.metadata().unwrap().len() > 0, "library copy non-empty");
    }

    /// `ensure_library_copies_for` on a fresh slice materializes the copy and
    /// records a tracked entry pointing at the library path.
    #[test]
    fn ensure_creates_copy_and_tracks_library_path() {
        let temp = tempfile::tempdir().unwrap();
        let vault = vault_at(temp.path());
        let doc_id = commit_docx(&vault, temp.path(), "report", b"v1");

        let mut slice = DesktopStateSlice::default();
        ensure_library_copies_for(&vault, &mut slice).unwrap();

        assert_eq!(slice.tracked.len(), 1);
        assert_eq!(slice.tracked[0].document_id, doc_id);
        let expected = temp.path().join("library").join(format!("{doc_id}.docx"));
        assert_eq!(slice.tracked[0].path, expected.display().to_string());
        assert!(expected.exists(), "library copy materialized");
        assert!(slice.tracked[0].size > 0, "baseline size probed");
    }

    /// A second `ensure` pass is a no-op for a correctly-tracked doc whose copy
    /// is present - the baseline (and any "modified" state) is preserved.
    #[test]
    fn ensure_preserves_correctly_tracked_doc() {
        let temp = tempfile::tempdir().unwrap();
        let vault = vault_at(temp.path());
        let _doc_id = commit_docx(&vault, temp.path(), "report", b"v1");

        let mut slice = DesktopStateSlice::default();
        ensure_library_copies_for(&vault, &mut slice).unwrap();
        let baseline_size = slice.tracked[0].size;
        let baseline_mtime = slice.tracked[0].mtime_ms;

        // Mutate the baseline to simulate a "modified" state, then re-ensure.
        slice.tracked[0].size = baseline_size + 9999;
        slice.tracked[0].mtime_ms = 1;
        ensure_library_copies_for(&vault, &mut slice).unwrap();

        // Preserved: not overwritten by a fresh probe.
        assert_eq!(slice.tracked[0].size, baseline_size + 9999);
        assert_eq!(slice.tracked[0].mtime_ms, 1);
        let _ = baseline_mtime;
    }

    /// `ensure` rebuilds a deleted library copy and re-baselines it.
    #[test]
    fn ensure_rebuilds_missing_copy_and_rebaselines() {
        let temp = tempfile::tempdir().unwrap();
        let vault = vault_at(temp.path());
        let doc_id = commit_docx(&vault, temp.path(), "report", b"v1");

        let mut slice = DesktopStateSlice::default();
        ensure_library_copies_for(&vault, &mut slice).unwrap();
        let lib_path = temp.path().join("library").join(format!("{doc_id}.docx"));
        fs::remove_file(&lib_path).unwrap();
        // Stale baseline (claims a size for a now-missing file).
        slice.tracked[0].size = 12345;

        ensure_library_copies_for(&vault, &mut slice).unwrap();

        assert!(lib_path.exists(), "copy rebuilt");
        assert_ne!(slice.tracked[0].size, 12345, "baseline refreshed after rebuild");
    }

    /// `ensure` repoints a stale tracked path (pre-library model) to the library
    /// path and re-baselines.
    #[test]
    fn ensure_repoints_stale_path_to_library() {
        let temp = tempfile::tempdir().unwrap();
        let vault = vault_at(temp.path());
        let doc_id = commit_docx(&vault, temp.path(), "report", b"v1");

        let mut slice = DesktopStateSlice {
            tracked: vec![TrackedFile {
                document_id: doc_id.clone(),
                path: "/old/elsewhere/report.docx".to_owned(),
                size: 1,
                mtime_ms: 1,
                sha256: None,
            }],
            ..Default::default()
        };
        ensure_library_copies_for(&vault, &mut slice).unwrap();

        let expected = temp.path().join("library").join(format!("{doc_id}.docx"));
        assert_eq!(slice.tracked[0].path, expected.display().to_string());
        assert!(slice.tracked[0].size > 1, "baseline refreshed after repoint");
    }

    /// `remove_library_copy_at` deletes `<lib>/<id>.*` and leaves siblings.
    #[test]
    fn remove_library_copy_globs_by_docid() {
        let temp = tempfile::tempdir().unwrap();
        let lib = temp.path().join("library");
        fs::create_dir_all(&lib).unwrap();
        fs::write(lib.join("docA.docx"), b"a").unwrap();
        fs::write(lib.join("docA.bak"), b"bak").unwrap(); // also matched (prefix)
        fs::write(lib.join("docB.docx"), b"b").unwrap(); // sibling, kept

        remove_library_copy_at(&lib, "docA").unwrap();

        assert!(!lib.join("docA.docx").exists());
        assert!(!lib.join("docA.bak").exists(), "prefix match also removed");
        assert!(lib.join("docB.docx").exists(), "sibling kept");
    }

    /// `remove_library_copy_at` is a no-op when the library dir is absent.
    #[test]
    fn remove_library_copy_missing_dir_is_ok() {
        let temp = tempfile::tempdir().unwrap();
        remove_library_copy_at(&temp.path().join("nope"), "docA").unwrap();
    }

    /// `ext_for_version` derives the extension from a specific (non-current)
    /// version's `original_filename`, not just the current one.
    #[test]
    fn ext_for_version_uses_requested_version_filename() {
        let temp = tempfile::tempdir().unwrap();
        let vault = vault_at(temp.path());
        let doc_id = commit_docx(&vault, temp.path(), "report", b"v1");
        let v1_id = vault
            .current_version(&DocumentRef::IdPrefix(doc_id.clone()))
            .unwrap()
            .unwrap()
            .id
            .as_str()
            .to_owned();
        let _v2_id = commit_version(&vault, temp.path(), &doc_id, "report_v2", b"v2");

        // v1 is now archived (non-current); its extension is still .docx.
        assert_eq!(ext_for_version(&vault, &doc_id, &v1_id).unwrap(), "docx");
    }

    /// `materialize_readonly_temp` exports the requested (non-current) version to
    /// a read-only temp file whose contents differ from the current version -
    /// proving the archived version, not the current one, was materialized.
    #[test]
    fn materialize_readonly_temp_exports_requested_version_readonly() {
        let temp = tempfile::tempdir().unwrap();
        let vault = vault_at(temp.path());
        let doc_id = commit_docx(&vault, temp.path(), "report", b"v1");
        let v1_id = vault
            .current_version(&DocumentRef::IdPrefix(doc_id.clone()))
            .unwrap()
            .unwrap()
            .id
            .as_str()
            .to_owned();
        let v2_id = commit_version(&vault, temp.path(), &doc_id, "report_v2", b"v2");
        // v2 is now current; v1 is archived.

        let temp_v1 = materialize_readonly_temp(&vault, &doc_id, &v1_id).unwrap();
        assert!(temp_v1.exists(), "temp file created");
        assert_eq!(
            temp_v1.extension().and_then(|e| e.to_str()),
            Some("docx"),
            "temp file carries the version's extension"
        );
        assert!(
            fs::metadata(&temp_v1).unwrap().permissions().readonly(),
            "temp file is read-only"
        );

        // Distinct from a current-version (v2) export -> the archived version
        // was exported, not the current one.
        let temp_v2 = materialize_readonly_temp(&vault, &doc_id, &v2_id).unwrap();
        assert_ne!(
            fs::read(&temp_v1).unwrap(),
            fs::read(&temp_v2).unwrap(),
            "v1 and v2 temp files differ"
        );

        // Clear read-only so the tempdir cleanup can delete them.
        for path in [&temp_v1, &temp_v2] {
            let _ = clear_readonly(path);
            let _ = fs::remove_file(path);
        }
    }
}
