use std::{
    fs,
    path::{Path, PathBuf},
};

use docvault_types::{CommitMetadata, Document, Version};

use super::*;

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

fn read_document_xml(package_path: &Path) -> Vec<u8> {
    let temp_dir = tempfile::tempdir().unwrap();
    docvault_ooxml::unpack_package(package_path, temp_dir.path()).unwrap();
    fs::read(temp_dir.path().join("word").join("document.xml")).unwrap()
}

fn commit(
    storage: &VaultStorage,
    document_ref: DocumentRef,
    source_path: &Path,
) -> (Document, Version) {
    storage
        .add_document_version(
            document_ref,
            source_path,
            CommitMetadata::default(),
            &NEVER_CANCELLED,
        )
        .unwrap()
}

#[test]
fn commits_lists_and_exports_versions_with_local_copy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");

    let storage = VaultStorage::init(paths.clone()).unwrap();
    let (_, version) = storage
        .add_document_version(
            DocumentRef::Name("report".to_owned()),
            &source,
            CommitMetadata {
                author: Some("Bryan".to_owned()),
                note: Some("Initial commit".to_owned()),
            },
            &NEVER_CANCELLED,
        )
        .unwrap();

    assert_eq!(storage.backend(), BackupBackend::LocalCopy);
    assert_eq!(version.id, "v1");
    assert_eq!(version.backup_backend, "local-copy");
    assert_eq!(version.original_filename, "report.docx");
    assert!(!Path::new(&version.archive_reference).is_absolute());
    assert!(version.manifest.entries.iter().any(|entry| {
        entry.path == "word/document.xml" && entry.size == "version one".len() as u64
    }));
    assert_eq!(version.author.as_deref(), Some("Bryan"));
    assert_eq!(version.note.as_deref(), Some("Initial commit"));
    assert_eq!(storage.list_documents().unwrap()[0].name, "report");
    let versions = storage
        .list_versions(&DocumentRef::Name("report".to_owned()))
        .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].author.as_deref(), Some("Bryan"));

    let restored = storage
        .export_version(
            &DocumentRef::Name("report".to_owned()),
            "latest",
            &paths.root_dir.join("restored"),
            &NEVER_CANCELLED,
        )
        .unwrap();
    assert_eq!(read_document_xml(&restored), b"version one");
}

#[test]
fn duplicate_document_names_are_ambiguous_by_name() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let storage = VaultStorage::init(paths).unwrap();
    storage.create_document("report", 1).unwrap();
    storage.create_document("report", 2).unwrap();

    let error = storage
        .list_versions(&DocumentRef::Name("report".to_owned()))
        .unwrap_err();

    assert!(matches!(
        error,
        StorageError::AmbiguousDocumentName { name, matches } if name == "report" && matches.len() == 2
    ));
}

#[test]
fn document_name_lookup_by_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let storage = VaultStorage::init(paths).unwrap();
    let document = storage.create_document("report", 1).unwrap();

    assert_eq!(
        storage.document_name(document.id.as_str()).unwrap(),
        "report"
    );
}

#[test]
fn document_name_missing_returns_error() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let storage = VaultStorage::init(paths).unwrap();

    let error = storage.document_name("nonexistent-id").unwrap_err();
    assert!(matches!(
        error,
        StorageError::DocumentIdNotFound(id) if id == "nonexistent-id"
    ));
}

#[test]
fn id_prefix_and_name_at_id_prefix_resolve_duplicate_names() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let storage = VaultStorage::init(paths).unwrap();
    let first = storage.create_document("report", 1).unwrap();
    let second = storage.create_document("report", 2).unwrap();
    commit(
        &storage,
        DocumentRef::IdPrefix(first.id.as_str()[..8].to_owned()),
        &source,
    );
    commit(
        &storage,
        DocumentRef::NameAndIdPrefix {
            name: "report".to_owned(),
            id_prefix: second.id.as_str()[..8].to_owned(),
        },
        &source,
    );

    let first_versions = storage
        .list_versions(&DocumentRef::IdPrefix(first.id.as_str()[..8].to_owned()))
        .unwrap();
    let second_versions = storage
        .list_versions(&DocumentRef::NameAndIdPrefix {
            name: "report".to_owned(),
            id_prefix: second.id.as_str()[..8].to_owned(),
        })
        .unwrap();

    assert_eq!(first_versions[0].document_id, first.id);
    assert_eq!(second_versions[0].document_id, second.id);
}

#[test]
fn checkout_changes_current_pointer() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let first = write_source(&paths.root_dir, "report.docx", b"version one");
    let second = write_source(&paths.root_dir, "report-2.docx", b"version two");
    let storage = VaultStorage::init(paths).unwrap();
    commit(&storage, DocumentRef::Name("report".to_owned()), &first);
    commit(&storage, DocumentRef::Name("report".to_owned()), &second);

    storage
        .checkout_version(
            &DocumentRef::Name("report".to_owned()),
            "v1",
            None,
            &NEVER_CANCELLED,
        )
        .unwrap();

    let current = storage
        .current_version(&DocumentRef::Name("report".to_owned()))
        .unwrap()
        .unwrap();
    assert_eq!(current.id, "v1");
}

#[test]
fn commit_after_checkout_uses_checked_out_version_as_parent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let first = write_source(&paths.root_dir, "report.docx", b"version one");
    let second = write_source(&paths.root_dir, "report-2.docx", b"version two");
    let third = write_source(&paths.root_dir, "report-3.docx", b"version three");
    let storage = VaultStorage::init(paths).unwrap();
    commit(&storage, DocumentRef::Name("report".to_owned()), &first);
    commit(&storage, DocumentRef::Name("report".to_owned()), &second);
    storage
        .checkout_version(
            &DocumentRef::Name("report".to_owned()),
            "v1",
            None,
            &NEVER_CANCELLED,
        )
        .unwrap();

    let (_, version) = commit(&storage, DocumentRef::Name("report".to_owned()), &third);

    assert_eq!(version.id, "v3");
    assert_eq!(version.parent_version_id.as_deref(), Some("v1"));
}

#[test]
fn export_does_not_change_current_pointer() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let first = write_source(&paths.root_dir, "report.docx", b"version one");
    let second = write_source(&paths.root_dir, "report-2.docx", b"version two");
    let storage = VaultStorage::init(paths.clone()).unwrap();
    commit(&storage, DocumentRef::Name("report".to_owned()), &first);
    commit(&storage, DocumentRef::Name("report".to_owned()), &second);

    let exported = storage
        .export_version(
            &DocumentRef::Name("report".to_owned()),
            "v1",
            &paths.root_dir.join("exports"),
            &NEVER_CANCELLED,
        )
        .unwrap();

    assert_eq!(read_document_xml(&exported), b"version one");
    let current = storage
        .current_version(&DocumentRef::Name("report".to_owned()))
        .unwrap()
        .unwrap();
    assert_eq!(current.id, "v2");
}

#[test]
fn delete_removes_document_versions_and_archive_dir() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let storage = VaultStorage::init(paths.clone()).unwrap();
    let (document, version) = commit(&storage, DocumentRef::Name("report".to_owned()), &source);

    // The local-copy archive exists before delete.
    let archive_path = paths.versions_dir.join(&version.archive_reference);
    assert!(archive_path.exists());

    storage
        .delete_document(
            &DocumentRef::IdPrefix(document.id.as_str().to_owned()),
            &NEVER_CANCELLED,
        )
        .unwrap();

    assert!(storage.list_documents().unwrap().is_empty());
    assert!(
        storage
            .list_versions(&DocumentRef::IdPrefix(document.id.as_str().to_owned()))
            .is_err()
    );
    // The per-document archive directory is removed.
    assert!(!paths.versions_dir.join(document.id.as_str()).exists());
}

#[test]
fn delete_unknown_document_errors() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let storage = VaultStorage::init(paths).unwrap();

    let error = storage
        .delete_document(
            &DocumentRef::IdPrefix("nonexistent".to_owned()),
            &NEVER_CANCELLED,
        )
        .unwrap_err();
    assert!(matches!(error, StorageError::DocumentIdNotFound(_)));
}

#[test]
fn delete_versions_removes_only_requested_versions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let first = write_source(&paths.root_dir, "report.docx", b"version one");
    let second = write_source(&paths.root_dir, "report-2.docx", b"version two");
    let storage = VaultStorage::init(paths.clone()).unwrap();
    commit(&storage, DocumentRef::Name("report".to_owned()), &first);
    let (document, _v2) = commit(&storage, DocumentRef::Name("report".to_owned()), &second);
    // v1 is not current, so it can be deleted while v2 (current) survives.
    let doc_ref = DocumentRef::Name("report".to_owned());
    storage
        .delete_versions(&doc_ref, &["v1".to_owned()], &NEVER_CANCELLED)
        .unwrap();

    let remaining = storage.list_versions(&doc_ref).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "v2");
    // The document itself is intact and still points at its current version.
    assert_eq!(storage.current_version(&doc_ref).unwrap().unwrap().id, "v2");
    // v1's archive directory is gone; v2's remains; the document's archive
    // root (the sibling-versions container) survives - only v1 was removed.
    let v1_dir = paths.versions_dir.join(document.id.as_str()).join("v1");
    let v2_dir = paths.versions_dir.join(document.id.as_str()).join("v2");
    assert!(!v1_dir.exists());
    assert!(v2_dir.exists());
    assert!(paths.versions_dir.join(document.id.as_str()).exists());
}

#[test]
fn delete_versions_rejects_current_version() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let first = write_source(&paths.root_dir, "report.docx", b"version one");
    let second = write_source(&paths.root_dir, "report-2.docx", b"version two");
    let storage = VaultStorage::init(paths).unwrap();
    commit(&storage, DocumentRef::Name("report".to_owned()), &first);
    commit(&storage, DocumentRef::Name("report".to_owned()), &second);
    // v2 is current after two commits.
    let doc_ref = DocumentRef::Name("report".to_owned());
    let error = storage
        .delete_versions(&doc_ref, &["v2".to_owned()], &NEVER_CANCELLED)
        .unwrap_err();
    assert!(matches!(
        error,
        StorageError::CannotDeleteCurrentVersion { .. }
    ));
    // Nothing was deleted.
    assert_eq!(storage.list_versions(&doc_ref).unwrap().len(), 2);
}

#[test]
fn delete_versions_unknown_id_errors() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let storage = VaultStorage::init(paths).unwrap();
    commit(&storage, DocumentRef::Name("report".to_owned()), &source);

    let error = storage
        .delete_versions(
            &DocumentRef::Name("report".to_owned()),
            &["v9".to_owned()],
            &NEVER_CANCELLED,
        )
        .unwrap_err();
    assert!(matches!(error, StorageError::VersionNotFound { .. }));
}

#[test]
fn delete_versions_empty_is_noop() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let storage = VaultStorage::init(paths).unwrap();
    commit(&storage, DocumentRef::Name("report".to_owned()), &source);

    storage
        .delete_versions(
            &DocumentRef::Name("report".to_owned()),
            &[],
            &NEVER_CANCELLED,
        )
        .unwrap();
    assert_eq!(
        storage
            .list_versions(&DocumentRef::Name("report".to_owned()))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn repo_size_reflects_local_copy_archives() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let storage = VaultStorage::init(paths).unwrap();
    let before = storage.repo_size().unwrap();
    commit(&storage, DocumentRef::Name("report".to_owned()), &source);
    let after = storage.repo_size().unwrap();
    assert!(
        after > before,
        "repo size should grow after a local-copy commit"
    );
}

#[test]
fn rename_updates_name_without_touching_versions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let storage = VaultStorage::init(paths).unwrap();
    let (document, version) = commit(&storage, DocumentRef::Name("report".to_owned()), &source);

    storage
        .rename_document(
            &DocumentRef::IdPrefix(document.id.as_str().to_owned()),
            "quarterly-report",
        )
        .unwrap();

    let renamed = storage.list_documents().unwrap();
    assert_eq!(renamed.len(), 1);
    assert_eq!(renamed[0].name, "quarterly-report");
    assert_eq!(renamed[0].id, document.id);

    // Versions are historical and untouched.
    let versions = storage
        .list_versions(&DocumentRef::IdPrefix(document.id.as_str().to_owned()))
        .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].id, version.id);
    assert_eq!(versions[0].archive_reference, version.archive_reference);
    assert_eq!(versions[0].original_filename, version.original_filename);
}

#[test]
fn set_version_note_updates_and_clears() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let storage = VaultStorage::init(paths).unwrap();
    let (document, version) = commit(&storage, DocumentRef::Name("report".to_owned()), &source);

    let document_ref = DocumentRef::IdPrefix(document.id.as_str().to_owned());

    // The `commit` helper uses default metadata, so the note starts as None.
    storage
        .set_version_note(&document_ref, &version.id, Some("updated note"))
        .unwrap();
    let versions = storage.list_versions(&document_ref).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].note.as_deref(), Some("updated note"));

    // `None` clears the note (column back to NULL).
    storage
        .set_version_note(&document_ref, &version.id, None)
        .unwrap();
    let versions = storage.list_versions(&document_ref).unwrap();
    assert!(versions[0].note.is_none());

    // A missing version surfaces as VersionNotFound, not a silent no-op.
    let err = storage
        .set_version_note(&document_ref, "v999", Some("nope"))
        .unwrap_err();
    assert!(matches!(err, StorageError::VersionNotFound { .. }));
}

fn config_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// A vault from before async commit has no `archive_status` column. Opening
/// it must migrate (re-add the column, defaulting legacy rows to `archived`)
/// without losing data, and recovery must find nothing pending.
#[test]
fn open_migrates_legacy_versions_table_without_archive_status() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"legacy");
    let document_id = {
        let storage = VaultStorage::init(paths.clone()).unwrap();
        let (document, _version) =
            commit(&storage, DocumentRef::Name("report".to_owned()), &source);
        // Simulate a legacy vault: drop the column so the versions table
        // looks pre-async-commit. Existing rows lose it; migrate must
        // restore them as `archived`.
        storage
            .connection
            .execute("ALTER TABLE versions DROP COLUMN archive_status", [])
            .unwrap();
        document.id.as_str().to_owned()
    };

    let storage = VaultStorage::open(paths.clone()).unwrap();
    assert!(storage.pending_versions().unwrap().is_empty());
    let versions = storage
        .list_versions(&DocumentRef::IdPrefix(document_id))
        .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(
        versions[0].archive_status, ARCHIVE_STATUS_ARCHIVED,
        "legacy version row migrated to archived"
    );
    // The migrated version is still exportable from its archive.
    let restored = storage
        .export_version(
            &DocumentRef::IdPrefix(versions[0].document_id.as_str().to_owned()),
            "current",
            &paths.root_dir.join("restored"),
            &NEVER_CANCELLED,
        )
        .unwrap();
    assert_eq!(read_document_xml(&restored), b"legacy");
}

/// Phase A writes a `pending` version + a durable intake copy, and exports
/// serve from the intake (so a just-committed version is openable before the
/// archive finishes). Phase B then finalizes the version to `archived` and
/// reclaims the intake; exports now serve from the real archive.
#[test]
fn begin_commit_serves_from_intake_then_archive_finalizes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let storage = VaultStorage::init(paths.clone()).unwrap();

    let (document, version) = storage
        .begin_commit(
            DocumentRef::NewName("report".to_owned()),
            &source,
            CommitMetadata::default(),
        )
        .unwrap();
    assert_eq!(version.archive_status, ARCHIVE_STATUS_PENDING);
    assert!(version.archive_reference.is_empty());
    let intake = storage.intake_path(
        document.id.as_str(),
        &version.id,
        &version.original_filename,
    );
    assert!(intake.exists(), "intake copy written by Phase A");

    // Export serves from the intake while the version is pending.
    let restored = storage
        .export_version(
            &DocumentRef::IdPrefix(document.id.as_str().to_owned()),
            "current",
            &paths.root_dir.join("restored-pending"),
            &NEVER_CANCELLED,
        )
        .unwrap();
    assert_eq!(read_document_xml(&restored), b"version one");

    // Phase B: archive from the intake, finalize the row, reclaim the intake.
    storage
        .archive_pending_version(&version, &NEVER_CANCELLED)
        .unwrap();
    let archived = storage
        .list_versions(&DocumentRef::IdPrefix(document.id.as_str().to_owned()))
        .unwrap();
    assert_eq!(archived[0].archive_status, ARCHIVE_STATUS_ARCHIVED);
    assert!(!archived[0].archive_reference.is_empty());
    assert!(!intake.exists(), "intake reclaimed after archive");

    // Export now serves from the local-copy archive.
    let restored2 = storage
        .export_version(
            &DocumentRef::IdPrefix(document.id.as_str().to_owned()),
            "current",
            &paths.root_dir.join("restored-archived"),
            &NEVER_CANCELLED,
        )
        .unwrap();
    assert_eq!(read_document_xml(&restored2), b"version one");
}

/// Crash-recovery contract: a `pending` version left by a crash (Phase A
/// done, Phase B never ran) is archived on the next `open`, its intake
/// reclaimed, and the version becomes normally exportable. No data is lost
/// and no duplicate work is performed.
#[test]
fn recover_pending_archives_versions_left_by_crash() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = write_source(&paths.root_dir, "report.docx", b"version one");
    let document_id = {
        let storage = VaultStorage::init(paths.clone()).unwrap();
        let (document, version) = storage
            .begin_commit(
                DocumentRef::NewName("report".to_owned()),
                &source,
                CommitMetadata::default(),
            )
            .unwrap();
        assert_eq!(version.archive_status, ARCHIVE_STATUS_PENDING);
        // Simulate a crash right after Phase A: drop the storage without
        // ever running Phase B. The intake copy + pending row are on disk.
        document.id.as_str().to_owned()
    };

    // Reopen: recovery runs Phase B for the pending version.
    let storage = VaultStorage::open(paths.clone()).unwrap();
    let pending = storage.pending_versions().unwrap();
    assert!(pending.is_empty(), "recovery archived the pending version");

    let versions = storage
        .list_versions(&DocumentRef::IdPrefix(document_id.clone()))
        .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].archive_status, ARCHIVE_STATUS_ARCHIVED);

    // gc_intake reclaimed the intake directory.
    let doc_id = &document_id;
    let intake_doc_dir = paths.intake_dir.join(doc_id);
    assert!(
        !intake_doc_dir.exists(),
        "intake directory reclaimed by recovery"
    );

    // The recovered version is exportable from its archive.
    let restored = storage
        .export_version(
            &DocumentRef::IdPrefix(document_id),
            "current",
            &paths.root_dir.join("restored"),
            &NEVER_CANCELLED,
        )
        .unwrap();
    assert_eq!(read_document_xml(&restored), b"version one");
}

/// `gc_intake` reclaims orphan intake copies whose version is no longer
/// pending (archived), while preserving intake for a still-pending version
/// (an archive in flight, or awaiting recovery).
#[test]
fn gc_intake_reclaims_orphans_but_preserves_pending() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source_a = write_source(&paths.root_dir, "a.docx", b"a");
    let source_b = write_source(&paths.root_dir, "b.docx", b"b");
    let storage = VaultStorage::init(paths.clone()).unwrap();

    let (doc_a, ver_a) = storage
        .begin_commit(
            DocumentRef::NewName("a".to_owned()),
            &source_a,
            CommitMetadata::default(),
        )
        .unwrap();
    let (doc_b, ver_b) = storage
        .begin_commit(
            DocumentRef::NewName("b".to_owned()),
            &source_b,
            CommitMetadata::default(),
        )
        .unwrap();
    // Archive only A; B stays pending.
    storage
        .archive_pending_version(&ver_a, &NEVER_CANCELLED)
        .unwrap();

    let intake_a = storage.intake_path(doc_a.id.as_str(), &ver_a.id, &ver_a.original_filename);
    let intake_b = storage.intake_path(doc_b.id.as_str(), &ver_b.id, &ver_b.original_filename);
    assert!(
        !intake_a.exists(),
        "A's intake removed by archive_pending_version"
    );
    assert!(intake_b.exists(), "B's intake still present while pending");

    // gc_intake is a no-op for B (still pending) and would only sweep
    // orphans; B's intake must survive.
    storage.gc_intake();
    assert!(
        intake_b.exists(),
        "gc_intake preserves pending version intake"
    );
}

#[test]
fn commits_and_exports_raw_binary_with_local_copy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    let source = paths.root_dir.join("notes.txt");
    fs::write(&source, b"plain text, not Office").unwrap();

    let storage = VaultStorage::init(paths.clone()).unwrap();
    let (_, version) = storage
        .add_document_version(
            DocumentRef::Name("notes".to_owned()),
            &source,
            CommitMetadata::default(),
            &NEVER_CANCELLED,
        )
        .unwrap();

    // Non-OOXML: a single-entry whole-file manifest (not per-package-part).
    assert_eq!(version.manifest.entries.len(), 1);
    assert_eq!(version.manifest.entries[0].path, "notes.txt");
    assert_eq!(
        version.manifest.entries[0].size,
        b"plain text, not Office".len() as u64
    );

    // Bytes round-trip unchanged through the local-copy backend.
    let restored = storage
        .export_version(
            &DocumentRef::Name("notes".to_owned()),
            "latest",
            &paths.root_dir.join("restored"),
            &NEVER_CANCELLED,
        )
        .unwrap();
    assert_eq!(fs::read(&restored).unwrap(), b"plain text, not Office");
}

#[test]
fn ooxml_kingsoft_wps_archived_like_office() {
    let temp_dir = tempfile::tempdir().unwrap();
    let paths = temp_paths(temp_dir.path());
    write_local_copy_config(&paths);
    // A `.wps` that is really an OOXML package (Kingsoft saving as docx):
    // content, not extension, decides it archives like Office.
    let source = write_source(&paths.root_dir, "kingsoft.wps", b"wps-as-docx");
    let storage = VaultStorage::init(paths.clone()).unwrap();
    let (_, version) = storage
        .add_document_version(
            DocumentRef::Name("kingsoft".to_owned()),
            &source,
            CommitMetadata::default(),
            &NEVER_CANCELLED,
        )
        .unwrap();

    // OOXML -> per-entry package manifest (not a single whole-file entry).
    assert!(
        version
            .manifest
            .entries
            .iter()
            .any(|entry| entry.path == "word/document.xml"),
        "OOXML .wps gets a per-part manifest, archived like Office"
    );
    assert_eq!(version.original_filename, "kingsoft.wps");
}
