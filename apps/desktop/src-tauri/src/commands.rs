use std::process::Command;

use tauri::State;

use docvault_storage::{DocumentRef, VaultPaths};
use docvault_types::VaultConfig;

use crate::dto::{ConfigDto, DocumentWithVersions, VaultStatusDto};
use crate::state::{self, AppState};

#[tauri::command]
pub fn vault_status(state: State<AppState>) -> Result<VaultStatusDto, String> {
    let vault = state.vault.lock().expect("vault mutex poisoned");
    Ok(VaultStatusDto {
        initialized: vault.is_some(),
        root_dir: VaultPaths::from_env().root_dir.display().to_string(),
    })
}

#[tauri::command]
pub fn init_vault(state: State<AppState>) -> Result<(), String> {
    state::init_vault(state.inner())
}

#[tauri::command]
pub fn list_documents_with_versions(
    state: State<AppState>,
) -> Result<Vec<DocumentWithVersions>, String> {
    let vault = state.vault.lock().expect("vault mutex poisoned");
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
    let vault = state.vault.lock().expect("vault mutex poisoned");
    let vault = vault.as_ref().ok_or("vault not initialized")?;
    let paths = vault.paths();
    let (log_level, log_file) = read_logging(&paths.config_path)?;
    Ok(ConfigDto {
        backend: vault.backend().as_str().to_owned(),
        data_dir: paths.data_dir.display().to_string(),
        repo_dir: paths.repo_dir.display().to_string(),
        db_path: paths.db_path.display().to_string(),
        restic_path: vault.restic_path().display().to_string(),
        restic_version: restic_version(vault.restic_path()),
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

/// Best-effort `restic version` capture; empty when the binary is unavailable
/// (e.g. local-copy backend without restic on PATH).
fn restic_version(restic_path: &std::path::Path) -> String {
    let Ok(output) = Command::new(restic_path).arg("version").output() else {
        return String::new();
    };
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_owned()
}
