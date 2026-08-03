//! Job-phase executors: the real work behind the write commands, extracted so
//! the command tests exercise the same code as production. Each `execute_*`
//! locks the shared vault, calls the `DocVault` method, and maps any error to
//! a [`JobOutcome`] the runner stores verbatim.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use docvault_core::CoreError;
use docvault_jobs::{JobEventCallback, JobOutcome, JobRecord};
use docvault_storage::{DocumentRef, ResticError, StorageError};
use docvault_types::CommitMetadata;
use tauri::{AppHandle, Emitter};
use tracing::warn;

/// Phase A of the async commit, run synchronously in the command (not a job):
/// durable intake copy + `pending` DB row + current-pointer flip, then
/// materialize the library copy from the just-committed (pending) version -
/// `export_version` serves a pending version from its intake, so the working
/// copy is ready before the archive finishes. No compression happens here, so
/// this is fast. Extracted so tests exercise the same code as the command.
pub(crate) fn phase_a_commit(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    path: impl AsRef<Path>,
    document_ref: DocumentRef,
    metadata: CommitMetadata,
) -> Result<(docvault_types::Document, docvault_types::Version), String> {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return Err(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return Err("vault not initialized".to_owned());
    };
    let source_path = path.as_ref();
    let (document, version) = match vault.begin_commit(source_path, document_ref, metadata) {
        Ok(result) => result,
        Err(e) => return Err(e.to_string()),
    };
    // Library model: when the committed source was an external file (add /
    // manual commit), materialize a library copy from the now-current version so
    // the tool owns a working copy. When the source IS the library copy
    // (commit-modified), skip - it already equals the just-committed version.
    let doc_id = document.id.as_str();
    match crate::library::library_path_for_doc(vault, doc_id) {
        Ok(lib_path) if source_path != lib_path => {
            if let Err(e) = vault.export_version(
                &DocumentRef::IdPrefix(doc_id.to_owned()),
                "current",
                &lib_path,
                &docvault_storage::NEVER_CANCELLED,
            ) {
                return Err(format!(
                    "committed but failed to materialize library copy: {e}"
                ));
            }
        }
        _ => {} // source is the library copy, or path unknown - nothing to materialize
    }
    Ok((document, version))
}

/// Phase B executor: archive a `pending` version from its durable intake copy,
/// finalize the DB row, and reclaim the intake. Idempotent, so re-running after
/// a crash (or the recovery on open) never duplicates work.
pub(crate) fn execute_archive(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    version: &docvault_types::Version,
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    match vault.archive_pending_version(version, cancel) {
        Ok(()) => JobOutcome::Succeeded,
        Err(CoreError::Storage(StorageError::Restic(ResticError::Cancelled))) => {
            JobOutcome::Cancelled
        }
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

pub(crate) fn execute_export(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    version: &str,
    output: &Path,
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    match vault.export_version(document_ref, version, output, cancel) {
        Ok(_) => JobOutcome::Succeeded,
        Err(StorageError::Restic(ResticError::Cancelled)) => JobOutcome::Cancelled,
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

pub(crate) fn execute_checkout(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    version: &str,
    output: Option<&Path>,
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    match vault.checkout_version(document_ref, version, output, cancel) {
        Ok(_) => JobOutcome::Succeeded,
        Err(StorageError::Restic(ResticError::Cancelled)) => JobOutcome::Cancelled,
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

pub(crate) fn execute_delete(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    match vault.delete_document(document_ref, cancel) {
        Ok(_) => JobOutcome::Succeeded,
        Err(CoreError::Storage(StorageError::Restic(ResticError::Cancelled))) => {
            JobOutcome::Cancelled
        }
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

pub(crate) fn execute_delete_versions(
    vault: &Arc<std::sync::Mutex<Option<docvault_core::DocVault>>>,
    document_ref: &DocumentRef,
    version_ids: &[String],
    cancel: &AtomicBool,
) -> JobOutcome {
    let vault = match vault.lock() {
        Ok(guard) => guard,
        Err(e) => return JobOutcome::Failed(e.to_string()),
    };
    let Some(vault) = vault.as_ref() else {
        return JobOutcome::Failed("vault not initialized".to_owned());
    };
    match vault.delete_versions(document_ref, version_ids, cancel) {
        Ok(_) => JobOutcome::Succeeded,
        Err(CoreError::Storage(StorageError::Restic(ResticError::Cancelled))) => {
            JobOutcome::Cancelled
        }
        Err(e) => JobOutcome::Failed(e.to_string()),
    }
}

/// Write a blank source file for `format` to a fresh temp dir and return its
/// path together with the dir handle. txt/md get an empty file; docx/xlsx/pptx
/// get a minimal valid OOXML package from `docvault_ooxml::create_empty_package`.
/// `aspect_ratio` is forwarded to the OOXML layer and only affects pptx (16:9 vs
/// the 4:3 default); ignored for the other formats. The caller must hold the
/// returned `TempDir` until after [`phase_a_commit`], which durably copies the
/// source into the vault's intake (so the temp file is unneeded once that
/// returns).
pub(crate) fn write_blank_source(
    format: &str,
    aspect_ratio: Option<&str>,
) -> Result<(PathBuf, tempfile::TempDir), String> {
    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source_path = temp_dir.path().join(format!("blank.{format}"));
    match format {
        "txt" | "md" => std::fs::write(&source_path, b"").map_err(|e| e.to_string())?,
        "docx" | "xlsx" | "pptx" => {
            docvault_ooxml::create_empty_package(format, aspect_ratio, &source_path)
                .map_err(|e| e.to_string())?
        }
        other => return Err(format!("unsupported document format: {other}")),
    }
    Ok((source_path, temp_dir))
}

/// Build the `on_event` callback that forwards each job snapshot to the UI as a
/// `job:update` event. `AppHandle` is `Send + Sync + Clone`, so the callback is
/// safe to invoke from job threads.
pub(crate) fn make_emitter(app: AppHandle) -> JobEventCallback {
    Arc::new(move |record: JobRecord| {
        if let Err(e) = app.emit("job:update", record) {
            warn!(error = %e, "failed to emit job:update event");
        }
    })
}
