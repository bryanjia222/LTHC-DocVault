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
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use uuid::Uuid;

/// Terminal-aware lifecycle. `Queued`/`Cancelled` are intentionally omitted for
/// now: the runner spawns immediately (no queue) and cannot interrupt a
/// blocking storage call, so exposing those states would lie to the UI. They
/// are deferred follow-ups (see plan decisions E/F).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobKind {
    Commit,
    Export,
    Checkout,
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
}

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

    /// Spawn `work` on a dedicated thread and return its id immediately. The
    /// record is already `Running` by the time this returns; `on_event` has
    /// already fired with the initial snapshot.
    ///
    /// `work` receives a progress reporter it may call between phases (ignored
    /// by fast local-copy ops; used by future restic streaming). Errors are
    /// mapped to `Failed` with `e.to_string()` so the backend's readable
    /// `Display` reaches the UI unchanged.
    pub fn spawn<F>(
        &self,
        kind: JobKind,
        target_label: impl Into<String>,
        on_event: JobEventCallback,
        work: F,
    ) -> JobId
    where
        F: FnOnce(&dyn Fn(Option<f64>)) -> Result<(), String> + Send + 'static,
    {
        let id = Uuid::new_v4().to_string();
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
            let result = work(&progress);
            let terminal = {
                let mut inner = inner.lock().expect("job registry poisoned");
                let Some(rec) = inner.records.get_mut(&id_for_thread) else {
                    return;
                };
                rec.finished_at = Some(now_epoch());
                match result {
                    Ok(()) => rec.status = JobStatus::Succeeded,
                    Err(err) => {
                        rec.status = JobStatus::Failed;
                        rec.error = Some(err);
                    }
                }
                rec.clone()
            };
            on_event(terminal);
        });

        id
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn records_succeeded_and_failed_terminal_states() {
        let registry = JobRegistry::new();
        let terminal = Arc::new(AtomicUsize::new(0));

        let ok_id = registry.spawn(
            JobKind::Commit,
            "report",
            make_counter(&terminal),
            |_progress| Ok(()),
        );
        let err_id = registry.spawn(
            JobKind::Export,
            "report v1",
            make_counter(&terminal),
            |_progress| Err("boom".to_owned()),
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
        let a = registry.spawn(JobKind::Commit, "a", make_noop(), |_| Ok(()));
        let b = registry.spawn(JobKind::Commit, "b", make_noop(), |_| Ok(()));

        let records = registry.list();
        assert_eq!(records.first().map(|r| r.id.as_str()), Some(b.as_str()));
        assert_eq!(records.last().map(|r| r.id.as_str()), Some(a.as_str()));
        assert_eq!(records.len(), 2);
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
