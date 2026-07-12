mod commands;
mod dto;
mod state;

use state::AppState;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocVault desktop");
}
