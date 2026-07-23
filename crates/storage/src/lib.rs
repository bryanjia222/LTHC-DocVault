mod archive;
mod config;
mod error;
mod paths;
mod restic;
mod sqlite;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{SystemTime, UNIX_EPOCH},
};

use docvault_types::{CommitMetadata, Document, DocumentId, Version};
use rusqlite::{Connection, params};
use tracing::{debug, info, warn};
use uuid::Uuid;

pub(crate) use config::StorageSettings;
pub use config::{ResticConfig, StorageOverrides, write_initial_config};
pub use error::{DatabaseError, ResticError, StorageError, StorageResult};
pub use paths::VaultPaths;

/// A cancellation flag that is never set. Used for restic calls that run
/// outside a job (vault init/open, startup recovery), where there is no job to
/// cancel.
pub static NEVER_CANCELLED: AtomicBool = AtomicBool::new(false);

/// A version whose durable intake copy exists but whose compressed archive has
/// not been finalized yet (the async commit path is still running or was
/// interrupted by a crash). See [`Version::archive_status`].
pub const ARCHIVE_STATUS_PENDING: &str = "pending";

/// A version whose archive is complete and is the source of truth for
/// exports/restores. The default for every pre-async-commit version.
pub const ARCHIVE_STATUS_ARCHIVED: &str = "archived";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentRef {
    Name(String),
    NewName(String),
    IdPrefix(String),
    NameAndIdPrefix { name: String, id_prefix: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupBackend {
    LocalCopy,
    Restic,
}

impl BackupBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalCopy => "local-copy",
            Self::Restic => "restic",
        }
    }

    pub(crate) fn parse(value: &str) -> StorageResult<Self> {
        match value {
            "local-copy" | "copy" => Ok(Self::LocalCopy),
            "restic" => Ok(Self::Restic),
            other => Err(StorageError::InvalidBackend(other.to_owned())),
        }
    }
}

pub struct VaultStorage {
    pub(crate) paths: VaultPaths,
    pub(crate) settings: StorageSettings,
    pub(crate) connection: Connection,
    /// Best-effort `restic version` captured once at init/open. Empty for the
    /// local-copy backend or when restic is unavailable; avoids re-spawning on
    /// every config read.
    pub(crate) restic_version: String,
}

impl VaultStorage {
    pub fn init(paths: VaultPaths) -> StorageResult<Self> {
        Self::init_with_overrides(paths, &StorageOverrides::default())
    }

    /// Initialize a vault, applying caller-supplied `overrides` on top of the
    /// on-disk `config.toml` (see [`StorageOverrides`]). Used by the desktop
    /// (to inject the bundled restic path) and the CLI (for `--restic-path`);
    /// [`VaultStorage::init`] is the no-override convenience for tests and
    /// config-only callers.
    pub fn init_with_overrides(
        paths: VaultPaths,
        overrides: &StorageOverrides,
    ) -> StorageResult<Self> {
        info!(root_dir = %paths.root_dir.display(), "initializing vault storage");
        fs::create_dir_all(&paths.root_dir)?;
        fs::create_dir_all(&paths.data_dir)?;
        fs::create_dir_all(&paths.staging_dir)?;
        fs::create_dir_all(&paths.versions_dir)?;
        fs::create_dir_all(&paths.cache_dir)?;
        fs::create_dir_all(&paths.repo_dir)?;
        if let Some(parent) = paths.db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if !paths.config_path.exists() {
            fs::write(&paths.config_path, config::default_config(&paths)?)?;
        }

        let settings = config::read_settings(&paths, overrides)?;
        let connection = Connection::open(&paths.db_path)?;
        let mut storage = Self {
            paths,
            settings,
            connection,
            restic_version: String::new(),
        };
        storage.migrate()?;
        if storage.settings.backend == BackupBackend::Restic {
            storage.ensure_restic_repo(&NEVER_CANCELLED)?;
            storage.restic_version = storage.capture_restic_version();
        }
        info!(root_dir = %storage.paths.root_dir.display(), "vault storage initialized");
        Ok(storage)
    }

    pub fn open(paths: VaultPaths) -> StorageResult<Self> {
        Self::open_with_overrides(paths, &StorageOverrides::default())
    }

    /// Open an existing vault, applying caller-supplied `overrides` on top of
    /// the on-disk `config.toml` (see [`StorageOverrides`]). Used by the desktop
    /// (bundled restic path) and the CLI (`--restic-path`); [`VaultStorage::open`]
    /// is the no-override convenience.
    pub fn open_with_overrides(
        paths: VaultPaths,
        overrides: &StorageOverrides,
    ) -> StorageResult<Self> {
        debug!(root_dir = %paths.root_dir.display(), "opening vault storage");
        let settings = config::read_settings(&paths, overrides)?;
        let connection = Connection::open(&paths.db_path)?;
        let mut storage = Self {
            paths,
            settings,
            connection,
            restic_version: String::new(),
        };
        storage.migrate()?;
        if storage.settings.backend == BackupBackend::Restic {
            storage.restic_version = storage.capture_restic_version();
        }
        // Reclaim staging left behind by a crash/interruption in a previous
        // session. No operation is in flight at open, so anything here is stale.
        storage.gc_staging();
        // Finish any commit whose archive (Phase B) a crash interrupted. The
        // intake copy is durable, so the data is safe; this just completes the
        // compress step idempotently. Best-effort: a failure here is logged but
        // does not block opening the vault - the version stays `pending` and is
        // retried on the next open.
        if let Err(error) = storage.recover_pending(&NEVER_CANCELLED) {
            warn!(error = %error, "startup recovery of pending versions failed; leaving them pending");
        }
        Ok(storage)
    }

    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub fn backend(&self) -> BackupBackend {
        self.settings.backend
    }

    /// On-disk size of the backup repository, in bytes.
    /// - restic: `restic stats --mode raw-data` (post-dedup + compression).
    /// - local-copy: total size of the archived version files under
    ///   `versions_dir` (the local-copy "repo" is just those copies).
    pub fn repo_size(&self) -> StorageResult<u64> {
        match self.settings.backend {
            BackupBackend::Restic => self.restic_raw_data_size(&NEVER_CANCELLED),
            BackupBackend::LocalCopy => Ok(dir_size(&self.paths.versions_dir)),
        }
    }

    pub fn restic_path(&self) -> &Path {
        &self.settings.restic_path
    }

    /// The cached `restic version` string (empty for local-copy or when restic
    /// is unavailable). Captured once at init/open.
    pub fn restic_version(&self) -> &str {
        &self.restic_version
    }

    pub fn add_document_version(
        &self,
        document_ref: DocumentRef,
        source_path: &Path,
        metadata: CommitMetadata,
        cancel: &AtomicBool,
    ) -> StorageResult<(Document, Version)> {
        let now = unix_timestamp();
        info!(source = %source_path.display(), "adding document version");
        let document = match document_ref {
            DocumentRef::NewName(name) => {
                let document = Document {
                    id: DocumentId::new(Uuid::new_v4().to_string()),
                    name,
                    current_version_id: None,
                    created_at: now,
                };
                self.insert_document(&document)?;
                document
            }
            DocumentRef::Name(name) => match self.find_documents_by_name(&name)?.as_slice() {
                [] => {
                    if name.contains('@') {
                        return Err(StorageError::DocumentNotFound(name));
                    }
                    let document = Document {
                        id: DocumentId::new(Uuid::new_v4().to_string()),
                        name,
                        current_version_id: None,
                        created_at: now,
                    };
                    self.insert_document(&document)?;
                    document
                }
                [document] => document.clone(),
                matches => {
                    return Err(StorageError::AmbiguousDocumentName {
                        name,
                        matches: matches.to_vec(),
                    });
                }
            },
            other => self.resolve_document_ref(&other)?,
        };

        self.add_version_to_document(document, source_path, metadata, now, cancel)
    }

    pub fn add_document_version_to_name_or_create(
        &self,
        name: &str,
        source_path: &Path,
        metadata: CommitMetadata,
        cancel: &AtomicBool,
    ) -> StorageResult<(Document, Version)> {
        self.add_document_version(
            DocumentRef::Name(name.to_owned()),
            source_path,
            metadata,
            cancel,
        )
    }

    /// Phase A of the async commit: resolve the document (creating it if new),
    /// write a durable intake copy of the source (fsynced), then atomically
    /// insert the version row as `pending` and repoint the document's
    /// current-version pointer at it - all before any archiving. Returns the
    /// new document + version so the caller can materialize the library copy
    /// and spawn the Phase B archive job ([`Self::archive_pending_version`]).
    ///
    /// Crash safety: the intake copy is fsynced BEFORE the DB transaction
    /// commits, so a `pending` row always has its intake on disk (no commit
    /// data is lost to a crash). The version insert + current-pointer update
    /// run in one transaction, so the version either exists fully (visible and
    /// current) or not at all. The archive runs later and is idempotent, so a
    /// crash at any point is recovered on the next open by
    /// [`Self::recover_pending`].
    pub fn begin_commit(
        &self,
        document_ref: DocumentRef,
        source_path: &Path,
        metadata: CommitMetadata,
    ) -> StorageResult<(Document, Version)> {
        let now = unix_timestamp();
        info!(source = %source_path.display(), "beginning async document commit (phase A)");
        let document = match document_ref {
            DocumentRef::NewName(name) => {
                let document = Document {
                    id: DocumentId::new(Uuid::new_v4().to_string()),
                    name,
                    current_version_id: None,
                    created_at: now,
                };
                self.insert_document(&document)?;
                document
            }
            DocumentRef::Name(name) => match self.find_documents_by_name(&name)?.as_slice() {
                [] => {
                    if name.contains('@') {
                        return Err(StorageError::DocumentNotFound(name));
                    }
                    let document = Document {
                        id: DocumentId::new(Uuid::new_v4().to_string()),
                        name,
                        current_version_id: None,
                        created_at: now,
                    };
                    self.insert_document(&document)?;
                    document
                }
                [document] => document.clone(),
                matches => {
                    return Err(StorageError::AmbiguousDocumentName {
                        name,
                        matches: matches.to_vec(),
                    });
                }
            },
            other => self.resolve_document_ref(&other)?,
        };

        let number = self.next_version_number(document.id.as_str())?;
        let version_id = format!("v{number}");
        let manifest = docvault_ooxml::manifest_for(source_path)?;
        let original_filename = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StorageError::InvalidFileName(source_path.to_path_buf()))?
            .to_owned();

        // 1. Durable intake copy (fsynced) BEFORE the DB commit (WAL invariant:
        // a `pending` row => its intake bytes are on disk).
        let intake = self.intake_path(document.id.as_str(), &version_id, &original_filename);
        self.write_intake(source_path, &intake)?;

        // 2. Atomic transaction: insert the version row as `pending` (with a
        //    placeholder archive_reference filled in by Phase B) and repoint the
        //    current-version pointer. Either both land or neither does.
        let version = Version {
            id: version_id,
            document_id: document.id.clone(),
            number,
            original_filename,
            archive_reference: String::new(),
            backup_backend: self.settings.backend.as_str().to_owned(),
            snapshot_id: None,
            manifest,
            parent_version_id: document.current_version_id.clone(),
            author: metadata.author,
            note: metadata.note,
            created_at: now,
            archive_status: ARCHIVE_STATUS_PENDING.to_owned(),
        };
        let transaction = self.connection.unchecked_transaction()?;
        sqlite::insert_version_into(&transaction, &version)?;
        transaction.execute(
            "UPDATE documents SET current_version_id = ?1 WHERE id = ?2",
            params![&version.id, document.id.as_str()],
        )?;
        transaction.commit()?;

        let mut updated_document = document;
        updated_document.current_version_id = Some(version.id.clone());
        info!(
            document_id = updated_document.id.as_str(),
            version_id = version.id.as_str(),
            "document version committed (pending archive)"
        );
        Ok((updated_document, version))
    }

    /// Startup recovery: finish the Phase B archive for every `pending`
    /// version (each is idempotent, so a crash mid-archive is completed, not
    /// duplicated), then reclaim orphan intake copies. A pending version whose
    /// intake copy is missing (violating the WAL invariant) is left pending and
    /// logged - it cannot be archived without its source, but the rest of the
    /// vault stays usable. Returns the count of versions recovered (archived
    /// this run) so the caller can surface it.
    pub fn recover_pending(&self, cancel: &AtomicBool) -> StorageResult<usize> {
        let pending = self.pending_versions()?;
        if pending.is_empty() {
            // Still sweep orphans (e.g. intake left by a crash after the DB
            // row was archived but before the intake file was deleted).
            self.gc_intake();
            return Ok(0);
        }
        let mut recovered = 0;
        for version in &pending {
            match self.archive_pending_version(version, cancel) {
                Ok(()) => recovered += 1,
                Err(StorageError::IntakeMissing {
                    document_id,
                    version_id,
                }) => {
                    warn!(
                        document_id,
                        version_id,
                        "pending version has no intake copy; left pending for manual review"
                    );
                }
                Err(error) => return Err(error),
            }
        }
        self.gc_intake();
        if recovered > 0 {
            info!(recovered, "recovered pending versions on open");
        }
        Ok(recovered)
    }

    pub fn export_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: &Path,
        cancel: &AtomicBool,
    ) -> StorageResult<PathBuf> {
        let document = self.resolve_document_ref(document_ref)?;
        let version = self.resolve_requested_version(&document, requested_version)?;
        info!(
            document_id = document.id.as_str(),
            version_id = version.id.as_str(),
            output = %output_path.display(),
            "exporting document version"
        );
        self.export_resolved_version(&document, &version, output_path, cancel)
    }

    pub fn checkout_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: Option<&Path>,
        cancel: &AtomicBool,
    ) -> StorageResult<Option<PathBuf>> {
        let document = self.resolve_document_ref(document_ref)?;
        let version = self.resolve_requested_version(&document, requested_version)?;
        info!(
            document_id = document.id.as_str(),
            version_id = version.id.as_str(),
            "checking out document version"
        );
        self.set_current_version(document.id.as_str(), &version.id)?;
        output_path
            .map(|output_path| {
                self.export_resolved_version(&document, &version, output_path, cancel)
            })
            .transpose()
    }

    pub fn current_version(&self, document_ref: &DocumentRef) -> StorageResult<Option<Version>> {
        let document = self.resolve_document_ref(document_ref)?;
        let Some(current_version_id) = document.current_version_id else {
            return Ok(None);
        };
        self.find_version(document.id.as_str(), &current_version_id)
    }

    /// Delete a document and all of its versions. For the restic backend the
    /// version snapshots are forgotten (and pruned) first so the DB and repo
    /// stay consistent; a forget failure aborts the delete (the rows remain,
    /// the user can retry). The local-copy archive directory is removed
    /// best-effort afterwards. The on-disk source file is never touched -
    /// DocVault does not own the user's working copy.
    pub fn delete_document(
        &self,
        document_ref: &DocumentRef,
        cancel: &AtomicBool,
    ) -> StorageResult<()> {
        let document = self.resolve_document_ref(document_ref)?;
        let versions = self.versions_for_document(document.id.as_str())?;
        if self.settings.backend == BackupBackend::Restic {
            let snapshot_ids: Vec<String> = versions
                .iter()
                .filter_map(|version| version.snapshot_id.clone())
                .collect();
            if !snapshot_ids.is_empty() {
                self.restic_forget(&snapshot_ids, cancel)?;
            }
        }
        self.remove_document(document.id.as_str())?;
        // Best-effort: a missing/busy dir is logged, not fatal - the DB rows
        // (the source of truth for existence) are already gone.
        let version_dir = self.paths.versions_dir.join(document.id.as_str());
        if version_dir.exists()
            && let Err(error) = fs::remove_dir_all(&version_dir)
        {
            warn!(
                document_id = document.id.as_str(),
                path = %version_dir.display(),
                error = %error,
                "failed to remove local archive directory after delete"
            );
        }
        info!(document_id = document.id.as_str(), "document deleted");
        Ok(())
    }

    /// Delete specific versions of a document by id, keeping the document and its
    /// other versions. For the restic backend each version's snapshot is
    /// forgotten (and pruned) first - in one batched call - so the DB and repo
    /// stay consistent; a forget failure aborts the delete (the rows remain, the
    /// user can retry). The local-copy archive directory for each version is then
    /// removed best-effort. The on-disk source file, the document, and its other
    /// versions are untouched.
    ///
    /// The caller owns the subtree policy: this deletes exactly the given ids (a
    /// version plus its descendants, when the user confirmed), never reparenting
    /// or orphaning survivors. The current (checked-out) version is refused so
    /// the document's `current_version_id` pointer never dangles.
    pub fn delete_versions(
        &self,
        document_ref: &DocumentRef,
        version_ids: &[String],
        cancel: &AtomicBool,
    ) -> StorageResult<()> {
        if version_ids.is_empty() {
            return Ok(());
        }
        let document = self.resolve_document_ref(document_ref)?;
        if let Some(current) = &document.current_version_id
            && version_ids.iter().any(|id| id == current)
        {
            return Err(StorageError::CannotDeleteCurrentVersion {
                document_name: document.name.clone(),
                version_id: current.clone(),
            });
        }
        let versions = self.versions_for_document(document.id.as_str())?;
        // Resolve each requested id to its row; a stale/unknown id surfaces as
        // VersionNotFound rather than a silent no-op.
        let to_delete: Vec<Version> = version_ids
            .iter()
            .map(|id| {
                versions
                    .iter()
                    .find(|v| &v.id == id)
                    .cloned()
                    .ok_or_else(|| StorageError::VersionNotFound {
                        document_name: document.name.clone(),
                        version: id.clone(),
                    })
            })
            .collect::<StorageResult<Vec<_>>>()?;
        if self.settings.backend == BackupBackend::Restic {
            let snapshot_ids: Vec<String> = to_delete
                .iter()
                .filter_map(|version| version.snapshot_id.clone())
                .collect();
            if !snapshot_ids.is_empty() {
                self.restic_forget(&snapshot_ids, cancel)?;
            }
        }
        self.remove_versions(document.id.as_str(), version_ids)?;
        // Best-effort per-version archive cleanup (a missing/busy dir is logged,
        // not fatal - the DB rows, the source of truth, are already gone). The
        // version's archive lives at `versions_dir/<doc>/<v>/` (local-copy); for
        // the restic backend nothing is written under `versions_dir`, so the dir
        // simply does not exist and this is a no-op. A `pending` version whose
        // archive isn't materialized yet likewise has no dir here.
        for version in &to_delete {
            let version_dir = self
                .paths
                .versions_dir
                .join(document.id.as_str())
                .join(&version.id);
            if version_dir.exists()
                && let Err(error) = fs::remove_dir_all(&version_dir)
            {
                warn!(
                    document_id = document.id.as_str(),
                    version_id = version.id.as_str(),
                    path = %version_dir.display(),
                    error = %error,
                    "failed to remove local archive directory after version delete"
                );
            }
        }
        info!(
            document_id = document.id.as_str(),
            count = to_delete.len(),
            "versions deleted"
        );
        Ok(())
    }

    /// Rename a document's display name. Does not touch the on-disk source file
    /// or any version's `original_filename` (historical).
    pub fn rename_document(&self, document_ref: &DocumentRef, new_name: &str) -> StorageResult<()> {
        let document = self.resolve_document_ref(document_ref)?;
        self.set_document_name(document.id.as_str(), new_name)?;
        info!(
            document_id = document.id.as_str(),
            new_name, "document renamed"
        );
        Ok(())
    }

    /// Update a version's note (its commit message). `None` clears it. The
    /// archive and every other version field are untouched. A missing version
    /// (or one that does not belong to the resolved document) is surfaced as
    /// [`StorageError::VersionNotFound`] rather than a silent no-op.
    pub fn set_version_note(
        &self,
        document_ref: &DocumentRef,
        version_id: &str,
        note: Option<&str>,
    ) -> StorageResult<()> {
        let document = self.resolve_document_ref(document_ref)?;
        if self
            .find_version(document.id.as_str(), version_id)?
            .is_none()
        {
            return Err(StorageError::VersionNotFound {
                document_name: document.name,
                version: version_id.to_owned(),
            });
        }
        self.update_version_note(document.id.as_str(), version_id, note)?;
        info!(
            document_id = document.id.as_str(),
            version_id, "version note updated"
        );
        Ok(())
    }

    pub(crate) fn add_version_to_document(
        &self,
        document: Document,
        source_path: &Path,
        metadata: CommitMetadata,
        now: i64,
        cancel: &AtomicBool,
    ) -> StorageResult<(Document, Version)> {
        let number = self.next_version_number(document.id.as_str())?;
        let version_id = format!("v{number}");
        debug!(
            document_id = document.id.as_str(),
            version_id = version_id.as_str(),
            "archiving source for document version"
        );
        let manifest = docvault_ooxml::manifest_for(source_path)?;
        let archive =
            self.archive_source(document.id.as_str(), &version_id, source_path, cancel)?;
        let original_filename = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StorageError::InvalidFileName(source_path.to_path_buf()))?
            .to_owned();
        let version = Version {
            id: version_id,
            document_id: document.id.clone(),
            number,
            original_filename,
            archive_reference: archive.reference,
            backup_backend: archive.backend.as_str().to_owned(),
            snapshot_id: archive.snapshot_id,
            manifest,
            parent_version_id: document.current_version_id.clone(),
            author: metadata.author,
            note: metadata.note,
            created_at: now,
            archive_status: ARCHIVE_STATUS_ARCHIVED.to_owned(),
        };
        self.insert_version(&version)?;
        self.set_current_version(document.id.as_str(), &version.id)?;
        let mut updated_document = document;
        updated_document.current_version_id = Some(version.id.clone());
        info!(
            document_id = updated_document.id.as_str(),
            version_id = version.id.as_str(),
            backend = version.backup_backend.as_str(),
            "document version added"
        );
        Ok((updated_document, version))
    }

    pub fn create_document(&self, name: &str, now: i64) -> StorageResult<Document> {
        let document = Document {
            id: DocumentId::new(Uuid::new_v4().to_string()),
            name: name.to_owned(),
            current_version_id: None,
            created_at: now,
        };
        self.insert_document(&document)?;
        Ok(document)
    }

    pub fn list_documents(&self) -> StorageResult<Vec<Document>> {
        self.list_all_documents()
    }

    pub fn list_versions(&self, document_ref: &DocumentRef) -> StorageResult<Vec<Version>> {
        let document = self.resolve_document_ref(document_ref)?;
        self.versions_for_document(document.id.as_str())
    }

    /// Look up a single document's display name by id, without scanning all
    /// documents. `DocumentIdNotFound` when no document has that exact id.
    pub fn document_name(&self, id: &str) -> StorageResult<String> {
        self.document_name_by_id(id)
    }

    fn resolve_requested_version(
        &self,
        document: &Document,
        requested_version: &str,
    ) -> StorageResult<Version> {
        let resolved = if requested_version == "current" {
            match document.current_version_id.as_deref() {
                Some(version_id) => self.find_version(document.id.as_str(), version_id)?,
                None => None,
            }
        } else {
            self.find_version(document.id.as_str(), requested_version)?
        };
        resolved.ok_or_else(|| StorageError::VersionNotFound {
            document_name: document.name.clone(),
            version: requested_version.to_owned(),
        })
    }
}

pub(crate) fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Recursively sum the sizes of all files under `path`. A missing directory
/// (or one that can't be read) contributes 0 rather than failing, so a repo
/// size read never errors on a transiently-unreadable subtree.
fn dir_size(path: &Path) -> u64 {
    fn walk(path: &Path) -> u64 {
        let mut total = 0u64;
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += walk(&path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
        total
    }
    walk(path)
}

pub(crate) fn format_document_matches(matches: &[Document]) -> String {
    let mut output = String::from("matches:\n");
    for document in matches {
        output.push_str(&format!("  {}  {}\n", document.id.as_str(), document.name));
    }
    output.push_str("use --id <document_id> or name@<id-prefix>");
    output
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use docvault_types::CommitMetadata;

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
        let (document, _v2) =
            commit(&storage, DocumentRef::Name("report".to_owned()), &second);
        // v1 is not current, so it can be deleted while v2 (current) survives.
        let doc_ref = DocumentRef::Name("report".to_owned());
        storage
            .delete_versions(&doc_ref, &["v1".to_owned()], &NEVER_CANCELLED)
            .unwrap();

        let remaining = storage.list_versions(&doc_ref).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "v2");
        // The document itself is intact and still points at its current version.
        assert_eq!(
            storage.current_version(&doc_ref).unwrap().unwrap().id,
            "v2"
        );
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
            .delete_versions(&DocumentRef::Name("report".to_owned()), &[], &NEVER_CANCELLED)
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
}
