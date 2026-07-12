mod commands;
mod dto;
mod jobs;
mod state;

use state::AppState;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            state::open_if_initialized(app.state::<AppState>().inner());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::init_vault,
            commands::list_documents_with_versions,
            commands::get_config,
            jobs::commit_document,
            jobs::export_version,
            jobs::checkout_version,
            jobs::list_jobs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocVault desktop");
}
