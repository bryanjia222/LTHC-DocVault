//! Write commands (commit / export / checkout) backed by the job runner.
//!
//! Each command resolves its `DocumentRef`, hands an executor closure to
//! [`JobRegistry::spawn`], and returns the job id immediately. The executor
//! (see [`executors::execute_archive`] / [`executors::execute_export`] /
//! [`executors::execute_checkout`]) locks the shared vault, calls the
//! `DocVault` method, and maps any error to `String` so the runner stores it
//! verbatim. State changes flow to the UI via the `job:update` Tauri event
//! (see [`executors::make_emitter`]); the frontend never optimistically
//! updates.
//!
//! `target_label` is derived from the backend (not passed by the UI) so the
//! label is authoritative and a missing document fails fast before a job is
//! ever spawned.

pub mod commands;
/// The executor functions are `pub(crate)` so the add-in bridge can push uploads
/// through the same two-phase pipeline as the write commands.
pub(crate) mod executors;
pub use commands::*;

#[cfg(test)]
mod tests;
