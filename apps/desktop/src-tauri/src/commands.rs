use tauri::{AppHandle, Manager, State};

use docvault_storage::{DocumentRef, VaultPaths};
use docvault_types::VaultConfig;

use crate::dto::{ConfigDto, ConnectError, ConnectOutcome, DocumentWithVersions, VaultStatusDto};
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
        recommended_root: VaultPaths::default_root().display().to_string(),
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
pub fn get_config(state: State<AppState>) -> Result<ConfigDto, String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    let paths = vault.paths();
    let (log_level, log_file) = read_logging(&paths.config_path)?;
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

/// Read the logging section from the on-disk config. Falls back to `info` /
/// empty when the file or section is absent.
fn read_logging(config_path: &std::path::Path) -> Result<(String, String), String> {
    let Ok(text) = std::fs::read_to_string(config_path) else {
        return Ok(("info".to_owned(), String::new()));
    };
    let config: VaultConfig = toml::from_str(&text).map_err(|e| e.to_string())?;
    Ok((
        config.logging.level,
        config.logging.file.unwrap_or_default(),
    ))
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
    let outcome = state::connect_vault_core(state.inner(), &root_dir, &backend, restic_password)?;
    prefs::save_root(&app, std::path::Path::new(&outcome.root_dir))
        .map_err(|e| ConnectError::Other(e.to_string()))?;
    Ok(outcome)
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
