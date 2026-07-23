mod commands;
mod devtools;
mod dto;
mod jobs;
mod library;
mod local_state;
mod logging;
mod prefs;
mod state;

use std::path::{Path, PathBuf};

use state::AppState;
use tauri::Manager;

/// Resolve the bundled restic binary: the packaged Tauri resource in a built
/// app, falling back to the `third_party` asset at its repo-relative location in
/// a dev build. Returns `None` when neither is present (the storage layer then
/// falls back to the system PATH). The result is stashed in [`AppState`] at
/// startup and injected into every vault open/init as an explicit
/// [`docvault_storage::StorageOverrides`] `restic_path` - so the bundled binary
/// is used regardless of where the vault lives, replacing the former
/// `DOCVAULT_RESTIC_PATH` env var.
fn bundled_restic_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::path::BaseDirectory;
    let resource = format!("resources/{}", restic_binary_name());
    if let Ok(path) = app.path().resolve(&resource, BaseDirectory::Resource) {
        if path.exists() {
            return Some(path);
        }
    }
    dev_restic_asset()
}

/// The platform-appropriate restic executable filename (`restic.exe` on Windows,
/// `restic` elsewhere). Mirrors the name `build.rs` stages and the Tauri
/// resource glob (`resources/restic*`) matches.
fn restic_binary_name() -> &'static str {
    if cfg!(windows) {
        "restic.exe"
    } else {
        "restic"
    }
}

/// Map the running host to the target triple we vendor restic for. `None` for
/// hosts we don't ship a binary for (the storage layer then falls back to PATH).
fn host_target_triple() -> Option<&'static str> {
    use std::env::consts::{ARCH, OS};
    match (OS, ARCH) {
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        _ => None,
    }
}

/// In a dev build the `third_party` restic asset for the host platform sits at a
/// fixed path relative to this crate's manifest dir. `None` when the host isn't
/// vendored or the asset hasn't been fetched (`npm run restic:fetch`). Pure so
/// the lookup is unit-testable.
fn dev_restic_asset() -> Option<PathBuf> {
    let triple = host_target_triple()?;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../third_party/restic/0.19.1")
        .join(triple)
        .join(restic_binary_name());
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

pub fn run() {
    tauri::Builder::default()
        // Registered first so a second launch focuses the existing main window
        // instead of starting a duplicate instance. Must precede all other
        // plugins (the plugin's own docs require it be the first plugin).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            // Resolve the bundled restic binary once and stash it in app state;
            // every vault open/init injects it as an explicit override. `None`
            // -> the storage layer falls back to its own auto-discovery (exe dir
            // / third_party beside the vault / PATH).
            let restic_path = bundled_restic_path(app.handle());
            *app.state::<AppState>().inner().restic_path.lock().unwrap() = restic_path;
            // Install the tracing subscriber (rolling file under the app config
            // dir) before opening the vault, so open_if_initialized's
            // reload_log_level has a subscriber to configure. `None` when the
            // config dir is unavailable - logging then stays off, matching prior
            // behavior (all tracing calls were discarded).
            *app.state::<AppState>().inner().logger.lock().unwrap() = logging::init(app.handle());
            state::open_if_initialized(app.handle(), app.state::<AppState>().inner());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::list_documents_with_versions,
            commands::get_config,
            commands::set_log_level,
            commands::connect_vault,
            commands::probe_vault,
            commands::open_devtools,
            commands::repo_size,
            commands::preview_version,
            jobs::commit_document,
            jobs::create_blank_document,
            jobs::export_version,
            jobs::checkout_version,
            jobs::delete_document,
            jobs::delete_versions,
            jobs::rename_document,
            jobs::set_version_note,
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
            library::export_working_copy,
            library::preview_working_copy,
            devtools::reset_to_stage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocVault desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restic_binary_name_matches_platform() {
        assert_eq!(
            restic_binary_name(),
            if cfg!(windows) {
                "restic.exe"
            } else {
                "restic"
            }
        );
    }

    #[test]
    fn host_target_triple_resolves_on_supported_hosts() {
        use std::env::consts::{ARCH, OS};
        // Every (OS, ARCH) we expect to build on must map to a triple, and no
        // other combination may claim one.
        let triple = host_target_triple();
        let supported = matches!(
            (OS, ARCH),
            ("windows", "x86_64")
                | ("macos", "x86_64")
                | ("macos", "aarch64")
                | ("linux", "x86_64")
                | ("linux", "aarch64")
        );
        assert_eq!(
            triple.is_some(),
            supported,
            "host ({OS}, {ARCH}) -> {triple:?}, expected Some == {supported}"
        );
    }

    #[test]
    fn dev_restic_asset_path_shape_when_present() {
        // Only asserts when the host's asset has actually been fetched, so this
        // passes on a fresh checkout without restic and asserts the path shape
        // once `npm run restic:fetch` has populated it.
        if let Some(path) = dev_restic_asset() {
            let triple = host_target_triple()
                .expect("host_target_triple must be Some when dev_restic_asset is Some");
            let expected = format!(
                "third_party/restic/0.19.1/{triple}/{}",
                restic_binary_name()
            );
            assert!(path.ends_with(&expected), "unexpected path: {path:?}");
            assert!(path.exists(), "asset should exist: {path:?}");
        }
    }
}
