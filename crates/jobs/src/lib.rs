//! Background job runner for DocVault.
//!
//! The registry owns the authoritative job state machine and runs each job on a
//! dedicated [`std::thread`] (storage ops are blocking). It is deliberately
//! Tauri-free: the desktop crate injects an `on_event` callback that forwards
//! state changes to the UI, so the runner stays reusable and unit-testable.
//!
//! Truthfulness contract: a job is `Running` from the moment it is spawned,
//! and transitions to exactly one terminal state (`Succeeded` or `Failed`)
//! with the backend's error string surfaced verbatim. The frontend mirrors
//! this state via events and never optimistically updates.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use uuid::Uuid;

/// Terminal-aware lifecycle. `Queued` is intentionally omitted: the runner
/// spawns immediately (no queue). `Cancelled` is reached only when a job's work
/// observes the cancel flag and aborts - a job that actually completes first
/// keeps its real `Succeeded`/`Failed` status, so the UI never lies about what
/// the backend did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// The terminal outcome a job's work reports. Splitting `Cancelled` from
/// `Failed` keeps cancellation truthful: the runner decides the recorded status
/// from this outcome (not from a late cancel flag), so a job that finished
/// before the cancel took effect is not mislabeled.
#[derive(Debug)]
pub enum JobOutcome {
    Succeeded,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobKind {
    Commit,
    Export,
    Checkout,
    Delete,
    /// Phase B of the async commit: compress (unzip + restic/local-copy) a
    /// `pending` version from its durable intake copy and finalize the DB row.
    /// Tracked separately from [`JobKind::Commit`] (which is now the synchronous
    /// Phase A) so the UI can show the long-running compress step on its own.
    Archive,
    /// Phase B of creating a blank document: identical compress work as
    /// [`Archive`], but tracked separately so the UI can label it "creating
    /// \<name\>" rather than "archiving" - from the user's view they just created
    /// a document, the compress step is incidental. Spawned by the desktop's
    /// `create_blank_document` command. Renamed `create_blank` (not `createblank`,
    /// which `rename_all = "lowercase"` alone would produce) so the wire token
    /// matches the i18n key `jobs.create_blank`.
    #[serde(rename = "create_blank")]
    CreateBlank,
}

/// Authoritative record for a single job. Serialized verbatim to the UI, which
/// formats dates/progress client-side (consistent with the read pipeline).
#[derive(Debug, Clone, Serialize)]
pub struct JobRecord {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    /// 0.0..=1.0 when the backend reports progress; `None` = indeterminate
    /// (the common case until restic `percent_done` streaming lands).
    pub progress: Option<f64>,
    pub error: Option<String>,
    pub target_label: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

pub type JobId = String;

/// Fired on every state change (spawn, progress, terminal). The desktop crate
/// implements this to emit a Tauri event; the runner knows nothing about Tauri.
pub type JobEventCallback = Arc<dyn Fn(JobRecord) + Send + Sync>;

/// Owns all job records. Cheap to clone (inner state is shared) so the desktop
/// can hand clones to command handlers while the registry lives in `AppState`.
#[derive(Clone, Default)]
pub struct JobRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

#[derive(Default)]
struct RegistryInner {
    records: HashMap<JobId, JobRecord>,
    /// Insertion order (oldest first), so [`JobRegistry::list`] can return
    /// newest-first deterministically without relying on timestamp granularity.
    order: Vec<JobId>,
    /// Live cancel flags for running jobs, dropped once the job is terminal.
    cancels: HashMap<JobId, Arc<AtomicBool>>,
}

/// Upper bound on retained job history. Terminal jobs beyond this are pruned
/// (oldest first); running jobs are never pruned.
const MAX_RECORDS: usize = 200;

impl JobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all jobs, newest first (by insertion order, not wall clock).
    pub fn list(&self) -> Vec<JobRecord> {
        let inner = self.inner.lock().expect("job registry poisoned");
        inner
            .order
            .iter()
            .rev()
            .filter_map(|id| inner.records.get(id).cloned())
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<JobRecord> {
        self.inner
            .lock()
            .expect("job registry poisoned")
            .records
            .get(id)
            .cloned()
    }

    /// Drop all job records. Called when switching to a different vault so the
    /// UI does not show the previous vault's jobs. Safe because the desktop's
    /// connect flow only switches when no job is `Running`.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().expect("job registry poisoned");
        inner.records.clear();
        inner.order.clear();
        inner.cancels.clear();
    }

    /// Request cancellation of a running job. Returns `false` if the job is
    /// unknown or already terminal (its cancel token has been dropped). The job
    /// reaches `Cancelled` only if its work observes the flag and aborts; a job
    /// that completes first keeps its real status.
    pub fn cancel(&self, id: &str) -> bool {
        let inner = self.inner.lock().expect("job registry poisoned");
        if let Some(token) = inner.cancels.get(id) {
            token.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Spawn `work` on a dedicated thread and return its id immediately. The
    /// record is already `Running` by the time this returns; `on_event` has
    /// already fired with the initial snapshot.
    ///
    /// `work` receives a progress reporter and a cancel flag it should poll
    /// during long operations (the restic layer checks it between polls). Its
    /// [`JobOutcome`] decides the terminal status: `Succeeded`/`Failed` carry
    /// the backend's real result, `Cancelled` only when the work observed the
    /// flag and aborted. A panic inside `work` is caught and mapped to `Failed`,
    /// so the job always reaches a terminal state.
    pub fn spawn<F>(
        &self,
        kind: JobKind,
        target_label: impl Into<String>,
        on_event: JobEventCallback,
        work: F,
    ) -> JobId
    where
        F: FnOnce(&dyn Fn(Option<f64>), &AtomicBool) -> JobOutcome + Send + 'static,
    {
        let id = Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let record = JobRecord {
            id: id.clone(),
            kind,
            status: JobStatus::Running,
            progress: None,
            error: None,
            target_label: target_label.into(),
            started_at: now_epoch(),
            finished_at: None,
        };
        {
            let mut inner = self.inner.lock().expect("job registry poisoned");
            inner.records.insert(id.clone(), record.clone());
            inner.order.push(id.clone());
            inner.cancels.insert(id.clone(), Arc::clone(&cancel));
            prune(&mut inner);
        }
        on_event(record);

        let inner = Arc::clone(&self.inner);
        let id_for_thread = id.clone();
        thread::spawn(move || {
            let progress = |p: Option<f64>| {
                let snapshot = {
                    let mut inner = inner.lock().expect("job registry poisoned");
                    let Some(rec) = inner.records.get_mut(&id_for_thread) else {
                        return;
                    };
                    rec.progress = p;
                    rec.clone()
                };
                on_event(snapshot);
            };
            // catch_unwind so a panic inside `work` still reaches a terminal
            // state and emits the event - otherwise the thread dies silently
            // and the job is stuck `Running` forever (truthfulness contract).
            let outcome = match catch_unwind(AssertUnwindSafe(|| work(&progress, &cancel))) {
                Ok(outcome) => outcome,
                Err(payload) => JobOutcome::Failed(panic_message(payload)),
            };
            let terminal = {
                let mut inner = inner.lock().expect("job registry poisoned");
                let terminal = {
                    let Some(rec) = inner.records.get_mut(&id_for_thread) else {
                        return;
                    };
                    rec.finished_at = Some(now_epoch());
                    match outcome {
                        JobOutcome::Succeeded => rec.status = JobStatus::Succeeded,
                        JobOutcome::Failed(err) => {
                            rec.status = JobStatus::Failed;
                            rec.error = Some(err);
                        }
                        JobOutcome::Cancelled => rec.status = JobStatus::Cancelled,
                    }
                    rec.clone()
                };
                // The job is terminal: drop its cancel token (a late cancel is
                // a no-op) and trim history to the bound.
                inner.cancels.remove(&id_for_thread);
                prune(&mut inner);
                terminal
            };
            on_event(terminal);
        });

        id
    }
}

/// Evict the oldest terminal jobs until `records` is within `MAX_RECORDS`.
/// Running jobs are never pruned; if every record is running, pruning stops
/// early and resumes when those jobs finish.
fn prune(inner: &mut RegistryInner) {
    while inner.records.len() > MAX_RECORDS {
        let Some(evict) = inner
            .order
            .iter()
            .find(|id| {
                inner
                    .records
                    .get(*id)
                    .is_some_and(|rec| rec.status != JobStatus::Running)
            })
            .cloned()
        else {
            break;
        };
        inner.records.remove(&evict);
        inner.cancels.remove(&evict);
        inner.order.retain(|id| *id != evict);
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Extract a readable message from a panic payload (`&'static str` or `String`
/// from `panic!`), falling back to a generic label so the `Failed` record
/// always carries some explanation.
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "job panicked".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn records_succeeded_and_failed_terminal_states() {
        let registry = JobRegistry::new();
        let terminal = Arc::new(AtomicUsize::new(0));

        let ok_id = registry.spawn(
            JobKind::Commit,
            "report",
            make_counter(&terminal),
            |_progress, _cancel| JobOutcome::Succeeded,
        );
        let err_id = registry.spawn(
            JobKind::Export,
            "report v1",
            make_counter(&terminal),
            |_progress, _cancel| JobOutcome::Failed("boom".to_owned()),
        );

        // Wait for both jobs to reach a terminal state (robust vs scheduling).
        let mut waited = 0;
        while terminal.load(Ordering::SeqCst) < 2 && waited < 500 {
            thread::sleep(Duration::from_millis(2));
            waited += 1;
        }
        assert_eq!(
            terminal.load(Ordering::SeqCst),
            2,
            "both jobs should finish"
        );

        let ok = registry.get(&ok_id).expect("ok job recorded");
        let err = registry.get(&err_id).expect("err job recorded");
        assert_eq!(ok.status, JobStatus::Succeeded);
        assert!(ok.error.is_none());
        assert!(ok.finished_at.is_some());
        assert_eq!(err.status, JobStatus::Failed);
        assert_eq!(err.error.as_deref(), Some("boom"));
        assert!(err.finished_at.is_some());
    }

    #[test]
    fn list_returns_newest_first() {
        let registry = JobRegistry::new();
        let a = registry.spawn(JobKind::Commit, "a", make_noop(), |_, _| {
            JobOutcome::Succeeded
        });
        let b = registry.spawn(JobKind::Commit, "b", make_noop(), |_, _| {
            JobOutcome::Succeeded
        });

        let records = registry.list();
        assert_eq!(records.first().map(|r| r.id.as_str()), Some(b.as_str()));
        assert_eq!(records.last().map(|r| r.id.as_str()), Some(a.as_str()));
        assert_eq!(records.len(), 2);
    }

    /// A panic inside `work` must not leave the job stuck `Running`: the runner
    /// catches it, marks the job `Failed` with the panic message, and still
    /// emits the terminal event (truthfulness contract).
    #[test]
    fn panic_in_work_is_marked_failed_and_emits_terminal() {
        let registry = JobRegistry::new();
        let terminal = Arc::new(AtomicUsize::new(0));

        let id = registry.spawn(
            JobKind::Commit,
            "boom",
            make_counter(&terminal),
            |_progress, _cancel| panic!("boom"),
        );

        let mut waited = 0;
        while terminal.load(Ordering::SeqCst) == 0 && waited < 500 {
            thread::sleep(Duration::from_millis(2));
            waited += 1;
        }
        assert_eq!(
            terminal.load(Ordering::SeqCst),
            1,
            "a panicking job must still emit its terminal event"
        );

        let record = registry.get(&id).expect("job recorded");
        assert_eq!(record.status, JobStatus::Failed);
        assert!(
            record.error.as_deref().is_some_and(|e| e.contains("boom")),
            "unexpected error: {:?}",
            record.error
        );
        assert!(record.finished_at.is_some());
    }

    /// A job that polls the cancel flag and reports `Cancelled` reaches
    /// `Cancelled` (not `Failed`), with no error.
    #[test]
    fn cancelled_job_reports_cancelled() {
        let registry = JobRegistry::new();
        let terminal = Arc::new(AtomicUsize::new(0));
        let id = registry.spawn(
            JobKind::Commit,
            "report",
            make_counter(&terminal),
            |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| {
                while !cancel.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(2));
                }
                JobOutcome::Cancelled
            },
        );

        thread::sleep(Duration::from_millis(20));
        assert!(registry.cancel(&id));

        let mut waited = 0;
        while terminal.load(Ordering::SeqCst) == 0 && waited < 500 {
            thread::sleep(Duration::from_millis(2));
            waited += 1;
        }
        assert_eq!(terminal.load(Ordering::SeqCst), 1);

        let record = registry.get(&id).expect("job recorded");
        assert_eq!(record.status, JobStatus::Cancelled);
        assert!(record.error.is_none());
        assert!(record.finished_at.is_some());
    }

    /// A job that completes before the cancel takes effect keeps its real
    /// `Succeeded` status; a late cancel is a no-op (the token was dropped).
    #[test]
    fn completed_job_stays_succeeded_when_cancel_arrives_late() {
        let registry = JobRegistry::new();
        let terminal = Arc::new(AtomicUsize::new(0));
        let id = registry.spawn(
            JobKind::Commit,
            "report",
            make_counter(&terminal),
            |_, _| JobOutcome::Succeeded,
        );

        let mut waited = 0;
        while terminal.load(Ordering::SeqCst) == 0 && waited < 500 {
            thread::sleep(Duration::from_millis(2));
            waited += 1;
        }

        let cancelled = registry.cancel(&id);
        assert!(!cancelled, "late cancel should find no live token");

        let record = registry.get(&id).expect("job recorded");
        assert_eq!(record.status, JobStatus::Succeeded);
    }

    /// History is bounded: terminal jobs beyond `MAX_RECORDS` are pruned
    /// (oldest first), running jobs are retained.
    #[test]
    fn history_is_pruned_beyond_max() {
        let registry = JobRegistry::new();
        for _ in 0..(MAX_RECORDS + 5) {
            registry.spawn(JobKind::Commit, "x", make_noop(), |_, _| {
                JobOutcome::Succeeded
            });
        }

        // Wait for every job to reach a terminal state so terminal pruning has
        // run; then the list must be capped at MAX_RECORDS.
        let mut waited = 0;
        loop {
            let any_running = registry
                .list()
                .iter()
                .any(|rec| rec.status == JobStatus::Running);
            if !any_running || waited > 1000 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
            waited += 1;
        }

        let records = registry.list();
        assert!(
            records.len() <= MAX_RECORDS,
            "history should be capped, got {}",
            records.len()
        );
        assert_eq!(records.len(), MAX_RECORDS);
    }

    fn make_counter(terminal: &Arc<AtomicUsize>) -> JobEventCallback {
        let terminal = Arc::clone(terminal);
        Arc::new(move |record: JobRecord| {
            if record.status != JobStatus::Running {
                terminal.fetch_add(1, Ordering::SeqCst);
            }
        })
    }

    fn make_noop() -> JobEventCallback {
        Arc::new(|_record: JobRecord| {})
    }
}
