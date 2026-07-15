mod commands;
mod devtools;
mod dto;
mod jobs;
mod library;
mod local_state;
mod prefs;
mod state;

use std::path::{Path, PathBuf};

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

/// Resolve the bundled restic binary: the packaged Tauri resource in a built
/// app, falling back to the `third_party` asset at its repo-relative location in
/// a dev build. Returns `None` when neither is present (the storage layer then
/// falls back to the system PATH). The storage layer reads `DOCVAULT_RESTIC_PATH`
/// ahead of `config.toml`, so setting this env var at startup makes every vault
/// connection use the bundled binary regardless of where the vault lives -
/// fixing dev with a non-repo vault and packaged installs with no system restic.
fn bundled_restic_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::path::BaseDirectory;
    if let Ok(path) = app.path().resolve("resources/restic.exe", BaseDirectory::Resource) {
        if path.exists() {
            return Some(path);
        }
    }
    dev_restic_asset()
}

/// In a dev build the `third_party` restic asset sits at a fixed path relative
/// to this crate's manifest dir. `None` off-Windows (no asset shipped) or when
/// the asset is absent. Pure so the lookup is unit-testable.
fn dev_restic_asset() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../third_party/restic/0.19.1/x86_64-pc-windows-msvc/restic.exe");
        if path.exists() {
            return Some(path);
        }
    }
    #[allow(unreachable_code)]
    None
}

pub fn run() {
    clear_docvault_env();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            // Point the storage layer at the bundled restic binary before any
            // vault is opened. Cleared above by `clear_docvault_env`, so this is
            // the only source. `None` -> storage falls back to its own
            // auto-discovery (exe dir / third_party beside the vault / PATH).
            if let Some(path) = bundled_restic_path(app.handle()) {
                std::env::set_var("DOCVAULT_RESTIC_PATH", &path);
            }
            state::open_if_initialized(app.handle(), app.state::<AppState>().inner());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::list_documents_with_versions,
            commands::get_config,
            commands::connect_vault,
            commands::open_devtools,
            commands::repo_size,
            jobs::commit_document,
            jobs::export_version,
            jobs::checkout_version,
            jobs::delete_document,
            jobs::rename_document,
            jobs::list_jobs,
            jobs::cancel_job,
            local_state::get_desktop_state,
            local_state::set_desktop_state,
            local_state::stat_files,
            local_state::probe_file,
            library::library_path,
            library::open_library_copy,
            library::remove_library_copy,
            library::ensure_library_copies,
            devtools::reset_to_stage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocVault desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn dev_restic_asset_points_at_repo_third_party() {
        let path = dev_restic_asset()
            .expect("third_party restic asset should exist in this repo on Windows");
        assert!(
            path.ends_with("third_party/restic/0.19.1/x86_64-pc-windows-msvc/restic.exe"),
            "unexpected path: {path:?}"
        );
        assert!(path.exists(), "asset should exist: {path:?}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn dev_restic_asset_absent_off_windows() {
        assert!(dev_restic_asset().is_none());
    }
}
