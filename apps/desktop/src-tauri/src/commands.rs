use tauri::{ipc::Response, AppHandle, Manager, State};

use docvault_storage::{DocumentRef, VaultPaths, NEVER_CANCELLED};
use docvault_types::VaultConfig;

use crate::dto::{
    ConfigDto, ConnectError, ConnectOutcome, DocumentWithVersions, VaultProbe, VaultStatusDto,
};
use crate::prefs;
use crate::state::{self, AppState};

#[tauri::command]
pub fn vault_status(app: AppHandle, state: State<AppState>) -> Result<VaultStatusDto, String> {
    // `initialized` reflects the actually-open vault; `root_dir` is the open
    // vault's root when one is open, otherwise the intended (pref/default) root.
    let (initialized, active_root) = {
        let vault = state::lock_vault(&state.vault);
        let active_root = vault.as_ref().map(|v| v.paths().root_dir.clone());
        (vault.is_some(), active_root)
    };
    let root_dir = active_root
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| state::current_root(&app).display().to_string());
    Ok(VaultStatusDto {
        initialized,
        root_dir,
        recommended_root: state::dev_vault_root(&app)
            .unwrap_or_else(VaultPaths::default_root)
            .display()
            .to_string(),
        open_error: state::open_error(state.inner()),
    })
}

#[tauri::command]
pub fn list_documents_with_versions(
    state: State<AppState>,
) -> Result<Vec<DocumentWithVersions>, String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    let documents = vault.list_documents().map_err(|e| e.to_string())?;
    let mut result = Vec::with_capacity(documents.len());
    for document in documents {
        let document_ref = DocumentRef::IdPrefix(document.id.as_str().to_owned());
        let versions = vault
            .list_versions(&document_ref)
            .map_err(|e| e.to_string())?;
        result.push(DocumentWithVersions { document, versions });
    }
    Ok(result)
}

#[tauri::command]
pub fn get_config(app: AppHandle, state: State<AppState>) -> Result<ConfigDto, String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    let paths = vault.paths();
    let log_level = crate::logging::read_level(&paths.config_path);
    // Show where logs actually roll (the fixed app-level logs dir), not the
    // `[logging].file` hint from config - the subscriber always writes under the
    // app config dir regardless of vault.
    let log_file = crate::logging::log_dir(&app)
        .map(|dir| dir.display().to_string())
        .unwrap_or_default();
    Ok(ConfigDto {
        backend: vault.backend().as_str().to_owned(),
        data_dir: paths.data_dir.display().to_string(),
        repo_dir: paths.repo_dir.display().to_string(),
        db_path: paths.db_path.display().to_string(),
        restic_path: vault.restic_path().display().to_string(),
        restic_version: vault.restic_version().to_owned(),
        log_level,
        log_file,
    })
}

/// Change the active vault's log level: validates the level, persists it to the
/// vault's `config.toml` `[logging].level`, and reloads the live tracing
/// subscriber so the new level takes effect immediately (no restart). The level
/// is vault-scoped (each vault keeps its own) and reapplied on every vault open.
#[tauri::command(rename_all = "snake_case")]
pub fn set_log_level(state: State<AppState>, level: String) -> Result<(), String> {
    validate_level(&level)?;
    let config_path = {
        let vault = state::lock_vault(&state.vault);
        let vault = vault.as_ref().ok_or("vault not initialized")?;
        vault.paths().config_path.clone()
    };
    write_log_level(&config_path, &level)?;
    state::reload_log_level(state.inner(), &config_path);
    Ok(())
}

fn validate_level(level: &str) -> Result<(), String> {
    match level {
        "error" | "warn" | "info" | "debug" | "trace" => Ok(()),
        _ => Err(format!("invalid log level: {level}")),
    }
}

/// Persist `[logging].level` into a vault's `config.toml` by round-tripping the
/// full `VaultConfig` (read -> mutate -> `to_string_pretty`, matching how
/// `write_initial_config` writes the file). Preserves every other field and
/// adds the `[logging]` section when absent (its other fields default).
fn write_log_level(config_path: &std::path::Path, level: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let mut config: VaultConfig = toml::from_str(&text).map_err(|e| e.to_string())?;
    config.logging.level = level.to_owned();
    let rendered = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(config_path, rendered).map_err(|e| e.to_string())?;
    Ok(())
}

/// Connect (and switch to) the vault at the chosen directory with the chosen
/// backend. See [`state::connect_vault_core`] for the empty/recognized/unrecognized
/// logic. Refuses while jobs are running.
#[tauri::command(rename_all = "snake_case")]
pub fn connect_vault(
    app: AppHandle,
    state: State<AppState>,
    root_dir: String,
    backend: String,
    restic_password: Option<String>,
) -> Result<ConnectOutcome, ConnectError> {
    if let Some(expected_root) = state::dev_vault_root(&app) {
        if std::path::Path::new(&root_dir) != expected_root.as_path() {
            return Err(ConnectError::Other(format!(
                "development builds may only connect to the isolated vault at {}",
                expected_root.display()
            )));
        }
    }
    let outcome = state::connect_vault_core(state.inner(), &root_dir, &backend, restic_password)?;
    prefs::save_root(&app, std::path::Path::new(&outcome.root_dir))
        .map_err(|e| ConnectError::Other(e.to_string()))?;
    Ok(outcome)
}

/// Probe a directory to classify it as empty/existing/unrecognized before
/// connecting, so the connect dialog can lock the backend selector for an
/// existing vault (whose backend is fixed by its config). See
/// [`state::probe_vault`].
#[tauri::command(rename_all = "snake_case")]
pub fn probe_vault(root_dir: String) -> Result<VaultProbe, String> {
    Ok(state::probe_vault(&root_dir))
}

/// Open the webview devtools (developer mode -> right-click -> inspect). Requires
/// the `devtools` tauri feature, which is enabled for this crate.
#[tauri::command]
pub fn open_devtools(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_owned())?;
    window.open_devtools();
    Ok(())
}

/// On-disk size of the active vault's backup repository, in bytes. For restic
/// this is `restic stats --mode raw-data` (post-dedup + compression); for
/// local-copy it is the archived version files. The UI refreshes this after
/// commits/deletes so the ArchiveView stat stays current.
#[tauri::command]
pub fn repo_size(state: State<AppState>) -> Result<u64, String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    vault.repo_size().map_err(|e| e.to_string())
}

/// Return a version's bytes for in-app preview. Exports the resolved version to
/// a temp file, reads it back, and returns it as a binary `ipc::Response` (no
/// CSP / asset-protocol change needed). Async + `spawn_blocking` so a slow
/// restic restore never freezes the UI: non-async commands run on the main
/// thread, but this one runs the vault lock + I/O on a blocking thread and
/// `await`s the result.
#[tauri::command(rename_all = "snake_case")]
pub async fn preview_version(
    state: State<'_, AppState>,
    document_id: String,
    version: String,
) -> Result<Response, String> {
    let vault = state.vault.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Response, String> {
        let guard = state::lock_vault(&vault);
        let vault = guard.as_ref().ok_or("vault not initialized")?;
        let document_ref = DocumentRef::IdPrefix(document_id);
        let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        // No extension => export writes `temp_dir/original_filename` and returns
        // that path, so read the path export reports (not `preview` itself).
        let exported = vault
            .export_version(
                &document_ref,
                &version,
                temp_dir.path().join("preview"),
                &NEVER_CANCELLED,
            )
            .map_err(|e| e.to_string())?;
        let bytes = std::fs::read(&exported).map_err(|e| e.to_string())?;
        // Drop the temp dir (and its file) now that bytes are in memory.
        drop(temp_dir);
        Ok(Response::new(bytes))
    })
    .await
    .map_err(|e| format!("preview task failed: {e}"))?
}
