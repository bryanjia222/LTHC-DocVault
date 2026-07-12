use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use docvault_core::DocVault;
use docvault_jobs::{JobRegistry, JobStatus};
use docvault_storage::{VaultPaths, VaultStorage};
use docvault_types::VaultConfig;
use tauri::AppHandle;

use crate::dto::{ConnectError, ConnectOutcome};
use crate::prefs;

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

/// The vault root the desktop should use: the last user-chosen root from prefs,
/// falling back to the platform default when the user has not yet connected.
pub fn current_root(app: &AppHandle) -> PathBuf {
    prefs::load_root(app).unwrap_or_else(VaultPaths::default_root)
}

/// Open the vault on startup if it has already been initialized (config.toml
/// exists at the current root). A missing config means first run; the UI will
/// prompt to init.
pub fn open_if_initialized(app: &AppHandle, state: &AppState) {
    let paths = VaultPaths::from_root(current_root(app));
    if !paths.config_path.exists() {
        return;
    }
    if let Ok(storage) = VaultStorage::open(paths) {
        *state.vault.lock().expect("vault mutex poisoned") = Some(DocVault::new(storage));
    }
}

/// Initialize the vault for the first time (onboarding) at the platform default
/// root using the `local-copy` backend. The user can later connect a different
/// directory/backend from Settings. Only writes when no config exists, so
/// re-running never clobbers an existing vault.
pub fn init_vault(state: &AppState) -> Result<(), String> {
    let paths = VaultPaths::from_root(VaultPaths::default_root());
    ensure_local_copy_config(&paths)?;
    let storage = VaultStorage::init(paths).map_err(|e| e.to_string())?;
    *state.vault.lock().expect("vault mutex poisoned") = Some(DocVault::new(storage));
    Ok(())
}

/// Connect (and switch to) the vault at `root_dir` using the chosen `backend`.
///
/// - Empty directory -> initialize a new vault with the chosen backend (restic
///   requires a password; the restic binary is auto-discovered by the storage
///   layer, which finds the bundled binary).
/// - Non-empty + recognizable DocVault vault (a parseable `config.toml`) ->
///   open it; the effective backend is whatever the existing config specifies.
/// - Non-empty + unrecognizable -> [`ConnectError::Unrecognized`].
///
/// Refuses to switch while a job is running, so a blocking storage call never
/// has its vault pulled out from under it. This core is AppHandle-free so it can
/// be unit-tested; the Tauri command wrapper persists the chosen root via prefs.
pub fn connect_vault_core(
    state: &AppState,
    root_dir: &str,
    backend: &str,
    restic_password: Option<String>,
) -> Result<ConnectOutcome, ConnectError> {
    let running = state
        .jobs
        .list()
        .iter()
        .any(|record| record.status == JobStatus::Running);
    if running {
        return Err(ConnectError::JobsRunning);
    }

    let root = PathBuf::from(root_dir);
    let paths = VaultPaths::from_root(&root);
    let empty = is_empty_dir(&root)?;

    let (mode, resolved_backend) = if empty {
        write_config(&paths, backend, restic_password.as_deref())?;
        let storage = VaultStorage::init(paths).map_err(|e| ConnectError::Other(e.to_string()))?;
        let backend = backend.to_owned();
        *state.vault.lock().expect("vault mutex poisoned") = Some(DocVault::new(storage));
        ("initialized", backend)
    } else if is_recognized_vault(&paths) {
        let storage = VaultStorage::open(paths).map_err(|e| ConnectError::Other(e.to_string()))?;
        let backend = storage.backend().as_str().to_owned();
        *state.vault.lock().expect("vault mutex poisoned") = Some(DocVault::new(storage));
        ("opened", backend)
    } else {
        return Err(ConnectError::Unrecognized);
    };

    Ok(ConnectOutcome {
        mode: mode.to_owned(),
        backend: resolved_backend,
        root_dir: root_dir.to_owned(),
    })
}

/// `true` when `root` does not exist or has no entries.
fn is_empty_dir(root: &Path) -> Result<bool, ConnectError> {
    if !root.exists() {
        return Ok(true);
    }
    let mut entries = fs::read_dir(root).map_err(|e| ConnectError::Other(e.to_string()))?;
    Ok(entries.next().is_none())
}

/// A directory is a recognizable DocVault vault when it contains a `config.toml`
/// that parses as a `VaultConfig`. Unknown/invalid backends are then rejected by
/// `VaultStorage::open`, which surfaces a clear error.
fn is_recognized_vault(paths: &VaultPaths) -> bool {
    let Ok(text) = fs::read_to_string(&paths.config_path) else {
        return false;
    };
    toml::from_str::<VaultConfig>(&text).is_ok()
}

/// Write a fresh `config.toml` for a newly initialized vault. For restic the
/// password is required; the restic binary path is left unset so the storage
/// layer auto-discovers the bundled binary.
fn write_config(
    paths: &VaultPaths,
    backend: &str,
    restic_password: Option<&str>,
) -> Result<(), ConnectError> {
    if let Some(parent) = paths.config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| ConnectError::Other(e.to_string()))?;
    }
    let mut config = VaultConfig::for_paths(
        paths.data_dir.clone(),
        paths.repo_dir.clone(),
        paths.db_path.clone(),
    );
    config.storage.backend = backend.to_owned();
    if backend == "restic" {
        config.storage.restic_password = restic_password
            .filter(|value| !value.is_empty())
            .ok_or(ConnectError::ResticPasswordRequired)?
            .to_owned();
    }
    let rendered =
        toml::to_string_pretty(&config).map_err(|e| ConnectError::Other(e.to_string()))?;
    fs::write(&paths.config_path, rendered).map_err(|e| ConnectError::Other(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::ConnectError;
    use docvault_storage::{VaultPaths, VaultStorage};
    use std::path::Path;

    /// An empty directory is initialized as a new local-copy vault.
    #[test]
    fn connect_initializes_empty_dir_local_copy() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let state = AppState::new();

        let outcome =
            connect_vault_core(&state, root.to_str().unwrap(), "local-copy", None).unwrap();

        assert_eq!(outcome.mode, "initialized");
        assert_eq!(outcome.backend, "local-copy");
        assert!(root.join("config.toml").exists());
        assert!(state.vault.lock().unwrap().is_some());
    }

    /// A non-empty, recognizable vault (parseable config.toml) is opened, not
    /// re-initialized, and the effective backend comes from the existing config.
    #[test]
    fn connect_opens_recognized_vault() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        // Pre-initialize a local-copy vault directly so the dir is non-empty.
        let paths = VaultPaths::from_root(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&paths.config_path, local_copy_config(&paths)).unwrap();
        VaultStorage::init(paths).unwrap();

        let state = AppState::new();
        let outcome =
            connect_vault_core(&state, root.to_str().unwrap(), "local-copy", None).unwrap();

        assert_eq!(outcome.mode, "opened");
        assert_eq!(outcome.backend, "local-copy");
        assert!(state.vault.lock().unwrap().is_some());
    }

    /// A non-empty directory without a recognizable config is rejected.
    #[test]
    fn connect_rejects_unrecognized_nonempty() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("junk");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("random.txt"), "not a vault").unwrap();

        let state = AppState::new();
        let err = connect_vault_core(&state, root.to_str().unwrap(), "local-copy", None)
            .expect_err("unrecognized dir should be rejected");
        assert!(matches!(err, ConnectError::Unrecognized));
        assert!(state.vault.lock().unwrap().is_none());
    }

    /// The restic backend requires a password; this validation happens before
    /// any restic binary is invoked, so it is testable without restic present.
    #[test]
    fn connect_restic_requires_password() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("vault");
        let state = AppState::new();

        let err = connect_vault_core(&state, root.to_str().unwrap(), "restic", None)
            .expect_err("restic without a password should be rejected");
        assert!(matches!(err, ConnectError::ResticPasswordRequired));
    }

    fn local_copy_config(paths: &VaultPaths) -> String {
        format!(
            "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
            cfg(&paths.data_dir),
            cfg(&paths.repo_dir),
            cfg(&paths.db_path)
        )
    }

    fn cfg(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }
}
