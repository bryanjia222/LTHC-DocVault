use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use directories::ProjectDirs;
use docvault_ooxml::OoxmlError;
use docvault_types::{CommitMetadata, Document, DocumentId, VaultConfig, Version};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, error, info};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("OOXML error: {0}")]
    Ooxml(#[from] OoxmlError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML decode error: {0}")]
    TomlDecode(#[from] toml::de::Error),
    #[error("TOML encode error: {0}")]
    TomlEncode(#[from] toml::ser::Error),
    #[error("document not found: {0}")]
    DocumentNotFound(String),
    #[error("document id not found: {0}")]
    DocumentIdNotFound(String),
    #[error(
        "document name is ambiguous: {name}\n{}",
        format_document_matches(matches)
    )]
    AmbiguousDocumentName {
        name: String,
        matches: Vec<Document>,
    },
    #[error(
        "document id prefix is ambiguous: {prefix}\n{}",
        format_document_matches(matches)
    )]
    AmbiguousDocumentIdPrefix {
        prefix: String,
        matches: Vec<Document>,
    },
    #[error("document reference mismatch: requested name {requested_name}, matched {} ({})", matched.name, matched.id.as_str())]
    DocumentReferenceMismatch {
        requested_name: String,
        matched: Box<Document>,
    },
    #[error("version {version} not found for document {document_name}")]
    VersionNotFound {
        document_name: String,
        version: String,
    },
    #[error("invalid file name: {}", .0.display())]
    InvalidFileName(PathBuf),
    #[error("invalid backup backend: {0}")]
    InvalidBackend(String),
    #[error(transparent)]
    Restic(#[from] ResticError),
}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        DatabaseError::from(value).into()
    }
}

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Error)]
pub enum ResticError {
    #[error("restic command failed ({command}): {stderr}")]
    Failed { command: String, stderr: String },
    #[error("restic backup did not return a snapshot id")]
    SnapshotMissing,
}

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
            .unwrap_or_else(default_root_dir);
        let config_path = absolute_path(root_dir.join("config.toml"));
        let config = read_config_file(&config_path).ok();
        let data_dir = env::var_os("DOCVAULT_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                config
                    .as_ref()
                    .map(|config| PathBuf::from(&config.storage.data_dir))
            })
            .unwrap_or_else(|| root_dir.join("data"));
        let db_path = env::var_os("DOCVAULT_DB_PATH")
            .map(PathBuf::from)
            .or_else(|| {
                config
                    .as_ref()
                    .map(|config| PathBuf::from(&config.database.path))
            })
            .unwrap_or_else(|| root_dir.join("db.sqlite"));
        let repo_dir = config
            .as_ref()
            .map(|config| PathBuf::from(&config.storage.repo_dir))
            .unwrap_or_else(|| root_dir.join("repo"));

        Self::new_with_repo(root_dir, data_dir, repo_dir, db_path)
    }

    pub fn new(
        root_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
        db_path: impl Into<PathBuf>,
    ) -> Self {
        let root_dir = root_dir.into();
        let repo_dir = root_dir.join("repo");
        Self::new_with_repo(root_dir, data_dir, repo_dir, db_path)
    }

    pub fn new_with_repo(
        root_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
        repo_dir: impl Into<PathBuf>,
        db_path: impl Into<PathBuf>,
    ) -> Self {
        let root_dir = absolute_path(root_dir.into());
        let data_dir = absolute_path(data_dir.into());
        let repo_dir = absolute_path(repo_dir.into());
        let db_path = absolute_path(db_path.into());
        Self {
            staging_dir: data_dir.join("staging"),
            versions_dir: data_dir.join("versions"),
            cache_dir: root_dir.join("cache"),
            repo_dir,
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
            fs::write(&paths.config_path, default_config(&paths)?)?;
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
        info!(root_dir = %storage.paths.root_dir.display(), "vault storage initialized");
        Ok(storage)
    }

    pub fn open(paths: VaultPaths) -> StorageResult<Self> {
        debug!(root_dir = %paths.root_dir.display(), "opening vault storage");
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
        metadata: CommitMetadata,
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

        self.add_version_to_document(document, source_path, metadata, now)
    }

    pub fn add_document_version_to_name_or_create(
        &self,
        name: &str,
        source_path: &Path,
        metadata: CommitMetadata,
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
        info!(
            document_id = document.id.as_str(),
            version_id = version.id.as_str(),
            output = %output_path.display(),
            "exporting document version"
        );
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
        info!(
            document_id = document.id.as_str(),
            version_id = version.id.as_str(),
            "checking out document version"
        );
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

    fn add_version_to_document(
        &self,
        document: Document,
        source_path: &Path,
        metadata: CommitMetadata,
        now: i64,
    ) -> StorageResult<(Document, Version)> {
        let number = self.next_version_number(document.id.as_str())?;
        let version_id = format!("v{number}");
        debug!(
            document_id = document.id.as_str(),
            version_id = version_id.as_str(),
            "archiving source for document version"
        );
        let archive = self.archive_source(&document, &version_id, source_path)?;
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
            parent_version_id: document.current_version_id.clone(),
            author: metadata.author,
            note: metadata.note,
            created_at: now,
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
        let mut statement = self.connection.prepare(
            "SELECT id, name, current_version_id, created_at FROM documents ORDER BY created_at, name, id",
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
                current_version_id TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS versions (
                id TEXT NOT NULL,
                document_id TEXT NOT NULL,
                number INTEGER NOT NULL,
                original_filename TEXT NOT NULL,
                archive_reference TEXT NOT NULL,
                backup_backend TEXT NOT NULL DEFAULT 'local-copy',
                snapshot_id TEXT,
                parent_version_id TEXT,
                author TEXT,
                note TEXT,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (document_id, id),
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );
            ",
        )?;
        Ok(())
    }

    fn insert_document(&self, document: &Document) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO documents (id, name, current_version_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                document.id.as_str(),
                document.name,
                document.current_version_id,
                document.created_at
            ],
        )?;
        Ok(())
    }

    fn insert_version(&self, version: &Version) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO versions (
                id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, parent_version_id, author, note, created_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                version.id,
                version.document_id.as_str(),
                version.number,
                version.original_filename,
                version.archive_reference,
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
            "SELECT id, name, current_version_id, created_at FROM documents WHERE name = ?1 ORDER BY created_at, id",
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
            "SELECT id, name, current_version_id, created_at FROM documents WHERE id LIKE ?1 ORDER BY created_at, id",
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
            "SELECT id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, parent_version_id, author, note, created_at
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
                "SELECT id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, parent_version_id, author, note, created_at
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
        debug!(
            document_id = document.id.as_str(),
            version_id,
            source = %source_path.display(),
            "archiving source with local copy backend"
        );
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
        let archive_reference = PathBuf::from(document.id.as_str())
            .join(version_id)
            .join(source_name)
            .display()
            .to_string();
        Ok(ArchiveReference {
            backend: BackupBackend::LocalCopy,
            reference: archive_reference,
            snapshot_id: None,
        })
    }

    fn archive_restic(
        &self,
        document: &Document,
        version_id: &str,
        source_path: &Path,
    ) -> StorageResult<ArchiveReference> {
        debug!(
            document_id = document.id.as_str(),
            version_id,
            source = %source_path.display(),
            "archiving source with restic backend"
        );
        self.ensure_restic_repo()?;
        let package_dir = self.restic_package_dir(document, version_id);
        reset_dir(&package_dir)?;
        docvault_ooxml::unpack_package(source_path, &package_dir)?;

        let snapshot_id = self.restic_backup(document, version_id, &package_dir)?;
        Ok(ArchiveReference {
            backend: BackupBackend::Restic,
            reference: format!("restic:{}:{version_id}", document.id.as_str()),
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
            Ok(output_path.join(&version.original_filename))
        }
    }

    fn export_resolved_version(
        &self,
        document: &Document,
        version: &Version,
        output_path: &Path,
    ) -> StorageResult<PathBuf> {
        let destination = self.restore_destination(version, output_path)?;
        info!(
            document_id = document.id.as_str(),
            version_id = version.id.as_str(),
            backend = version.backup_backend.as_str(),
            destination = %destination.display(),
            "restoring archived version"
        );
        match BackupBackend::parse(&version.backup_backend)? {
            BackupBackend::LocalCopy => {
                fs::copy(
                    self.paths.versions_dir.join(&version.archive_reference),
                    &destination,
                )?;
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
            .ok_or(ResticError::SnapshotMissing)?;
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
        debug!(repo = %self.paths.repo_dir.display(), "checking restic repository");
        let config = self.run_restic(["cat", "config"])?;
        if config.status.success() {
            return Ok(());
        }

        info!(repo = %self.paths.repo_dir.display(), "initializing restic repository");
        let init = self.run_restic(["init"])?;
        if init.status.success() {
            Ok(())
        } else {
            let error = restic_failed("init", init.stderr);
            error!(repo = %self.paths.repo_dir.display(), %error, "failed to initialize restic repository");
            Err(error)
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
            let error = restic_failed("backup", output.stderr);
            error!(
                document_id = document.id.as_str(),
                version_id,
                %error,
                "restic backup failed"
            );
            return Err(error);
        }
        let snapshot_id = snapshot_id_from_backup_json(&output.stdout)?;
        info!(
            document_id = document.id.as_str(),
            version_id,
            snapshot_id = snapshot_id.as_str(),
            "restic backup completed"
        );
        Ok(snapshot_id)
    }

    fn restic_restore(&self, snapshot_id: &str, target: &Path) -> StorageResult<()> {
        fs::create_dir_all(target)?;
        let target = target.display().to_string();
        let output = self.run_restic(["restore", snapshot_id, "--target", target.as_str()])?;
        if output.status.success() {
            info!(snapshot_id, target, "restic restore completed");
            Ok(())
        } else {
            let error = restic_failed("restore", output.stderr);
            error!(snapshot_id, target, %error, "restic restore failed");
            Err(error)
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
        debug!(
            restic = %self.settings.restic_path.display(),
            repo = %self.paths.repo_dir.display(),
            args = ?args,
            current_dir = ?current_dir,
            "running restic command"
        );
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
    reference: String,
    snapshot_id: Option<String>,
}

fn version_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Version> {
    Ok(Version {
        id: row.get(0)?,
        document_id: DocumentId::new(row.get::<_, String>(1)?),
        number: row.get(2)?,
        original_filename: row.get(3)?,
        archive_reference: row.get(4)?,
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
        current_version_id: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn format_document_matches(matches: &[Document]) -> String {
    let mut output = String::from("matches:\n");
    for document in matches {
        output.push_str(&format!("  {}  {}\n", document.id.as_str(), document.name));
    }
    output.push_str("use --id <document_id> or name@<id-prefix>");
    output
}

fn read_settings(paths: &VaultPaths) -> StorageResult<StorageSettings> {
    let config = read_config(paths)?;
    let backend = env::var("DOCVAULT_BACKUP_BACKEND")
        .ok()
        .unwrap_or_else(|| config.storage.backend.clone());
    let restic_path = env::var_os("DOCVAULT_RESTIC_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            config
                .storage
                .restic_path
                .clone()
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| bundled_or_system_restic(paths));
    let restic_password = env::var("DOCVAULT_RESTIC_PASSWORD")
        .ok()
        .unwrap_or_else(|| config.storage.restic_password.clone());

    Ok(StorageSettings {
        backend: BackupBackend::parse(&backend)?,
        restic_path,
        restic_password,
    })
}

fn read_config(paths: &VaultPaths) -> StorageResult<VaultConfig> {
    if paths.config_path.exists() {
        read_config_file(&paths.config_path)
    } else {
        Ok(VaultConfig::for_paths(
            paths.data_dir.clone(),
            paths.repo_dir.clone(),
            paths.db_path.clone(),
        ))
    }
}

fn read_config_file(path: &Path) -> StorageResult<VaultConfig> {
    let config = fs::read_to_string(path)?;
    Ok(toml::from_str(&config)?)
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

fn default_root_dir() -> PathBuf {
    ProjectDirs::from("com", "LTHC", "DocVault")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".docvault"))
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
    ResticError::Failed {
        command: command.to_owned(),
        stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
    }
    .into()
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
    Err(ResticError::SnapshotMissing.into())
}

fn default_config(paths: &VaultPaths) -> StorageResult<String> {
    Ok(toml::to_string_pretty(&VaultConfig::for_paths(
        paths.data_dir.clone(),
        paths.repo_dir.clone(),
        paths.db_path.clone(),
    ))?)
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
    fn commits_lists_and_restores_versions_with_local_copy() {
        let paths = unique_test_paths("storage");
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
        let source_dir = paths.root_dir.join("sources");
        fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("report.docx");
        fs::write(&source, b"version one").unwrap();

        let storage = VaultStorage::init(paths.clone()).unwrap();
        let (_, version) = storage
            .add_document_version(
                DocumentRef::Name("report".to_owned()),
                &source,
                CommitMetadata {
                    author: Some("Bryan".to_owned()),
                    note: Some("Initial commit".to_owned()),
                },
            )
            .unwrap();

        assert_eq!(storage.backend(), BackupBackend::LocalCopy);
        assert_eq!(version.id, "v1");
        assert_eq!(version.backup_backend, "local-copy");
        assert_eq!(version.original_filename, "report.docx");
        assert!(!Path::new(&version.archive_reference).is_absolute());
        assert_eq!(version.author.as_deref(), Some("Bryan"));
        assert_eq!(version.note.as_deref(), Some("Initial commit"));
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
    fn extracts_snapshot_id_from_restic_json_summary() {
        let output = br#"{"message_type":"status","percent_done":0}
{"message_type":"summary","snapshot_id":"abc123"}
"#;

        assert_eq!(snapshot_id_from_backup_json(output).unwrap(), "abc123");
    }

    fn config_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }
}
