//! Write commands (commit / export / checkout) backed by the job runner.
//!
//! Each command resolves its `DocumentRef`, hands an executor closure to
//! [`JobRegistry::spawn`], and returns the job id immediately. The executor
//! (see [`execute_commit`] / [`execute_export`] / [`execute_checkout`]) locks
//! the shared vault, calls the `DocVault` method, and maps any error to
//! `String` so the runner stores it verbatim. State changes flow to the UI via
//! the `job:update` Tauri event (see [`make_emitter`]); the frontend never
//! optimistically updates.
//!
//! `target_label` is derived from the backend (not passed by the UI) so the
//! label is authoritative and a missing document fails fast before a job is
//! ever spawned.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use docvault_core::CoreError;
use docvault_jobs::{JobEventCallback, JobKind, JobOutcome, JobRecord};
use docvault_storage::{DocumentRef, ResticError, StorageError};
use docvault_types::CommitMetadata;
use tauri::{AppHandle, Emitter, State};
use tracing::warn;

use crate::state::{self, AppState};

#[tauri::command(rename_all = "snake_case")]
pub fn commit_document(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    document_id: Option<String>,
    new_name: Option<String>,
    author: Option<String>,
    note: Option<String>,
) -> Result<String, String> {
    let (document_ref, target_label) = resolve_commit_ref(&state, document_id, new_name)?;
    let metadata = CommitMetadata { author, note };
    let vault = state.vault.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Commit,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_commit(&vault, &path, document_ref, metadata, cancel)
        },
    );
    Ok(job_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn export_version(
    app: AppHandle,
    state: State<'_, AppState>,
    document_id: String,
    version: String,
    output_path: String,
) -> Result<String, String> {
    let target_label = format!("{} {version}", lookup_document_name(&state, &document_id)?);
    let document_ref = DocumentRef::IdPrefix(document_id);
    let output = PathBuf::from(output_path);
    let vault = state.vault.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Export,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_export(&vault, &document_ref, &version, &output, cancel)
        },
    );
    Ok(job_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn checkout_version(
    app: AppHandle,
    state: State<'_, AppState>,
    document_id: String,
    version: String,
    output_path: Option<String>,
) -> Result<String, String> {
    let target_label = format!("{} {version}", lookup_document_name(&state, &document_id)?);
    let document_ref = DocumentRef::IdPrefix(document_id);
    let output = output_path.map(PathBuf::from);
    let vault = state.vault.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Checkout,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_checkout(&vault, &document_ref, &version, output.as_deref(), cancel)
        },
    );
    Ok(job_id)
}

/// Delete a document and all of its versions (restic snapshots are forgotten
/// + pruned for the restic backend). Runs as a job because forget/prune can be
/// slow. Returns the spawned job id; state arrives via `job:update`.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_document(
    app: AppHandle,
    state: State<'_, AppState>,
    document_id: String,
) -> Result<String, String> {
    let target_label = lookup_document_name(&state, &document_id)?;
    let document_ref = DocumentRef::IdPrefix(document_id);
    let vault = state.vault.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Delete,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_delete(&vault, &document_ref, cancel)
        },
    );
    Ok(job_id)
}

/// Rename a document's display name. Synchronous (a single SQL UPDATE); not a
/// job. Does not touch the on-disk source file or any version's filename.
#[tauri::command(rename_all = "snake_case")]
pub fn rename_document(
    state: State<'_, AppState>,
    document_id: String,
    new_name: String,
) -> Result<(), String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    vault
        .rename_document(&DocumentRef::IdPrefix(document_id), &new_name)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Result<Vec<JobRecord>, String> {
    Ok(state.jobs.list())
}

/// Request cancellation of a running job. Returns whether a live job was found
/// to cancel; the job reaches `Cancelled` only if its work observes the flag
/// (a job that finishes first keeps its real `Succeeded`/`Failed` status).
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) -> Result<bool, String> {
    Ok(state.jobs.cancel(&job_id))
}

// --- executors: the real work, extracted so tests exercise the same code ---

fn execute_commit(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    path: impl AsRef<Path>,
    document_ref: DocumentRef,
    metadata: CommitMetadata,
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    let source_path = path.as_ref();
    let (document, _version) = match vault.commit_document(source_path, document_ref, metadata, cancel) {
        Ok(result) => result,
        Err(CoreError::Storage(StorageError::Restic(ResticError::Cancelled))) => {
            return JobOutcome::Cancelled;
        }
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    // Library model: when the committed source was an external file (add /
    // manual commit), materialize a library copy from the now-current version so
    // the tool owns a working copy. When the source IS the library copy
    // (commit-modified), skip - it already equals the just-committed version, so
    // re-exporting would be a wasteful no-op (especially costly on restic).
    let doc_id = document.id.as_str();
    match crate::library::library_path_for_doc(vault, doc_id) {
        Ok(lib_path) if source_path != lib_path => {
            if let Err(e) = vault.export_version(
                &DocumentRef::IdPrefix(doc_id.to_owned()),
                "current",
                &lib_path,
                cancel,
            ) {
                return JobOutcome::Failed(format!(
                    "committed but failed to materialize library copy: {e}"
                ));
            }
        }
        _ => {} // source is the library copy, or path unknown - nothing to materialize
    }
    JobOutcome::Succeeded
}

fn execute_export(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    version: &str,
    output: &Path,
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    match vault.export_version(document_ref, version, output, cancel) {
        Ok(_) => JobOutcome::Succeeded,
        Err(StorageError::Restic(ResticError::Cancelled)) => JobOutcome::Cancelled,
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

fn execute_checkout(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    version: &str,
    output: Option<&Path>,
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    match vault.checkout_version(document_ref, version, output, cancel) {
        Ok(_) => JobOutcome::Succeeded,
        Err(StorageError::Restic(ResticError::Cancelled)) => JobOutcome::Cancelled,
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

fn execute_delete(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    match vault.delete_document(document_ref, cancel) {
        Ok(_) => JobOutcome::Succeeded,
        Err(CoreError::Storage(StorageError::Restic(ResticError::Cancelled))) => {
            JobOutcome::Cancelled
        }
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

/// Resolve a commit target: an existing document by id (name looked up so the
/// UI gets an authoritative label and a missing doc fails fast), or a new
/// document by name. Exactly one must be supplied.
fn resolve_commit_ref(
    state: &State<AppState>,
    document_id: Option<String>,
    new_name: Option<String>,
) -> Result<(DocumentRef, String), String> {
    match (document_id, new_name) {
        (Some(id), _) => {
            let name = lookup_document_name(state, &id)?;
            Ok((DocumentRef::IdPrefix(id), name))
        }
        (None, Some(name)) => Ok((DocumentRef::NewName(name.clone()), name)),
        (None, None) => Err("either document_id or new_name is required".into()),
    }
}

/// Look up a document's display name by id via a targeted query (not a full
/// document scan). Fails fast with a clear message if the document does not
/// exist, so the UI never spawns a job doomed to fail.
fn lookup_document_name(state: &State<AppState>, id: &str) -> Result<String, String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault not initialized".to_owned())?;
    vault.document_name(id).map_err(|e| e.to_string())
}

/// Build the `on_event` callback that forwards each job snapshot to the UI as a
/// `job:update` event. `AppHandle` is `Send + Sync + Clone`, so the callback is
/// safe to invoke from job threads.
fn make_emitter(app: AppHandle) -> JobEventCallback {
    Arc::new(move |record: JobRecord| {
        if let Err(e) = app.emit("job:update", record) {
            warn!(error = %e, "failed to emit job:update event");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use docvault_core::DocVault;
    use docvault_jobs::{JobRegistry, JobStatus};
    use docvault_storage::{DocumentRef, VaultPaths, VaultStorage};
    use docvault_types::CommitMetadata;

    /// End-to-end proof of the truthfulness contract: a real commit job against
    /// a real local-copy vault reaches `Succeeded`, the error stays `None`, and
    /// the new document/version is visible in the vault afterward.
    #[test]
    fn commit_job_succeeds_and_version_appears() {
        let temp = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp.path());
        write_local_copy_config(&paths);
        let storage = VaultStorage::init(paths).unwrap();
        let vault: Arc<Mutex<Option<DocVault>>> =
            Arc::new(Mutex::new(Some(DocVault::new(storage))));

        let docx = write_source(temp.path(), "report.docx", b"version one");
        let path = docx.to_string_lossy().to_string();

        let registry = JobRegistry::new();
        let (job_id, terminal) = spawn_commit(
            &registry,
            Arc::clone(&vault),
            path,
            DocumentRef::NewName("report".to_owned()),
        );

        wait_for_terminal(&terminal);
        let record = registry.get(&job_id).expect("job recorded");
        assert_eq!(record.status, JobStatus::Succeeded);
        assert!(
            record.error.is_none(),
            "unexpected error: {:?}",
            record.error
        );
        assert!(record.finished_at.is_some());

        let vault = vault.lock().unwrap();
        let vault = vault.as_ref().unwrap();
        let documents = vault.list_documents().unwrap();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].name, "report");
        let versions = vault
            .list_versions(&DocumentRef::IdPrefix(documents[0].id.as_str().to_owned()))
            .unwrap();
        assert_eq!(versions.len(), 1);
        // Library model: committing an external file materializes a library copy
        // at <root>/library/<id>.<ext>.
        let lib_path = crate::library::library_path_for_doc(vault, documents[0].id.as_str())
            .expect("library path resolves");
        assert!(lib_path.exists(), "library copy materialized after commit");
    }

    /// Commit-modified: when the committed source IS the library copy itself,
    /// the executor skips materialize (source == library path) and just archives
    /// it as a new version. Verifies the library-model fast path for the normal
    /// edit-save-commit loop.
    #[test]
    fn commit_modified_commits_library_copy_without_rematerialize() {
        let temp = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp.path());
        write_local_copy_config(&paths);
        let storage = VaultStorage::init(paths).unwrap();
        let vault: Arc<Mutex<Option<DocVault>>> =
            Arc::new(Mutex::new(Some(DocVault::new(storage))));

        let docx = write_source(temp.path(), "report.docx", b"version one");
        let registry = JobRegistry::new();
        let (job_id, terminal) = spawn_commit(
            &registry,
            Arc::clone(&vault),
            docx.to_string_lossy().to_string(),
            DocumentRef::NewName("report".to_owned()),
        );
        wait_for_terminal(&terminal);
        assert_eq!(registry.get(&job_id).unwrap().status, JobStatus::Succeeded);

        let (doc_id, lib_path) = {
            let vault = vault.lock().unwrap();
            let vault = vault.as_ref().unwrap();
            let id = vault.list_documents().unwrap()[0].id.as_str().to_owned();
            let path = crate::library::library_path_for_doc(vault, &id).unwrap();
            (id, path)
        };
        assert!(lib_path.exists(), "library copy exists after add");

        // commit-modified: the source IS the library copy -> executor skips
        // materialize and archives it directly as a new version.
        let (job_id2, terminal2) = spawn_commit(
            &registry,
            Arc::clone(&vault),
            lib_path.to_string_lossy().to_string(),
            DocumentRef::IdPrefix(doc_id.clone()),
        );
        wait_for_terminal(&terminal2);
        let record2 = registry.get(&job_id2).expect("second job recorded");
        assert_eq!(record2.status, JobStatus::Succeeded, "commit-modified should succeed");
        assert!(record2.error.is_none(), "unexpected error: {:?}", record2.error);

        let vault = vault.lock().unwrap();
        let vault = vault.as_ref().unwrap();
        let versions = vault.list_versions(&DocumentRef::IdPrefix(doc_id)).unwrap();
        assert_eq!(versions.len(), 2, "commit-modified added a second version");
    }

    /// The failure path surfaces the backend's error verbatim and marks the job
    /// `Failed` (here: an unsupported non-Office file is rejected by the core).
    #[test]
    fn commit_job_fails_on_unsupported_file() {
        let temp = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp.path());
        write_local_copy_config(&paths);
        let storage = VaultStorage::init(paths).unwrap();
        let vault: Arc<Mutex<Option<DocVault>>> =
            Arc::new(Mutex::new(Some(DocVault::new(storage))));

        let txt = temp.path().join("notes.txt");
        fs::write(&txt, b"not office").unwrap();
        let path = txt.to_string_lossy().to_string();

        let registry = JobRegistry::new();
        let (job_id, terminal) = spawn_commit(
            &registry,
            Arc::clone(&vault),
            path,
            DocumentRef::NewName("notes".to_owned()),
        );

        wait_for_terminal(&terminal);
        let record = registry.get(&job_id).expect("job recorded");
        assert_eq!(record.status, JobStatus::Failed);
        assert!(record
            .error
            .as_deref()
            .is_some_and(|e| e.contains("unsupported")));
        assert!(record.finished_at.is_some());
    }

    fn spawn_commit(
        registry: &JobRegistry,
        vault: Arc<Mutex<Option<DocVault>>>,
        path: String,
        document_ref: DocumentRef,
    ) -> (String, Arc<AtomicUsize>) {
        let terminal = Arc::new(AtomicUsize::new(0));
        let on_event = {
            let terminal = Arc::clone(&terminal);
            Arc::new(move |record: JobRecord| {
                if record.status != JobStatus::Running {
                    terminal.fetch_add(1, Ordering::SeqCst);
                }
            }) as JobEventCallback
        };
        let job_id = registry.spawn(
            JobKind::Commit,
            "report",
            on_event,
            move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
                execute_commit(
                    &vault,
                    &path,
                    document_ref,
                    CommitMetadata::default(),
                    cancel,
                )
            },
        );
        (job_id, terminal)
    }

    /// A job whose work observes the cancel flag and reports `Cancelled` must
    /// reach `Cancelled` (not `Failed`), with no error, and emit its terminal
    /// event. Mirrors how a stalled restic call surfaces cancellation.
    #[test]
    fn cancel_request_marks_running_job_cancelled() {
        let registry = JobRegistry::new();
        let terminal = Arc::new(AtomicUsize::new(0));
        let on_event = {
            let terminal = Arc::clone(&terminal);
            Arc::new(move |record: JobRecord| {
                if record.status != JobStatus::Running {
                    terminal.fetch_add(1, Ordering::SeqCst);
                }
            }) as JobEventCallback
        };
        let job_id = registry.spawn(
            JobKind::Export,
            "report v1",
            on_event,
            |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
                while !cancel.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(2));
                }
                JobOutcome::Cancelled
            },
        );

        thread::sleep(Duration::from_millis(20));
        assert!(registry.cancel(&job_id), "cancel should find a live job");

        let mut waited = 0;
        while terminal.load(Ordering::SeqCst) == 0 && waited < 1000 {
            thread::sleep(Duration::from_millis(2));
            waited += 1;
        }
        assert_eq!(
            terminal.load(Ordering::SeqCst),
            1,
            "cancelled job must emit its terminal event"
        );

        let record = registry.get(&job_id).expect("job recorded");
        assert_eq!(record.status, JobStatus::Cancelled);
        assert!(record.error.is_none(), "cancelled job carries no error");
        assert!(record.finished_at.is_some());
    }

    fn wait_for_terminal(terminal: &AtomicUsize) {
        let mut waited = 0;
        while terminal.load(Ordering::SeqCst) == 0 && waited < 1000 {
            thread::sleep(Duration::from_millis(2));
            waited += 1;
        }
        assert_eq!(terminal.load(Ordering::SeqCst), 1, "job did not finish");
    }

    fn temp_paths(root: &Path) -> VaultPaths {
        VaultPaths::new(root, root.join("data"), root.join("db.sqlite"))
    }

    fn write_local_copy_config(paths: &VaultPaths) {
        fs::create_dir_all(&paths.root_dir).unwrap();
        fs::write(
            &paths.config_path,
            format!(
                "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
                config_path(&paths.data_dir),
                config_path(&paths.repo_dir),
                config_path(&paths.db_path)
            ),
        )
        .unwrap();
    }

    fn write_source(root: &Path, name: &str, contents: &[u8]) -> PathBuf {
        let source_dir = root.join("sources");
        let package_dir = root.join("package-source").join(name);
        fs::create_dir_all(package_dir.join("word")).unwrap();
        fs::write(package_dir.join("[Content_Types].xml"), b"types").unwrap();
        fs::write(package_dir.join("word").join("document.xml"), contents).unwrap();
        fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join(name);
        docvault_ooxml::pack_package(package_dir, &source).unwrap();
        source
    }

    fn config_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }
}
