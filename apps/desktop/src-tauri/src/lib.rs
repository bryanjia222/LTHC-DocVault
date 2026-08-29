mod bridge;
mod commands;
mod devtools;
mod dto;
mod jobs;
mod library;
mod local_state;
mod logging;
mod platform;
mod prefs;
mod preview_cache;
mod qinbixin;
mod state;
mod web;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

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
    let resource = format!("resources/{}", platform::restic_binary_name());
    if let Ok(path) = app.path().resolve(&resource, BaseDirectory::Resource) {
        if path.exists() {
            return Some(path);
        }
    }
    dev_restic_asset()
}

/// In a dev build the `third_party` restic asset for the host platform sits at a
/// fixed path relative to this crate's manifest dir. `None` when the host isn't
/// vendored or the asset hasn't been fetched (`npm run restic:fetch`). Pure so
/// the lookup is unit-testable.
fn dev_restic_asset() -> Option<PathBuf> {
    let triple = platform::host_target_triple()?;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../third_party/restic/0.19.1")
        .join(triple)
        .join(platform::restic_binary_name());
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

pub fn run() {
    // Linux graphics-compat prep (first-run EGL probe + software-rendering env
    // injection) runs before the builder so the env vars are in place before
    // any webview inits. Compile-time no-op off Linux; all platform knowledge
    // lives in the `platform` module.
    platform::prepare_boot();
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
            // Stamp the window title with the crate version so the title bar
            // always matches Cargo.toml (the version the CI stamps from the
            // release tag). tauri.conf.json carries the title without a version
            // as the pre-load value.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(&format!(
                    "兰天嗨彩办公文档管理 v{}",
                    env!("CARGO_PKG_VERSION")
                ));
            }
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
            qinbixin::load_session(app.handle(), &app.state::<AppState>().inner().qinbixin);
            state::open_if_initialized(app.handle(), app.state::<AppState>().inner());
            // Start the loopback add-in bridge so Word/Excel/PPT add-ins can POST
            // the active document straight into the vault. A bind failure (port
            // taken) only disables the bridge for the session - the add-in then
            // reports the app as offline instead of erroring.
            if let Err(e) = bridge::start(app.handle().clone(), app.state::<AppState>().inner()) {
                tracing::warn!(error = %e, "add-in bridge failed to start");
            }
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
            preview_cache::read_preview_cache,
            preview_cache::write_preview_cache,
            preview_cache::clear_preview_cache,
            preview_cache::list_preview_cache,
            web::fetch_url_meta,
            web::open_url,
            qinbixin::qinbixin_status,
            qinbixin::qinbixin_login,
            qinbixin::qinbixin_logout,
            qinbixin::qinbixin_conversations,
            qinbixin::qinbixin_messages,
            qinbixin::qinbixin_inbox,
            qinbixin::qinbixin_outbox,
            qinbixin::qinbixin_send,
            qinbixin::qinbixin_upload,
            qinbixin::qinbixin_thumbnail,
            qinbixin::qinbixin_mark_read,
            #[cfg(debug_assertions)]
            qinbixin::qinbixin_set_environment,
            #[cfg(debug_assertions)]
            qinbixin::qinbixin_dev_accounts,
            #[cfg(debug_assertions)]
            qinbixin::qinbixin_login_dev_account,
            devtools::reset_to_stage,
        ])
        .build(tauri::generate_context!())
        .expect("error while building DocVault desktop")
        .run(|app_handle: &tauri::AppHandle, event: tauri::RunEvent| {
            // Signal the add-in bridge to stop on exit so its accept-loop thread
            // (blocked on `recv_timeout`) returns instead of lingering.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.bridge_stop.store(true, Ordering::Relaxed);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_restic_asset_path_shape_when_present() {
        // Only asserts when the host's asset has actually been fetched, so this
        // passes on a fresh checkout without restic and asserts the path shape
        // once `npm run restic:fetch` has populated it. The platform helpers
        // (restic_binary_name / host_target_triple) now live in `platform`.
        if let Some(path) = dev_restic_asset() {
            let triple = platform::host_target_triple()
                .expect("host_target_triple must be Some when dev_restic_asset is Some");
            let expected = format!(
                "third_party/restic/0.19.1/{triple}/{}",
                platform::restic_binary_name()
            );
            assert!(path.ends_with(&expected), "unexpected path: {path:?}");
            assert!(path.exists(), "asset should exist: {path:?}");
        }
    }
}
