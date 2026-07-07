use std::{
    env, fs,
    hash::Hasher,
    io,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use docvault_ooxml::OoxmlError;
use docvault_types::{Document, DocumentId, ImportMetadata, TrackedPath, TrackedScan, Version};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Ooxml(OoxmlError),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    DocumentNotFound(String),
    DocumentIdNotFound(String),
    AmbiguousDocumentName {
        name: String,
        matches: Vec<Document>,
    },
    AmbiguousDocumentIdPrefix {
        prefix: String,
        matches: Vec<Document>,
    },
    DocumentReferenceMismatch {
        requested_name: String,
        matched: Box<Document>,
    },
    VersionNotFound {
        document_name: String,
        version: String,
    },
    InvalidFileName(PathBuf),
    InvalidBackend(String),
    ResticFailed {
        command: String,
        stderr: String,
    },
    ResticSnapshotMissing,
    TrackedPathNotFound(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Ooxml(error) => write!(f, "OOXML error: {error}"),
            Self::Sqlite(error) => write!(f, "SQLite error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::DocumentNotFound(name) => write!(f, "document not found: {name}"),
            Self::DocumentIdNotFound(id) => write!(f, "document id not found: {id}"),
            Self::AmbiguousDocumentName { name, matches } => {
                writeln!(f, "document name is ambiguous: {name}")?;
                write_document_matches(f, matches)
            }
            Self::AmbiguousDocumentIdPrefix { prefix, matches } => {
                writeln!(f, "document id prefix is ambiguous: {prefix}")?;
                write_document_matches(f, matches)
            }
            Self::DocumentReferenceMismatch {
                requested_name,
                matched,
            } => {
                write!(
                    f,
                    "document reference mismatch: requested name {requested_name}, matched {} ({})",
                    matched.name,
                    matched.id.as_str()
                )
            }
            Self::VersionNotFound {
                document_name,
                version,
            } => {
                write!(
                    f,
                    "version {version} not found for document {document_name}"
                )
            }
            Self::InvalidFileName(path) => write!(f, "invalid file name: {}", path.display()),
            Self::InvalidBackend(backend) => write!(f, "invalid backup backend: {backend}"),
            Self::ResticFailed { command, stderr } => {
                write!(f, "restic command failed ({command}): {stderr}")
            }
            Self::ResticSnapshotMissing => write!(f, "restic backup did not return a snapshot id"),
            Self::TrackedPathNotFound(path) => write!(f, "tracked path not found: {path}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<io::Error> for StorageError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<OoxmlError> for StorageError {
    fn from(value: OoxmlError) -> Self {
        Self::Ooxml(value)
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub type StorageResult<T> = Result<T, StorageError>;

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
    fn as_str(self) -> &'static str {
        match self {
            Self::LocalCopy => "local-copy",
            Self::Restic => "restic",
        }
    }

    fn parse(value: &str) -> StorageResult<Self> {
        match value {
            "local-copy" | "copy" => Ok(Self::LocalCopy),
            "restic" => Ok(Self::Restic),
            other => Err(StorageError::InvalidBackend(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    pub root_dir: PathBuf,
    pub data_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub repo_dir: PathBuf,
    pub db_path: PathBuf,
    pub config_path: PathBuf,
}

impl VaultPaths {
    pub fn from_env() -> Self {
        let root_dir = env::var_os("DOCVAULT_ROOT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".docvault"));
        let data_dir = env::var_os("DOCVAULT_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root_dir.join("data"));
        let db_path = env::var_os("DOCVAULT_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| root_dir.join("db.sqlite"));

        Self::new(root_dir, data_dir, db_path)
    }

    pub fn new(
        root_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
        db_path: impl Into<PathBuf>,
    ) -> Self {
        let root_dir = absolute_path(root_dir.into());
        let data_dir = absolute_path(data_dir.into());
        let db_path = absolute_path(db_path.into());
        Self {
            staging_dir: data_dir.join("staging"),
            versions_dir: data_dir.join("versions"),
            cache_dir: root_dir.join("cache"),
            repo_dir: root_dir.join("repo"),
            config_path: root_dir.join("config.toml"),
            root_dir,
            data_dir,
            db_path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResticConfig {
    pub repo_dir: PathBuf,
    pub restic_path: Option<PathBuf>,
}

impl ResticConfig {
    pub fn new(repo_dir: impl Into<PathBuf>) -> Self {
        Self {
            repo_dir: repo_dir.into(),
            restic_path: None,
        }
    }

    pub fn with_restic_path(mut self, restic_path: impl Into<PathBuf>) -> Self {
        self.restic_path = Some(restic_path.into());
        self
    }
}

pub struct VaultStorage {
    paths: VaultPaths,
    settings: StorageSettings,
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StorageSettings {
    backend: BackupBackend,
    restic_path: PathBuf,
    restic_password: String,
}

impl VaultStorage {
    pub fn init(paths: VaultPaths) -> StorageResult<Self> {
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
            fs::write(&paths.config_path, default_config(&paths))?;
        }

        let settings = read_settings(&paths)?;
        let connection = Connection::open(&paths.db_path)?;
        let storage = Self {
            paths,
            settings,
            connection,
        };
        storage.migrate()?;
        if storage.settings.backend == BackupBackend::Restic {
            storage.ensure_restic_repo()?;
        }
        Ok(storage)
    }

    pub fn open(paths: VaultPaths) -> StorageResult<Self> {
        let settings = read_settings(&paths)?;
        let connection = Connection::open(&paths.db_path)?;
        let storage = Self {
            paths,
            settings,
            connection,
        };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub fn backend(&self) -> BackupBackend {
        self.settings.backend
    }

    pub fn add_document_version(
        &self,
        document_ref: DocumentRef,
        source_path: &Path,
        metadata: ImportMetadata,
    ) -> StorageResult<(Document, Version)> {
        let now = unix_timestamp();
        let document = match document_ref {
            DocumentRef::NewName(name) => {
                let document = Document {
                    id: DocumentId::new(Uuid::new_v4().to_string()),
                    name,
                    source_path: source_path.display().to_string(),
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
                        source_path: source_path.display().to_string(),
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

        self.add_version_to_document(document, source_path, metadata, now)
    }

    pub fn add_document_version_to_name_or_create(
        &self,
        name: &str,
        source_path: &Path,
        metadata: ImportMetadata,
    ) -> StorageResult<(Document, Version)> {
        self.add_document_version(DocumentRef::Name(name.to_owned()), source_path, metadata)
    }

    pub fn export_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: &Path,
    ) -> StorageResult<PathBuf> {
        let document = self.resolve_document_ref(document_ref)?;
        let version = self
            .find_version(document.id.as_str(), requested_version)?
            .ok_or_else(|| StorageError::VersionNotFound {
                document_name: document.name.clone(),
                version: requested_version.to_owned(),
            })?;
        self.export_resolved_version(&document, &version, output_path)
    }

    pub fn checkout_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: Option<&Path>,
    ) -> StorageResult<Option<PathBuf>> {
        let document = self.resolve_document_ref(document_ref)?;
        let version = self
            .find_version(document.id.as_str(), requested_version)?
            .ok_or_else(|| StorageError::VersionNotFound {
                document_name: document.name.clone(),
                version: requested_version.to_owned(),
            })?;
        self.set_current_version(document.id.as_str(), &version.id)?;
        output_path
            .map(|output_path| self.export_resolved_version(&document, &version, output_path))
            .transpose()
    }

    pub fn current_version(&self, document_ref: &DocumentRef) -> StorageResult<Option<Version>> {
        let document = self.resolve_document_ref(document_ref)?;
        let Some(current_version_id) = document.current_version_id else {
            return Ok(None);
        };
        self.find_version(document.id.as_str(), &current_version_id)
    }

    pub fn track_path(
        &self,
        path: &Path,
        document_ref: Option<&DocumentRef>,
    ) -> StorageResult<TrackedPath> {
        let document_id = document_ref
            .map(|document_ref| self.resolve_document_ref(document_ref))
            .transpose()?
            .map(|document| document.id);
        self.track_document_path(path, document_id.as_ref())
    }

    pub fn track_document_path(
        &self,
        path: &Path,
        document_id: Option<&DocumentId>,
    ) -> StorageResult<TrackedPath> {
        let now = unix_timestamp();
        let path = absolute_path(path.to_path_buf()).display().to_string();
        let existing = self.find_tracked_path_by_path(&path)?;
        match existing {
            Some(mut tracked_path) => {
                self.connection.execute(
                    "UPDATE tracked_paths SET document_id = ?1 WHERE id = ?2",
                    params![
                        document_id.map(DocumentId::as_str),
                        tracked_path.id.as_str()
                    ],
                )?;
                tracked_path.document_id = document_id.cloned();
                Ok(tracked_path)
            }
            None => {
                let tracked_path = TrackedPath {
                    id: Uuid::new_v4().to_string(),
                    document_id: document_id.cloned(),
                    path,
                    fingerprint: None,
                    last_scanned_at: None,
                    created_at: now,
                };
                self.insert_tracked_path(&tracked_path)?;
                Ok(tracked_path)
            }
        }
    }

    pub fn list_tracked_paths(&self) -> StorageResult<Vec<TrackedPath>> {
        let mut statement = self.connection.prepare(
            "SELECT id, document_id, path, fingerprint, last_scanned_at, created_at FROM tracked_paths ORDER BY created_at, path, id",
        )?;
        let tracked_paths = statement
            .query_map([], tracked_path_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tracked_paths)
    }

    pub fn scan_tracked_paths(&self) -> StorageResult<Vec<TrackedScan>> {
        let tracked_paths = self.list_tracked_paths()?;
        tracked_paths
            .into_iter()
            .map(|tracked_path| self.scan_tracked_path(tracked_path))
            .collect()
    }

    fn add_version_to_document(
        &self,
        document: Document,
        source_path: &Path,
        metadata: ImportMetadata,
        now: i64,
    ) -> StorageResult<(Document, Version)> {
        let number = self.next_version_number(document.id.as_str())?;
        let version_id = format!("v{number}");
        let archive = self.archive_source(&document, &version_id, source_path)?;
        let version = Version {
            id: version_id,
            document_id: document.id.clone(),
            number,
            original_path: source_path.display().to_string(),
            archive_path: archive.reference.display().to_string(),
            backup_backend: archive.backend.as_str().to_owned(),
            snapshot_id: archive.snapshot_id,
            parent_version_id: document.current_version_id.clone(),
            author: metadata.author,
            note: metadata.note,
            created_at: now,
        };
        self.insert_version(&version)?;
        self.set_current_version(document.id.as_str(), &version.id)?;
        let mut updated_document = document;
        updated_document.current_version_id = Some(version.id.clone());
        Ok((updated_document, version))
    }

    pub fn create_document(
        &self,
        name: &str,
        source_path: &Path,
        now: i64,
    ) -> StorageResult<Document> {
        let document = Document {
            id: DocumentId::new(Uuid::new_v4().to_string()),
            name: name.to_owned(),
            source_path: source_path.display().to_string(),
            current_version_id: None,
            created_at: now,
        };
        self.insert_document(&document)?;
        Ok(document)
    }

    pub fn list_documents(&self) -> StorageResult<Vec<Document>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, source_path, current_version_id, created_at FROM documents ORDER BY created_at, name, id",
        )?;
        let documents = statement
            .query_map([], document_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(documents)
    }

    pub fn list_versions(&self, document_ref: &DocumentRef) -> StorageResult<Vec<Version>> {
        let document = self.resolve_document_ref(document_ref)?;
        self.versions_for_document(document.id.as_str())
    }

    pub fn restore_version(
        &self,
        document_ref: &DocumentRef,
        requested_version: &str,
        output_path: &Path,
    ) -> StorageResult<PathBuf> {
        self.export_version(document_ref, requested_version, output_path)
    }

    fn migrate(&self) -> StorageResult<()> {
        self.connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                source_path TEXT NOT NULL,
                current_version_id TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS versions (
                id TEXT NOT NULL,
                document_id TEXT NOT NULL,
                number INTEGER NOT NULL,
                original_path TEXT NOT NULL,
                archive_path TEXT NOT NULL,
                backup_backend TEXT NOT NULL DEFAULT 'local-copy',
                snapshot_id TEXT,
                parent_version_id TEXT,
                author TEXT,
                note TEXT,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (document_id, id),
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );

            CREATE TABLE IF NOT EXISTS tracked_paths (
                id TEXT PRIMARY KEY,
                document_id TEXT,
                path TEXT NOT NULL UNIQUE,
                fingerprint TEXT,
                last_scanned_at INTEGER,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );
            ",
        )?;
        Ok(())
    }

    fn insert_document(&self, document: &Document) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO documents (id, name, source_path, current_version_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                document.id.as_str(),
                document.name,
                document.source_path,
                document.current_version_id,
                document.created_at
            ],
        )?;
        Ok(())
    }

    fn insert_tracked_path(&self, tracked_path: &TrackedPath) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO tracked_paths (id, document_id, path, fingerprint, last_scanned_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                tracked_path.id,
                tracked_path.document_id.as_ref().map(DocumentId::as_str),
                tracked_path.path,
                tracked_path.fingerprint,
                tracked_path.last_scanned_at,
                tracked_path.created_at,
            ],
        )?;
        Ok(())
    }

    fn find_tracked_path_by_path(&self, path: &str) -> StorageResult<Option<TrackedPath>> {
        self.connection
            .query_row(
                "SELECT id, document_id, path, fingerprint, last_scanned_at, created_at FROM tracked_paths WHERE path = ?1",
                [path],
                tracked_path_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn scan_tracked_path(&self, mut tracked_path: TrackedPath) -> StorageResult<TrackedScan> {
        let previous_fingerprint = tracked_path.fingerprint.clone();
        let scanned_at = unix_timestamp();
        let path = PathBuf::from(&tracked_path.path);
        let fingerprint = if path.is_file() {
            Some(file_fingerprint(&path)?)
        } else {
            None
        };
        let exists = fingerprint.is_some();
        let changed = fingerprint != previous_fingerprint;
        self.connection.execute(
            "UPDATE tracked_paths SET fingerprint = ?1, last_scanned_at = ?2 WHERE id = ?3",
            params![fingerprint, scanned_at, tracked_path.id],
        )?;
        tracked_path.fingerprint = fingerprint.clone();
        tracked_path.last_scanned_at = Some(scanned_at);
        Ok(TrackedScan {
            tracked_path,
            fingerprint,
            changed,
            exists,
            scanned_at,
        })
    }

    fn insert_version(&self, version: &Version) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO versions (
                id, document_id, number, original_path, archive_path, backup_backend, snapshot_id, parent_version_id, author, note, created_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                version.id,
                version.document_id.as_str(),
                version.number,
                version.original_path,
                version.archive_path,
                version.backup_backend,
                version.snapshot_id,
                version.parent_version_id,
                version.author,
                version.note,
                version.created_at
            ],
        )?;
        Ok(())
    }

    fn find_documents_by_name(&self, name: &str) -> StorageResult<Vec<Document>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, source_path, current_version_id, created_at FROM documents WHERE name = ?1 ORDER BY created_at, id",
        )?;
        let documents = statement
            .query_map([name], document_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(documents)
    }

    fn resolve_document_ref(&self, document_ref: &DocumentRef) -> StorageResult<Document> {
        match document_ref {
            DocumentRef::Name(name) => match self.find_documents_by_name(name)?.as_slice() {
                [] => Err(StorageError::DocumentNotFound(name.clone())),
                [document] => Ok(document.clone()),
                matches => Err(StorageError::AmbiguousDocumentName {
                    name: name.clone(),
                    matches: matches.to_vec(),
                }),
            },
            DocumentRef::NewName(name) => Err(StorageError::DocumentNotFound(name.clone())),
            DocumentRef::IdPrefix(prefix) => self.resolve_document_id_prefix(prefix),
            DocumentRef::NameAndIdPrefix { name, id_prefix } => {
                let document = self.resolve_document_id_prefix(id_prefix)?;
                if document.name == *name {
                    Ok(document)
                } else {
                    Err(StorageError::DocumentReferenceMismatch {
                        requested_name: name.clone(),
                        matched: Box::new(document),
                    })
                }
            }
        }
    }

    fn resolve_document_id_prefix(&self, prefix: &str) -> StorageResult<Document> {
        let pattern = format!("{prefix}%");
        let mut statement = self.connection.prepare(
            "SELECT id, name, source_path, current_version_id, created_at FROM documents WHERE id LIKE ?1 ORDER BY created_at, id",
        )?;
        let documents = statement
            .query_map([pattern], document_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        match documents.as_slice() {
            [] => Err(StorageError::DocumentIdNotFound(prefix.to_owned())),
            [document] => Ok(document.clone()),
            matches => Err(StorageError::AmbiguousDocumentIdPrefix {
                prefix: prefix.to_owned(),
                matches: matches.to_vec(),
            }),
        }
    }

    fn set_current_version(&self, document_id: &str, version_id: &str) -> StorageResult<()> {
        self.connection.execute(
            "UPDATE documents SET current_version_id = ?1 WHERE id = ?2",
            params![version_id, document_id],
        )?;
        Ok(())
    }

    fn next_version_number(&self, document_id: &str) -> StorageResult<i64> {
        let current = self.connection.query_row(
            "SELECT COALESCE(MAX(number), 0) FROM versions WHERE document_id = ?1",
            [document_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(current + 1)
    }

    fn versions_for_document(&self, document_id: &str) -> StorageResult<Vec<Version>> {
        let mut statement = self.connection.prepare(
            "SELECT id, document_id, number, original_path, archive_path, backup_backend, snapshot_id, parent_version_id, author, note, created_at
             FROM versions WHERE document_id = ?1 ORDER BY number",
        )?;
        let versions = statement
            .query_map([document_id], version_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(versions)
    }

    fn find_version(
        &self,
        document_id: &str,
        requested_version: &str,
    ) -> StorageResult<Option<Version>> {
        let version_id = if requested_version == "latest" {
            self.connection
                .query_row(
                    "SELECT id FROM versions WHERE document_id = ?1 ORDER BY number DESC LIMIT 1",
                    [document_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
        } else {
            Some(requested_version.to_owned())
        };

        let Some(version_id) = version_id else {
            return Ok(None);
        };

        self.connection
            .query_row(
                "SELECT id, document_id, number, original_path, archive_path, backup_backend, snapshot_id, parent_version_id, author, note, created_at
                 FROM versions WHERE document_id = ?1 AND id = ?2",
                params![document_id, version_id],
                version_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn archive_source(
        &self,
        document: &Document,
        version_id: &str,
        source_path: &Path,
    ) -> StorageResult<ArchiveReference> {
        match self.settings.backend {
            BackupBackend::LocalCopy => self.archive_local_copy(document, version_id, source_path),
            BackupBackend::Restic => self.archive_restic(document, version_id, source_path),
        }
    }

    fn archive_local_copy(
        &self,
        document: &Document,
        version_id: &str,
        source_path: &Path,
    ) -> StorageResult<ArchiveReference> {
        let source_name = source_path
            .file_name()
            .ok_or_else(|| StorageError::InvalidFileName(source_path.to_path_buf()))?;
        let version_dir = self
            .paths
            .versions_dir
            .join(document.id.as_str())
            .join(version_id);
        fs::create_dir_all(&version_dir)?;
        let archive_path = version_dir.join(source_name);
        fs::copy(source_path, &archive_path)?;
        Ok(ArchiveReference {
            backend: BackupBackend::LocalCopy,
            reference: archive_path,
            snapshot_id: None,
        })
    }

    fn archive_restic(
        &self,
        document: &Document,
        version_id: &str,
        source_path: &Path,
    ) -> StorageResult<ArchiveReference> {
        self.ensure_restic_repo()?;
        let package_dir = self.restic_package_dir(document, version_id);
        reset_dir(&package_dir)?;
        docvault_ooxml::unpack_package(source_path, &package_dir)?;

        let snapshot_id = self.restic_backup(document, version_id, &package_dir)?;
        Ok(ArchiveReference {
            backend: BackupBackend::Restic,
            reference: package_dir,
            snapshot_id: Some(snapshot_id),
        })
    }

    fn restore_destination(&self, version: &Version, output_path: &Path) -> StorageResult<PathBuf> {
        if output_path.extension().is_some() {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)?;
            }
            Ok(output_path.to_path_buf())
        } else {
            fs::create_dir_all(output_path)?;
            let source_name = Path::new(&version.original_path)
                .file_name()
                .ok_or_else(|| {
                    StorageError::InvalidFileName(PathBuf::from(&version.original_path))
                })?;
            Ok(output_path.join(source_name))
        }
    }

    fn export_resolved_version(
        &self,
        document: &Document,
        version: &Version,
        output_path: &Path,
    ) -> StorageResult<PathBuf> {
        let destination = self.restore_destination(version, output_path)?;
        match BackupBackend::parse(&version.backup_backend)? {
            BackupBackend::LocalCopy => {
                fs::copy(&version.archive_path, &destination)?;
            }
            BackupBackend::Restic => {
                self.restore_restic_version(document, version, &destination)?;
            }
        }
        Ok(destination)
    }

    fn restore_restic_version(
        &self,
        document: &Document,
        version: &Version,
        destination: &Path,
    ) -> StorageResult<()> {
        let snapshot_id = version
            .snapshot_id
            .as_deref()
            .ok_or(StorageError::ResticSnapshotMissing)?;
        let restore_root = self
            .paths
            .staging_dir
            .join("restore")
            .join(document.id.as_str())
            .join(&version.id);
        reset_dir(&restore_root)?;
        self.restic_restore(snapshot_id, &restore_root)?;

        let restored_package = restore_root.join("package");
        docvault_ooxml::pack_package(restored_package, destination)?;
        Ok(())
    }

    fn restic_package_dir(&self, document: &Document, version_id: &str) -> PathBuf {
        self.paths
            .staging_dir
            .join("backup")
            .join(document.id.as_str())
            .join(version_id)
            .join("package")
    }

    fn ensure_restic_repo(&self) -> StorageResult<()> {
        let config = self.run_restic(["cat", "config"])?;
        if config.status.success() {
            return Ok(());
        }

        let init = self.run_restic(["init"])?;
        if init.status.success() {
            Ok(())
        } else {
            Err(restic_failed("init", init.stderr))
        }
    }

    fn restic_backup(
        &self,
        document: &Document,
        version_id: &str,
        package_dir: &Path,
    ) -> StorageResult<String> {
        let parent = package_dir
            .parent()
            .ok_or_else(|| StorageError::InvalidFileName(package_dir.to_path_buf()))?;
        let tag = format!("docvault:{}:{version_id}", document.id.as_str());
        let output = self.run_restic_in_dir(
            [
                "backup",
                "--json",
                "--tag",
                tag.as_str(),
                "--host",
                "docvault",
                "package",
            ],
            parent,
        )?;
        if !output.status.success() {
            return Err(restic_failed("backup", output.stderr));
        }
        snapshot_id_from_backup_json(&output.stdout)
    }

    fn restic_restore(&self, snapshot_id: &str, target: &Path) -> StorageResult<()> {
        fs::create_dir_all(target)?;
        let target = target.display().to_string();
        let output = self.run_restic(["restore", snapshot_id, "--target", target.as_str()])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(restic_failed("restore", output.stderr))
        }
    }

    fn run_restic<const N: usize>(&self, args: [&str; N]) -> StorageResult<std::process::Output> {
        self.run_restic_command(args, None)
    }

    fn run_restic_in_dir<const N: usize>(
        &self,
        args: [&str; N],
        current_dir: &Path,
    ) -> StorageResult<std::process::Output> {
        self.run_restic_command(args, Some(current_dir))
    }

    fn run_restic_command<const N: usize>(
        &self,
        args: [&str; N],
        current_dir: Option<&Path>,
    ) -> StorageResult<std::process::Output> {
        let mut command = Command::new(&self.settings.restic_path);
        command
            .args(["-r", self.paths.repo_dir.to_string_lossy().as_ref()])
            .args(args)
            .env("RESTIC_PASSWORD", &self.settings.restic_password)
            .env("RESTIC_CACHE_DIR", &self.paths.cache_dir);
        if let Some(current_dir) = current_dir {
            command.current_dir(current_dir);
        }
        Ok(command.output()?)
    }
}

#[derive(Debug, Clone)]
struct ArchiveReference {
    backend: BackupBackend,
    reference: PathBuf,
    snapshot_id: Option<String>,
}

fn version_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Version> {
    Ok(Version {
        id: row.get(0)?,
        document_id: DocumentId::new(row.get::<_, String>(1)?),
        number: row.get(2)?,
        original_path: row.get(3)?,
        archive_path: row.get(4)?,
        backup_backend: row.get(5)?,
        snapshot_id: row.get(6)?,
        parent_version_id: row.get(7)?,
        author: row.get(8)?,
        note: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn document_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: DocumentId::new(row.get::<_, String>(0)?),
        name: row.get(1)?,
        source_path: row.get(2)?,
        current_version_id: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn tracked_path_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrackedPath> {
    Ok(TrackedPath {
        id: row.get(0)?,
        document_id: row.get::<_, Option<String>>(1)?.map(DocumentId::new),
        path: row.get(2)?,
        fingerprint: row.get(3)?,
        last_scanned_at: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn write_document_matches(
    f: &mut std::fmt::Formatter<'_>,
    matches: &[Document],
) -> std::fmt::Result {
    writeln!(f, "matches:")?;
    for document in matches {
        writeln!(
            f,
            "  {}  {}  {}",
            document.id.as_str(),
            document.name,
            document.source_path
        )?;
    }
    write!(f, "use --id <document_id> or name@<id-prefix>")
}

fn read_settings(paths: &VaultPaths) -> StorageResult<StorageSettings> {
    let config = fs::read_to_string(&paths.config_path).unwrap_or_default();
    let backend = env::var("DOCVAULT_BACKUP_BACKEND")
        .ok()
        .or_else(|| config_value(&config, "backend"))
        .unwrap_or_else(|| "restic".to_owned());
    let restic_path = env::var_os("DOCVAULT_RESTIC_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            config_value(&config, "restic_path")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| bundled_or_system_restic(paths));
    let restic_password = env::var("DOCVAULT_RESTIC_PASSWORD")
        .ok()
        .or_else(|| config_value(&config, "restic_password"))
        .unwrap_or_else(|| "docvault-local-development-password".to_owned());

    Ok(StorageSettings {
        backend: BackupBackend::parse(&backend)?,
        restic_path,
        restic_password,
    })
}

fn config_value(config: &str, key: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let line = line.trim();
        let (candidate, value) = line.split_once('=')?;
        if candidate.trim() != key {
            return None;
        }
        Some(value.trim().trim_matches('"').to_owned())
    })
}

fn bundled_or_system_restic(paths: &VaultPaths) -> PathBuf {
    let bundled = paths
        .root_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("third_party")
        .join("restic")
        .join("0.19.1")
        .join(target_triple())
        .join(restic_binary_name());
    if bundled.exists() {
        bundled
    } else {
        PathBuf::from(restic_binary_name())
    }
}

fn absolute_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map(|current_dir| current_dir.join(&path))
            .unwrap_or(path)
    }
}

fn target_triple() -> &'static str {
    if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

fn restic_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "restic.exe"
    } else {
        "restic"
    }
}

fn reset_dir(path: &Path) -> StorageResult<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}

fn restic_failed(command: &str, stderr: Vec<u8>) -> StorageError {
    StorageError::ResticFailed {
        command: command.to_owned(),
        stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
    }
}

fn snapshot_id_from_backup_json(stdout: &[u8]) -> StorageResult<String> {
    let output = String::from_utf8_lossy(stdout);
    for line in output.lines() {
        let value: Value = serde_json::from_str(line)?;
        if value.get("message_type").and_then(Value::as_str) == Some("summary")
            && let Some(snapshot_id) = value.get("snapshot_id").and_then(Value::as_str)
        {
            return Ok(snapshot_id.to_owned());
        }
    }
    Err(StorageError::ResticSnapshotMissing)
}

fn file_fingerprint(path: &Path) -> StorageResult<String> {
    let metadata = fs::metadata(path)?;
    let mut hasher = Fnv1a64::new();
    hasher.write_u64(metadata.len());

    let mut file = fs::File::open(path)?;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.write(&buffer[..bytes_read]);
    }
    Ok(format!("{:016x}", hasher.finish()))
}

#[derive(Debug, Clone)]
struct Fnv1a64(u64);

impl Fnv1a64 {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }
}

impl Hasher for Fnv1a64 {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

fn default_config(paths: &VaultPaths) -> String {
    format!(
        "[storage]\nbackend = \"restic\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\nrestic_path = \"\"\nrestic_password = \"docvault-local-development-password\"\n\n[database]\npath = \"{}\"\n\n[logging]\nlevel = \"info\"\n",
        paths.data_dir.display(),
        paths.repo_dir.display(),
        paths.db_path.display()
    )
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_paths(name: &str) -> VaultPaths {
        let root = env::temp_dir().join(format!("docvault-{name}-{}", unix_timestamp()));
        VaultPaths::new(root.clone(), root.join("data"), root.join("db.sqlite"))
    }

    #[test]
    fn stores_explicit_restic_path() {
        let config = ResticConfig::new(".docvault/repo").with_restic_path("tools/restic.exe");

        assert_eq!(config.restic_path, Some(PathBuf::from("tools/restic.exe")));
    }

    #[test]
    fn imports_lists_and_restores_versions_with_local_copy() {
        let paths = unique_test_paths("storage");
        fs::create_dir_all(&paths.root_dir).unwrap();
        fs::write(
            &paths.config_path,
            format!(
                "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
                paths.data_dir.display(),
                paths.repo_dir.display(),
                paths.db_path.display()
            ),
        )
        .unwrap();
        let source_dir = paths.root_dir.join("sources");
        fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("report.docx");
        fs::write(&source, b"version one").unwrap();

        let storage = VaultStorage::init(paths.clone()).unwrap();
        let (_, version) = storage
            .add_document_version(
                DocumentRef::Name("report".to_owned()),
                &source,
                ImportMetadata {
                    author: Some("Bryan".to_owned()),
                    note: Some("Initial import".to_owned()),
                },
            )
            .unwrap();

        assert_eq!(storage.backend(), BackupBackend::LocalCopy);
        assert_eq!(version.id, "v1");
        assert_eq!(version.backup_backend, "local-copy");
        assert_eq!(version.author.as_deref(), Some("Bryan"));
        assert_eq!(version.note.as_deref(), Some("Initial import"));
        assert_eq!(storage.list_documents().unwrap()[0].name, "report");
        let versions = storage
            .list_versions(&DocumentRef::Name("report".to_owned()))
            .unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].author.as_deref(), Some("Bryan"));

        let restored = storage
            .restore_version(
                &DocumentRef::Name("report".to_owned()),
                "latest",
                &paths.root_dir.join("restored"),
            )
            .unwrap();
        assert_eq!(fs::read(restored).unwrap(), b"version one");
    }

    #[test]
    fn scans_tracked_paths_and_detects_changes() {
        let paths = unique_test_paths("track");
        fs::create_dir_all(&paths.root_dir).unwrap();
        fs::write(
            &paths.config_path,
            format!(
                "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\n\n[database]\npath = \"{}\"\n",
                paths.data_dir.display(),
                paths.repo_dir.display(),
                paths.db_path.display()
            ),
        )
        .unwrap();
        let source = paths.root_dir.join("tracked.docx");
        fs::write(&source, b"version one").unwrap();

        let storage = VaultStorage::init(paths).unwrap();
        let tracked_path = storage.track_path(&source, None).unwrap();

        assert_eq!(tracked_path.document_id, None);
        let first_scan = storage.scan_tracked_paths().unwrap();
        assert_eq!(first_scan.len(), 1);
        assert!(first_scan[0].changed);
        assert!(first_scan[0].exists);
        assert!(first_scan[0].fingerprint.is_some());

        let second_scan = storage.scan_tracked_paths().unwrap();
        assert!(!second_scan[0].changed);

        fs::write(&source, b"version two").unwrap();
        let third_scan = storage.scan_tracked_paths().unwrap();
        assert!(third_scan[0].changed);
    }

    #[test]
    fn extracts_snapshot_id_from_restic_json_summary() {
        let output = br#"{"message_type":"status","percent_done":0}
{"message_type":"summary","snapshot_id":"abc123"}
"#;

        assert_eq!(snapshot_id_from_backup_json(output).unwrap(), "abc123");
    }
}
