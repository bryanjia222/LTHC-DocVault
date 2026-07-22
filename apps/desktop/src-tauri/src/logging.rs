//! Tracing subscriber initialization + runtime log-level control.
//!
//! The crates emit `tracing::info!` / `warn!` / `debug!` throughout, but
//! without an initialized subscriber those calls are discarded. This module
//! installs a `tracing-subscriber` registry that writes to a rolling daily file
//! under the app config dir (so logs survive restarts and are not lost with the
//! console). The filter is wrapped in a `reload::Layer` so [`set_level`] can
//! switch the active level live, without a restart. The level itself is
//! persisted in the vault's `config.toml` `[logging].level` and re-applied on
//! every vault open (see [`state::reload_log_level`]).

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::EnvFilter, prelude::*, reload};

/// Owns the live tracing subscriber: the reload handle flips the active filter
/// level at runtime, and the `WorkerGuard` must stay alive for the app's
/// lifetime so the non-blocking writer flushes its buffer on exit.
pub struct Logger {
    filter_handle: reload::Handle<EnvFilter, tracing_subscriber::registry::Registry>,
    _guard: WorkerGuard,
}

/// The fixed directory logs roll into: `<app_config_dir>/logs/`. `None` when
/// the platform config dir cannot be resolved (the subscriber then stays
/// uninitialized and logs are discarded, as before).
pub fn log_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    Some(dir.join("logs"))
}

/// Read the configured log level from a vault's `config.toml`. Falls back to
/// `info` when the file or `[logging]` section is absent or unparseable, so a
/// pre-logging config (or a corrupt one) never blocks startup.
pub fn read_level(config_path: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(config_path) else {
        return "info".to_owned();
    };
    match toml::from_str::<docvault_types::VaultConfig>(&text) {
        Ok(config) => config.logging.level,
        Err(_) => "info".to_owned(),
    }
}

/// Install the file-rolling subscriber. Uses `try_init` so a second call (or an
/// already-set global subscriber, e.g. in a test process) is a no-op rather than
/// a panic. Returns `None` when the app config dir is unavailable, the
/// subscriber is already installed, or the level reload handle could not be
/// captured - in all those cases logging stays off, matching prior behavior.
pub fn init(app: &AppHandle) -> Option<Logger> {
    let dir = log_dir(app)?;
    // Best-effort: a missing logs dir is created so the appender can write.
    // Failure is non-fatal - the appender would simply produce no output.
    let _ = std::fs::create_dir_all(&dir);

    let file_appender = tracing_appender::rolling::daily(&dir, "docvault.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let (filter_layer, filter_handle) = reload::Layer::new(EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .try_init()
        .ok()?;

    Some(Logger {
        filter_handle,
        _guard: guard,
    })
}

/// Switch the active log level. `level` is one of `error` / `warn` / `info` /
/// `debug` / `trace` (validated by the caller, so `EnvFilter::new` is
/// infallible here). Returns an error string when the reload itself fails.
pub fn set_level(logger: &Logger, level: &str) -> Result<(), String> {
    let filter = EnvFilter::new(level);
    logger
        .filter_handle
        .reload(filter)
        .map_err(|e| e.to_string())
}
