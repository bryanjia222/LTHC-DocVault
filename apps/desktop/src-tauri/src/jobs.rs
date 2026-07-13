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
use std::sync::Arc;

use docvault_jobs::{JobEventCallback, JobKind, JobRecord};
use docvault_storage::DocumentRef;
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
        move |_: &dyn Fn(Option<f64>)| -> Result<(), String> {
            execute_commit(&vault, &path, document_ref, metadata)
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
        move |_: &dyn Fn(Option<f64>)| -> Result<(), String> {
            execute_export(&vault, &document_ref, &version, &output)
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
        move |_: &dyn Fn(Option<f64>)| -> Result<(), String> {
            execute_checkout(&vault, &document_ref, &version, output.as_deref())
        },
    );
    Ok(job_id)
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Result<Vec<JobRecord>, String> {
    Ok(state.jobs.list())
}

// --- executors: the real work, extracted so tests exercise the same code ---

fn execute_commit(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    path: impl AsRef<Path>,
    document_ref: DocumentRef,
    metadata: CommitMetadata,
) -> Result<(), String> {
    let vault = vault.lock().map_err(|e| e.to_string())?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault not initialized".to_owned())?;
    vault
        .commit_document(path, document_ref, metadata)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_export(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    version: &str,
    output: &Path,
) -> Result<(), String> {
    let vault = vault.lock().map_err(|e| e.to_string())?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault not initialized".to_owned())?;
    vault
        .export_version(document_ref, version, output)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_checkout(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    version: &str,
    output: Option<&Path>,
) -> Result<(), String> {
    let vault = vault.lock().map_err(|e| e.to_string())?;
    let vault = vault
        .as_ref()
        .ok_or_else(|| "vault not initialized".to_owned())?;
    vault
        .checkout_version(document_ref, version, output)
        .map_err(|e| e.to_string())?;
    Ok(())
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
    use std::sync::atomic::{AtomicUsize, Ordering};
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
            move |_: &dyn Fn(Option<f64>)| -> Result<(), String> {
                execute_commit(&vault, &path, document_ref, CommitMetadata::default())
            },
        );
        (job_id, terminal)
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
