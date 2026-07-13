mod commands;
mod dto;
mod jobs;
mod prefs;
mod state;

use state::AppState;
use tauri::Manager;

/// `DOCVAULT_*` environment variables that the storage layer lets override the
/// on-disk `config.toml`. The desktop app clears them at startup so the config
/// file (and the user's Settings choices) is the single source of truth - stale
/// vars from a dev shell would otherwise silently force restic mode. The CLI
/// binary is separate and keeps env support.
const DOCVAULT_ENV_VARS: &[&str] = &[
    "DOCVAULT_ROOT_DIR",
    "DOCVAULT_DATA_DIR",
    "DOCVAULT_DB_PATH",
    "DOCVAULT_BACKUP_BACKEND",
    "DOCVAULT_RESTIC_PATH",
    "DOCVAULT_RESTIC_PASSWORD",
];

fn clear_docvault_env() {
    for key in DOCVAULT_ENV_VARS {
        std::env::remove_var(key);
    }
}

pub fn run() {
    clear_docvault_env();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            state::open_if_initialized(app.handle(), app.state::<AppState>().inner());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::init_vault,
            commands::list_documents_with_versions,
            commands::get_config,
            commands::connect_vault,
            commands::open_devtools,
            jobs::commit_document,
            jobs::export_version,
            jobs::checkout_version,
            jobs::list_jobs,
            jobs::cancel_job,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocVault desktop");
}
