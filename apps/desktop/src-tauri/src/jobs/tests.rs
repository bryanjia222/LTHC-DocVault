use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use docvault_core::DocVault;
use docvault_jobs::{JobEventCallback, JobKind, JobOutcome, JobRecord, JobRegistry, JobStatus};
use docvault_storage::{DocumentRef, VaultPaths, VaultStorage};
use docvault_types::CommitMetadata;

use super::executors::{execute_archive, phase_a_commit, write_blank_source};

/// End-to-end proof of the async-commit contract: Phase A (synchronous)
/// writes the `pending` version + materializes the library copy, then the
/// Phase B Archive job reaches `Succeeded` and finalizes the version (no
/// longer `pending`). The document/version are visible throughout because
/// Phase A flips the current pointer before any archiving.
#[test]
fn commit_job_succeeds_and_version_appears() {
    let temp = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp.path());
    write_local_copy_config(&paths);
    let storage = VaultStorage::init(paths).unwrap();
    let vault: Arc<Mutex<Option<DocVault>>> = Arc::new(Mutex::new(Some(DocVault::new(storage))));

    let docx = write_source(temp.path(), "report.docx", b"version one");
    let path = docx.to_string_lossy().to_string();

    let registry = JobRegistry::new();
    let (job_id, terminal, doc_id) = commit_and_spawn_archive(
        &registry,
        Arc::clone(&vault),
        path,
        DocumentRef::NewName("report".to_owned()),
    );

    // Phase A already ran (synchronously) before the job was spawned: the
    // document + version are visible and the library copy is materialized.
    {
        let vault = vault.lock().unwrap();
        let vault = vault.as_ref().unwrap();
        let documents = vault.list_documents().unwrap();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].name, "report");
        let versions = vault
            .list_versions(&DocumentRef::IdPrefix(documents[0].id.as_str().to_owned()))
            .unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(
            versions[0].archive_status, "pending",
            "version is pending until Phase B"
        );
        let lib_path = crate::library::library_path_for_doc(vault, documents[0].id.as_str())
            .expect("library path resolves");
        assert!(lib_path.exists(), "library copy materialized by Phase A");
    }

    wait_for_terminal(&terminal);
    let record = registry.get(&job_id).expect("job recorded");
    assert_eq!(record.status, JobStatus::Succeeded);
    assert!(
        record.error.is_none(),
        "unexpected error: {:?}",
        record.error
    );
    assert!(record.finished_at.is_some());

    // Phase B finalized the version: no longer pending.
    let vault = vault.lock().unwrap();
    let vault = vault.as_ref().unwrap();
    let versions = vault.list_versions(&DocumentRef::IdPrefix(doc_id)).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(
        versions[0].archive_status, "archived",
        "Phase B flipped the version to archived"
    );
}

/// Commit-modified: when the committed source IS the library copy itself,
/// Phase A skips materialize (source == library path) and just writes the
/// pending version. Verifies the library-model fast path for the normal
/// edit-save-commit loop.
#[test]
fn commit_modified_commits_library_copy_without_rematerialize() {
    let temp = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp.path());
    write_local_copy_config(&paths);
    let storage = VaultStorage::init(paths).unwrap();
    let vault: Arc<Mutex<Option<DocVault>>> = Arc::new(Mutex::new(Some(DocVault::new(storage))));

    let docx = write_source(temp.path(), "report.docx", b"version one");
    let registry = JobRegistry::new();
    let (_job_id, terminal, doc_id) = commit_and_spawn_archive(
        &registry,
        Arc::clone(&vault),
        docx.to_string_lossy().to_string(),
        DocumentRef::NewName("report".to_owned()),
    );
    wait_for_terminal(&terminal);

    let lib_path = {
        let vault = vault.lock().unwrap();
        let vault = vault.as_ref().unwrap();
        crate::library::library_path_for_doc(vault, &doc_id).unwrap()
    };
    assert!(lib_path.exists(), "library copy exists after add");

    // commit-modified: the source IS the library copy -> Phase A skips
    // materialize and writes a second pending version.
    let (_job_id2, terminal2, _) = commit_and_spawn_archive(
        &registry,
        Arc::clone(&vault),
        lib_path.to_string_lossy().to_string(),
        DocumentRef::IdPrefix(doc_id.clone()),
    );
    wait_for_terminal(&terminal2);
    let record2 = registry.get(&_job_id2).expect("second job recorded");
    assert_eq!(
        record2.status,
        JobStatus::Succeeded,
        "commit-modified should succeed"
    );
    assert!(
        record2.error.is_none(),
        "unexpected error: {:?}",
        record2.error
    );

    let vault = vault.lock().unwrap();
    let vault = vault.as_ref().unwrap();
    let versions = vault.list_versions(&DocumentRef::IdPrefix(doc_id)).unwrap();
    assert_eq!(versions.len(), 2, "commit-modified added a second version");
    assert!(
        versions.iter().all(|v| v.archive_status == "archived"),
        "both versions archived after their Phase B jobs"
    );
}

/// A non-Office file (txt) commits through the async path just like an
/// Office document: Phase A writes the pending version + materializes the
/// library copy, then the Phase B Archive job reaches `Succeeded` and
/// finalizes it. Document management works for every managed type, not just
/// Office - the content-aware archive stores raw binaries verbatim.
#[test]
fn commit_job_succeeds_for_raw_binary_file() {
    let temp = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp.path());
    write_local_copy_config(&paths);
    let storage = VaultStorage::init(paths).unwrap();
    let vault: Arc<Mutex<Option<DocVault>>> = Arc::new(Mutex::new(Some(DocVault::new(storage))));

    let txt = temp.path().join("notes.txt");
    fs::write(&txt, b"plain text, not Office").unwrap();

    let registry = JobRegistry::new();
    let (job_id, terminal, doc_id) = commit_and_spawn_archive(
        &registry,
        Arc::clone(&vault),
        txt.to_string_lossy().to_string(),
        DocumentRef::NewName("notes".to_owned()),
    );

    wait_for_terminal(&terminal);
    let record = registry.get(&job_id).expect("job recorded");
    assert_eq!(
        record.status,
        JobStatus::Succeeded,
        "raw-binary commit archives"
    );
    assert!(
        record.error.is_none(),
        "unexpected error: {:?}",
        record.error
    );

    let vault = vault.lock().unwrap();
    let vault = vault.as_ref().unwrap();
    let versions = vault.list_versions(&DocumentRef::IdPrefix(doc_id)).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].archive_status, "archived");
    assert_eq!(versions[0].original_filename, "notes.txt");
    // Raw binary -> single-entry whole-file manifest.
    assert_eq!(versions[0].manifest.entries.len(), 1);
    assert_eq!(versions[0].manifest.entries[0].path, "notes.txt");
}

/// `write_blank_source` produces a valid blank file for every supported
/// format: an empty file for txt/md, a minimal valid OOXML package for
/// docx/xlsx/pptx (recognized by content, not extension).
#[test]
fn write_blank_source_produces_valid_files() {
    for format in ["txt", "md", "docx", "xlsx", "pptx"] {
        let (path, _temp) = write_blank_source(format, None).expect("blank source written");
        assert!(path.exists(), "{format} source exists");
        match format {
            "txt" | "md" => {
                assert_eq!(path.metadata().unwrap().len(), 0, "{format} is empty");
            }
            "docx" | "xlsx" | "pptx" => {
                assert!(
                    docvault_ooxml::is_ooxml_package(&path),
                    "{format} is a valid OOXML package"
                );
            }
            _ => unreachable!(),
        }
    }
    // Unknown format is rejected.
    assert!(
        write_blank_source("pdf", None).is_err(),
        "unsupported format rejected"
    );
}

/// A blank docx (from `write_blank_source`) flows through the full Phase A +
/// Phase B pipeline: the document + version appear, the library copy is
/// materialized, and the Archive job finalizes the version. Mirrors the
/// `create_blank_document` command path (Phase A + an Archive job).
#[test]
fn blank_document_archives_through_commit_pipeline() {
    let temp = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp.path());
    write_local_copy_config(&paths);
    let storage = VaultStorage::init(paths).unwrap();
    let vault: Arc<Mutex<Option<DocVault>>> = Arc::new(Mutex::new(Some(DocVault::new(storage))));

    let (source, _blank_temp) = write_blank_source("docx", None).unwrap();
    let path = source.to_string_lossy().to_string();

    let registry = JobRegistry::new();
    let (job_id, terminal, doc_id) = commit_and_spawn_archive(
        &registry,
        Arc::clone(&vault),
        path,
        DocumentRef::NewName("blank doc".to_owned()),
    );
    wait_for_terminal(&terminal);

    let record = registry.get(&job_id).expect("job recorded");
    assert_eq!(record.status, JobStatus::Succeeded);
    assert!(
        record.error.is_none(),
        "unexpected error: {:?}",
        record.error
    );

    let vault = vault.lock().unwrap();
    let vault = vault.as_ref().unwrap();
    let versions = vault
        .list_versions(&DocumentRef::IdPrefix(doc_id.clone()))
        .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].archive_status, "archived");
    // The blank source is `blank.docx`, so the version's original filename
    // carries the .docx extension (drives type/extension derivation).
    assert_eq!(versions[0].original_filename, "blank.docx");
    // OOXML -> multi-entry package manifest (not a single raw entry).
    assert!(
        versions[0].manifest.entries.len() > 1,
        "blank docx archived as an OOXML package"
    );
    // Phase A materialized the library copy.
    let lib_path = crate::library::library_path_for_doc(vault, &doc_id).unwrap();
    assert!(lib_path.exists(), "library copy materialized");
}

/// Run Phase A synchronously, then spawn the Phase B Archive job. Returns
/// the archive job id, a terminal counter, and the committed document id.
fn commit_and_spawn_archive(
    registry: &JobRegistry,
    vault: Arc<Mutex<Option<DocVault>>>,
    path: String,
    document_ref: DocumentRef,
) -> (String, Arc<AtomicUsize>, String) {
    let (document, version) = phase_a_commit(
        &vault,
        PathBuf::from(&path),
        document_ref,
        CommitMetadata::default(),
    )
    .expect("Phase A commit succeeds");
    let doc_id = document.id.as_str().to_owned();
    let terminal = Arc::new(AtomicUsize::new(0));
    let on_event = {
        let terminal = Arc::clone(&terminal);
        Arc::new(move |record: JobRecord| {
            if record.status != JobStatus::Running {
                terminal.fetch_add(1, Ordering::SeqCst);
            }
        }) as JobEventCallback
    };
    let version_for_job = version.clone();
    let job_id = registry.spawn(
        JobKind::Archive,
        "report",
        on_event,
        move |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            execute_archive(&vault, &version_for_job, cancel)
        },
    );
    (job_id, terminal, doc_id)
}

/// A job whose work observes the cancel flag and reports `Cancelled` must
/// reach `Cancelled` (not `Failed`), with no error, and emit its terminal
/// event. Mirrors how a stalled restic call surfaces cancellation.
#[test]
fn cancel_request_marks_running_job_cancelled() {
    let registry = JobRegistry::new();
    let terminal = Arc::new(AtomicUsize::new(0));
    let on_event = {
        let terminal = Arc::clone(&terminal);
        Arc::new(move |record: JobRecord| {
            if record.status != JobStatus::Running {
                terminal.fetch_add(1, Ordering::SeqCst);
            }
        }) as JobEventCallback
    };
    let job_id = registry.spawn(
        JobKind::Export,
        "report v1",
        on_event,
        |_: &dyn Fn(Option<f64>), cancel: &AtomicBool| -> JobOutcome {
            while !cancel.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(2));
            }
            JobOutcome::Cancelled
        },
    );

    thread::sleep(Duration::from_millis(20));
    assert!(registry.cancel(&job_id), "cancel should find a live job");

    let mut waited = 0;
    while terminal.load(Ordering::SeqCst) == 0 && waited < 1000 {
        thread::sleep(Duration::from_millis(2));
        waited += 1;
    }
    assert_eq!(
        terminal.load(Ordering::SeqCst),
        1,
        "cancelled job must emit its terminal event"
    );

    let record = registry.get(&job_id).expect("job recorded");
    assert_eq!(record.status, JobStatus::Cancelled);
    assert!(record.error.is_none(), "cancelled job carries no error");
    assert!(record.finished_at.is_some());
}

fn wait_for_terminal(terminal: &AtomicUsize) {
    let mut waited = 0;
    while terminal.load(Ordering::SeqCst) == 0 && waited < 1000 {
        thread::sleep(Duration::from_millis(2));
        waited += 1;
    }
    assert_eq!(terminal.load(Ordering::SeqCst), 1, "job did not finish");
}

fn temp_paths(root: &Path) -> VaultPaths {
    VaultPaths::new(root, root.join("data"), root.join("db.sqlite"))
}

fn write_local_copy_config(paths: &VaultPaths) {
    fs::create_dir_all(&paths.root_dir).unwrap();
    fs::write(
        &paths.config_path,
        format!(
            "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
            config_path(&paths.data_dir),
            config_path(&paths.repo_dir),
            config_path(&paths.db_path)
        ),
    )
    .unwrap();
}

fn write_source(root: &Path, name: &str, contents: &[u8]) -> PathBuf {
    let source_dir = root.join("sources");
    let package_dir = root.join("package-source").join(name);
    fs::create_dir_all(package_dir.join("word")).unwrap();
    fs::write(package_dir.join("[Content_Types].xml"), b"types").unwrap();
    fs::write(package_dir.join("word").join("document.xml"), contents).unwrap();
    fs::create_dir_all(&source_dir).unwrap();
    let source = source_dir.join(name);
    docvault_ooxml::pack_package(package_dir, &source).unwrap();
    source
}

fn config_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}
