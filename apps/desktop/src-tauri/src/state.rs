use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use docvault_core::DocVault;
use docvault_jobs::{JobRegistry, JobStatus};
use docvault_storage::{VaultPaths, VaultStorage};
use docvault_types::VaultConfig;
use tauri::AppHandle;
use tracing::warn;

use crate::dto::{ConnectError, ConnectOutcome};
use crate::prefs;

/// Shared application state. The vault is `None` until the user initializes it
/// (first run); once initialized it is opened on startup. `rusqlite::Connection`
/// is `Send` but not `Sync`, so the whole vault is guarded by a `Mutex`.
///
/// The vault lives behind an `Arc` so background job threads can clone the
/// handle and lock it independently of the Tauri command that spawned them.
/// `jobs` is the authoritative job state machine; the UI mirrors it via events.
/// `last_open_error` holds the error from the most recent failed attempt to open
/// an already-initialized vault, so the UI can explain the silence.
pub struct AppState {
    pub vault: Arc<Mutex<Option<DocVault>>>,
    pub jobs: JobRegistry,
    pub last_open_error: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            vault: Arc::new(Mutex::new(None)),
            jobs: JobRegistry::new(),
            last_open_error: Arc::new(Mutex::new(None)),
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

/// Lock the shared vault, recovering from a poisoned mutex instead of panicking.
/// A panic in a job executor is caught by the runner, but the unwinding can
/// still poison the mutex via its guard's `Drop`; read paths recover best-effort
/// so a single panic does not cascade into every subsequent command crashing.
pub fn lock_vault(
    vault: &Arc<Mutex<Option<DocVault>>>,
) -> std::sync::MutexGuard<'_, Option<DocVault>> {
    match vault.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("vault mutex poisoned; recovering for best-effort access");
            poisoned.into_inner()
        }
    }
}

/// The error from the last attempt to open an already-initialized vault, if any.
/// Surfaced via `vault_status` so the UI can explain why an existing vault is
/// unavailable instead of silently showing the onboarding screen.
pub fn open_error(state: &AppState) -> Option<String> {
    state
        .last_open_error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub(crate) fn set_open_error(state: &AppState, message: Option<String>) {
    *state
        .last_open_error
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = message;
}

/// Open the vault on startup if the user has previously connected one. Requires
/// a saved root pref (written by every `connect_vault`): with no pref the app
/// stays at onboarding rather than auto-opening whatever vault might happen to
/// live at the default root. This keeps the dev "fresh" reset (which clears the
/// pref) persistent across restarts, and avoids reopening a stale vault left at
/// the default root by an older install. A pref pointing at a missing/unreadable
/// config is treated as first run; a failure to open an existing vault is
/// captured in `last_open_error` (surfaced via `vault_status`) instead of being
/// swallowed.
pub fn open_if_initialized(app: &AppHandle, state: &AppState) {
    let Some(root) = prefs::load_root(app) else {
        return;
    };
    let paths = VaultPaths::from_root(&root);
    if !paths.config_path.exists() {
        return;
    }
    match VaultStorage::open(paths) {
        Ok(storage) => {
            set_open_error(state, None);
            *lock_vault(&state.vault) = Some(DocVault::new(storage));
        }
        Err(e) => {
            warn!(error = %e, root = %root.display(), "failed to open existing vault");
            set_open_error(state, Some(e.to_string()));
        }
    }
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
        *lock_vault(&state.vault) = Some(DocVault::new(storage));
        ("initialized", backend)
    } else if is_recognized_vault(&paths) {
        let storage = VaultStorage::open(paths).map_err(|e| ConnectError::Other(e.to_string()))?;
        let backend = storage.backend().as_str().to_owned();
        *lock_vault(&state.vault) = Some(DocVault::new(storage));
        ("opened", backend)
    } else {
        return Err(ConnectError::Unrecognized);
    };

    // The previous vault's job history is no longer relevant, and any prior open
    // error is now resolved. Safe because the `running` guard above guarantees
    // no job is in flight.
    set_open_error(state, None);
    state.jobs.clear();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::ConnectError;
    use docvault_jobs::{JobKind, JobOutcome};
    use docvault_storage::{VaultPaths, VaultStorage};
    use std::path::Path;
    use std::thread;
    use std::time::Duration;

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

    /// Switching to a new vault clears the job registry so the UI does not show
    /// the previous vault's (terminal) jobs.
    #[test]
    fn connect_clears_jobs_from_previous_vault() {
        let temp = tempfile::tempdir().unwrap();
        let state = AppState::new();

        // First vault + a quickly-terminating job.
        let root = temp.path().join("vault");
        connect_vault_core(&state, root.to_str().unwrap(), "local-copy", None).unwrap();
        let job_id = state
            .jobs
            .spawn(JobKind::Commit, "report", Arc::new(|_| {}), |_, _| {
                JobOutcome::Succeeded
            });
        let mut waited = 0;
        while state
            .jobs
            .get(&job_id)
            .is_some_and(|record| record.status == JobStatus::Running)
            && waited < 500
        {
            thread::sleep(Duration::from_millis(2));
            waited += 1;
        }
        assert!(waited < 500, "job did not finish before switch");
        assert!(!state.jobs.list().is_empty());

        // Switch to a fresh empty vault; the old job record must be gone.
        let root2 = temp.path().join("vault2");
        connect_vault_core(&state, root2.to_str().unwrap(), "local-copy", None).unwrap();
        assert!(
            state.jobs.list().is_empty(),
            "switching vaults should clear the job registry"
        );
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
