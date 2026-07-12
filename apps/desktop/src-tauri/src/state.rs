use std::sync::{Arc, Mutex};

use docvault_core::DocVault;
use docvault_jobs::JobRegistry;
use docvault_storage::{VaultPaths, VaultStorage};
use docvault_types::VaultConfig;

/// Shared application state. The vault is `None` until the user initializes it
/// (first run); once initialized it is opened on startup. `rusqlite::Connection`
/// is `Send` but not `Sync`, so the whole vault is guarded by a `Mutex`.
///
/// The vault lives behind an `Arc` so background job threads can clone the
/// handle and lock it independently of the Tauri command that spawned them.
/// `jobs` is the authoritative job state machine; the UI mirrors it via events.
pub struct AppState {
    pub vault: Arc<Mutex<Option<DocVault>>>,
    pub jobs: JobRegistry,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            vault: Arc::new(Mutex::new(None)),
            jobs: JobRegistry::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Open the vault on startup if it has already been initialized (config.toml
/// exists). A missing config means first run; the UI will prompt to init.
pub fn open_if_initialized(state: &AppState) {
    let paths = VaultPaths::from_env();
    if !paths.config_path.exists() {
        return;
    }
    if let Ok(storage) = VaultStorage::open(paths) {
        *state.vault.lock().expect("vault mutex poisoned") = Some(DocVault::new(storage));
    }
}

/// Initialize the vault for the first time and store it in app state. Uses the
/// `local-copy` backend so the prototype does not depend on the external restic
/// binary; restic remains selectable later as the cloud on-ramp.
pub fn init_vault(state: &AppState) -> Result<(), String> {
    let paths = VaultPaths::from_env();
    ensure_local_copy_config(&paths)?;
    let storage = VaultStorage::init(paths).map_err(|e| e.to_string())?;
    *state.vault.lock().expect("vault mutex poisoned") = Some(DocVault::new(storage));
    Ok(())
}

/// Write a `local-copy` config before first init. Only writes when no config
/// exists, so re-running init never clobbers an existing vault.
fn ensure_local_copy_config(paths: &VaultPaths) -> Result<(), String> {
    if paths.config_path.exists() {
        return Ok(());
    }
    if let Some(parent) = paths.config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut config = VaultConfig::for_paths(
        paths.data_dir.clone(),
        paths.repo_dir.clone(),
        paths.db_path.clone(),
    );
    config.storage.backend = "local-copy".to_owned();
    let rendered = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&paths.config_path, rendered).map_err(|e| e.to_string())?;
    Ok(())
}
