//! Tauri command entry points for write operations (commit / export /
//! checkout / delete / rename / note). Each resolves its `DocumentRef`, hands
//! an executor closure to [`JobRegistry::spawn`], and returns the job id
//! immediately; synchronous commands (`rename_document`, `set_version_note`,
//! `list_jobs`, `cancel_job`) run inline. The executor work itself lives in
//! [`super::executors`], extracted so the command tests exercise the same code
//! as production.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use docvault_jobs::{JobKind, JobOutcome, JobRecord};
use docvault_storage::DocumentRef;
use docvault_types::CommitMetadata;
use tauri::{AppHandle, State};

use super::executors::{
    execute_archive, execute_checkout, execute_delete, execute_delete_versions, execute_export,
    make_emitter, phase_a_commit, write_blank_source,
};
use crate::state::{self, AppState};

#[tauri::command(rename_all = "snake_case")]
pub fn commit_document(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    document_id: Option<String>,
    new_name: Option<String>,
    author: Option<String>,
    note: Option<String>,
) -> Result<String, String> {
    let (document_ref, target_label) = resolve_commit_ref(&state, document_id, new_name)?;
    let metadata = CommitMetadata { author, note };
    let source_path = PathBuf::from(&path);
    // Phase A (synchronous, fast): durable intake copy + `pending` DB row +
    // current-pointer flip + library materialize (served from the intake). No
    // compression happens here, so this resolves quickly and the UI can show
    // "commit succeeded" the moment it returns. Crash-safe: the intake is
    // fsynced before the DB row, so a crash here loses no data; a `pending`
    // version left by a crash is recovered on the next open.
    let (_document, version) = phase_a_commit(&state.vault, &source_path, document_ref, metadata)?;
    // Phase B (async Archive job): compress the pending version from its intake
    // and finalize the DB row. Tracked separately so the UI shows the long
    // compress step on its own; the job id is returned for the frontend to
    // surface progress + refresh the repo size when it finishes.
    let vault = state.vault.clone();
    let version_for_job = version.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Archive,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_archive(&vault, &version_for_job, cancel)
        },
    );
    Ok(job_id)
}

/// Create a brand-new blank document of the given format (`txt`/`md`/`docx`/
/// `xlsx`/`pptx`) and commit its first version through the same two-phase
/// pipeline as [`commit_document`]. Phase A (here, synchronous) writes a
/// durable intake copy of a freshly generated blank file + the `pending` DB
/// row + materializes the library copy; the Phase B CreateBlank job then
/// finalizes it (same compress work as Archive, labeled "creating" for the UI).
/// Returns the job id.
///
/// The blank source is generated, not supplied: txt/md are empty files, the
/// Office formats are minimal valid OOXML packages from
/// `docvault_ooxml::create_empty_package`. The new document's membership in a
/// project (if any) is the frontend's concern - it owns desktop-state and sets
/// it after the job reaches a terminal state, so `project_id` is not taken here.
#[tauri::command(rename_all = "snake_case")]
pub fn create_blank_document(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    format: String,
    aspect_ratio: Option<String>,
) -> Result<String, String> {
    let format = format.to_ascii_lowercase();
    let (source_path, _temp_dir) = write_blank_source(&format, aspect_ratio.as_deref())?;
    let document_ref = DocumentRef::NewName(name.clone());
    let metadata = CommitMetadata::default();
    let (_document, version) = phase_a_commit(&state.vault, &source_path, document_ref, metadata)?;
    let vault = state.vault.clone();
    let version_for_job = version.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::CreateBlank,
        name,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_archive(&vault, &version_for_job, cancel)
        },
    );
    Ok(job_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn export_version(
    app: AppHandle,
    state: State<'_, AppState>,
    document_id: String,
    version: String,
    output_path: String,
) -> Result<String, String> {
    let target_label = format!("{} {version}", lookup_document_name(&state, &document_id)?);
    let document_ref = DocumentRef::IdPrefix(document_id);
    let output = PathBuf::from(output_path);
    let vault = state.vault.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Export,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_export(&vault, &document_ref, &version, &output, cancel)
        },
    );
    Ok(job_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn checkout_version(
    app: AppHandle,
    state: State<'_, AppState>,
    document_id: String,
    version: String,
    output_path: Option<String>,
) -> Result<String, String> {
    let target_label = format!("{} {version}", lookup_document_name(&state, &document_id)?);
    let document_ref = DocumentRef::IdPrefix(document_id);
    let output = output_path.map(PathBuf::from);
    let vault = state.vault.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Checkout,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_checkout(&vault, &document_ref, &version, output.as_deref(), cancel)
        },
    );
    Ok(job_id)
}

/// Delete a document and all of its versions (restic snapshots are forgotten
/// + pruned for the restic backend). Runs as a job because forget/prune can be
/// slow. Returns the spawned job id; state arrives via `job:update`.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_document(
    app: AppHandle,
    state: State<'_, AppState>,
    document_id: String,
) -> Result<String, String> {
    let target_label = lookup_document_name(&state, &document_id)?;
    let document_ref = DocumentRef::IdPrefix(document_id);
    let vault = state.vault.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Delete,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_delete(&vault, &document_ref, cancel)
        },
    );
    Ok(job_id)
}

/// Delete specific versions of a document by id, keeping the document and its
/// other versions. Spawns a `Delete` job - restic forget/prune of the deleted
/// snapshots can be slow. Returns the spawned job id; state arrives via
/// `job:update`. The caller passes the version plus any descendants it
/// confirmed deleting (the crate deletes exactly those ids - it never reparents
/// or orphans survivors, and refuses the current version).
#[tauri::command(rename_all = "snake_case")]
pub fn delete_versions(
    app: AppHandle,
    state: State<'_, AppState>,
    document_id: String,
    version_ids: Vec<String>,
) -> Result<String, String> {
    let target_label = lookup_document_name(&state, &document_id)?;
    let document_ref = DocumentRef::IdPrefix(document_id);
    let vault = state.vault.clone();
    let on_event = make_emitter(app);
    let job_id = state.jobs.spawn(
        JobKind::Delete,
        target_label,
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_delete_versions(&vault, &document_ref, &version_ids, cancel)
        },
    );
    Ok(job_id)
}

/// Rename a document's display name. Synchronous (a single SQL UPDATE); not a
/// job. Does not touch the on-disk source file or any version's filename.
#[tauri::command(rename_all = "snake_case")]
pub fn rename_document(
    state: State<'_, AppState>,
    document_id: String,
    new_name: String,
) -> Result<(), String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault
        .as_ref()
        .ok_or_else(|| crate::logging::log_warn("vault not initialized"))?;
    vault
        .rename_document(&DocumentRef::IdPrefix(document_id), &new_name)
        .map_err(crate::logging::log_error)?;
    Ok(())
}

/// Update a version's note (its commit message). Synchronous (a single SQL
/// UPDATE); not a job. `None` (or an empty string from the UI) clears the note.
/// Does not touch the archive or any other version field.
#[tauri::command(rename_all = "snake_case")]
pub fn set_version_note(
    state: State<'_, AppState>,
    document_id: String,
    version_id: String,
    note: Option<String>,
) -> Result<(), String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault
        .as_ref()
        .ok_or_else(|| crate::logging::log_warn("vault not initialized"))?;
    vault
        .set_version_note(
            &DocumentRef::IdPrefix(document_id),
            &version_id,
            note.as_deref(),
        )
        .map_err(crate::logging::log_error)?;
    Ok(())
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Result<Vec<JobRecord>, String> {
    Ok(state.jobs.list())
}

/// Request cancellation of a running job. Returns whether a live job was found
/// to cancel; the job reaches `Cancelled` only if its work observes the flag
/// (a job that finishes first keeps its real `Succeeded`/`Failed` status).
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) -> Result<bool, String> {
    Ok(state.jobs.cancel(&job_id))
}

/// Resolve a commit target: an existing document by id (name looked up so the
/// UI gets an authoritative label and a missing doc fails fast), or a new
/// document by name. Exactly one must be supplied.
fn resolve_commit_ref(
    state: &State<AppState>,
    document_id: Option<String>,
    new_name: Option<String>,
) -> Result<(DocumentRef, String), String> {
    match (document_id, new_name) {
        (Some(id), _) => {
            let name = lookup_document_name(state, &id)?;
            Ok((DocumentRef::IdPrefix(id), name))
        }
        (None, Some(name)) => Ok((DocumentRef::NewName(name.clone()), name)),
        (None, None) => Err(crate::logging::log_warn(
            "either document_id or new_name is required",
        )),
    }
}

/// Look up a document's display name by id via a targeted query (not a full
/// document scan). Fails fast with a clear message if the document does not
/// exist, so the UI never spawns a job doomed to fail.
fn lookup_document_name(state: &State<AppState>, id: &str) -> Result<String, String> {
    let vault = state::lock_vault(&state.vault);
    let vault = vault
        .as_ref()
        .ok_or_else(|| crate::logging::log_warn("vault not initialized"))?;
    vault.document_name(id).map_err(crate::logging::log_error)
}
