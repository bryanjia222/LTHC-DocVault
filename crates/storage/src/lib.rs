use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use docvault_ooxml::OoxmlError;
use docvault_types::{Document, DocumentId, ImportMetadata, Version};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Ooxml(OoxmlError),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    DocumentNotFound(String),
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
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Ooxml(error) => write!(f, "OOXML error: {error}"),
            Self::Sqlite(error) => write!(f, "SQLite error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::DocumentNotFound(name) => write!(f, "document not found: {name}"),
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
        name: &str,
        source_path: &Path,
        metadata: ImportMetadata,
    ) -> StorageResult<(Document, Version)> {
        let now = unix_timestamp();
        let document = match self.find_document_by_name(name)? {
            Some(document) => document,
            None => {
                let document = Document {
                    id: DocumentId::new(slugify(name)),
                    name: name.to_owned(),
                    source_path: source_path.display().to_string(),
                    created_at: now,
                };
                self.insert_document(&document)?;
                document
            }
        };

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
            author: metadata.author,
            note: metadata.note,
            created_at: now,
        };
        self.insert_version(&version)?;
        Ok((document, version))
    }

    pub fn list_documents(&self) -> StorageResult<Vec<Document>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, source_path, created_at FROM documents ORDER BY created_at, name",
        )?;
        let documents = statement
            .query_map([], |row| {
                Ok(Document {
                    id: DocumentId::new(row.get::<_, String>(0)?),
                    name: row.get(1)?,
                    source_path: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(documents)
    }

    pub fn list_versions(&self, document_name: &str) -> StorageResult<Vec<Version>> {
        let document = self
            .find_document_by_name(document_name)?
            .ok_or_else(|| StorageError::DocumentNotFound(document_name.to_owned()))?;
        self.versions_for_document(document.id.as_str())
    }

    pub fn restore_version(
        &self,
        document_name: &str,
        requested_version: &str,
        output_path: &Path,
    ) -> StorageResult<PathBuf> {
        let document = self
            .find_document_by_name(document_name)?
            .ok_or_else(|| StorageError::DocumentNotFound(document_name.to_owned()))?;
        let version = self
            .find_version(document.id.as_str(), requested_version)?
            .ok_or_else(|| StorageError::VersionNotFound {
                document_name: document_name.to_owned(),
                version: requested_version.to_owned(),
            })?;
        let destination = self.restore_destination(&version, output_path)?;

        match BackupBackend::parse(&version.backup_backend)? {
            BackupBackend::LocalCopy => {
                fs::copy(&version.archive_path, &destination)?;
            }
            BackupBackend::Restic => {
                self.restore_restic_version(&document, &version, &destination)?;
            }
        }

        Ok(destination)
    }

    fn migrate(&self) -> StorageResult<()> {
        self.connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                source_path TEXT NOT NULL,
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
                author TEXT,
                note TEXT,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (document_id, id),
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );
            ",
        )?;
        add_column_if_missing(
            &self.connection,
            "versions",
            "backup_backend",
            "TEXT NOT NULL DEFAULT 'local-copy'",
        )?;
        add_column_if_missing(&self.connection, "versions", "snapshot_id", "TEXT")?;
        add_column_if_missing(&self.connection, "versions", "author", "TEXT")?;
        add_column_if_missing(&self.connection, "versions", "note", "TEXT")?;
        Ok(())
    }

    fn insert_document(&self, document: &Document) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO documents (id, name, source_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                document.id.as_str(),
                document.name,
                document.source_path,
                document.created_at
            ],
        )?;
        Ok(())
    }

    fn insert_version(&self, version: &Version) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO versions (
                id, document_id, number, original_path, archive_path, backup_backend, snapshot_id, author, note, created_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                version.id,
                version.document_id.as_str(),
                version.number,
                version.original_path,
                version.archive_path,
                version.backup_backend,
                version.snapshot_id,
                version.author,
                version.note,
                version.created_at
            ],
        )?;
        Ok(())
    }

    fn find_document_by_name(&self, name: &str) -> StorageResult<Option<Document>> {
        self.connection
            .query_row(
                "SELECT id, name, source_path, created_at FROM documents WHERE name = ?1",
                [name],
                |row| {
                    Ok(Document {
                        id: DocumentId::new(row.get::<_, String>(0)?),
                        name: row.get(1)?,
                        source_path: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
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
            "SELECT id, document_id, number, original_path, archive_path, backup_backend, snapshot_id, author, note, created_at
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
                "SELECT id, document_id, number, original_path, archive_path, backup_backend, snapshot_id, author, note, created_at
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
        author: row.get(7)?,
        note: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> StorageResult<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == column);

    if !exists {
        connection.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
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

fn default_config(paths: &VaultPaths) -> String {
    format!(
        "[storage]\nbackend = \"restic\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\nrestic_path = \"\"\nrestic_password = \"docvault-local-development-password\"\n\n[database]\npath = \"{}\"\n\n[logging]\nlevel = \"info\"\n",
        paths.data_dir.display(),
        paths.repo_dir.display(),
        paths.db_path.display()
    )
}

fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();

    if slug.is_empty() {
        "document".to_owned()
    } else {
        slug
    }
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
                "report",
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
        let versions = storage.list_versions("report").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].author.as_deref(), Some("Bryan"));

        let restored = storage
            .restore_version("report", "latest", &paths.root_dir.join("restored"))
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
}
