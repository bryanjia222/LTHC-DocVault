use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use docvault_types::{Document, DocumentId, Version};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    DocumentNotFound(String),
    VersionNotFound {
        document_name: String,
        version: String,
    },
    InvalidFileName(PathBuf),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Sqlite(error) => write!(f, "SQLite error: {error}"),
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
        }
    }
}

impl std::error::Error for StorageError {}

impl From<io::Error> for StorageError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    pub root_dir: PathBuf,
    pub data_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub versions_dir: PathBuf,
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
        let root_dir = root_dir.into();
        let data_dir = data_dir.into();
        Self {
            staging_dir: data_dir.join("staging"),
            versions_dir: data_dir.join("versions"),
            repo_dir: root_dir.join("repo"),
            config_path: root_dir.join("config.toml"),
            root_dir,
            data_dir,
            db_path: db_path.into(),
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
    connection: Connection,
}

impl VaultStorage {
    pub fn init(paths: VaultPaths) -> StorageResult<Self> {
        fs::create_dir_all(&paths.root_dir)?;
        fs::create_dir_all(&paths.data_dir)?;
        fs::create_dir_all(&paths.staging_dir)?;
        fs::create_dir_all(&paths.versions_dir)?;
        fs::create_dir_all(&paths.repo_dir)?;
        if let Some(parent) = paths.db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if !paths.config_path.exists() {
            fs::write(&paths.config_path, default_config(&paths))?;
        }

        let connection = Connection::open(&paths.db_path)?;
        let storage = Self { paths, connection };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn open(paths: VaultPaths) -> StorageResult<Self> {
        let connection = Connection::open(&paths.db_path)?;
        let storage = Self { paths, connection };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub fn add_document_version(
        &self,
        name: &str,
        source_path: &Path,
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
        let archive_path = self.archive_source(&document, &version_id, source_path)?;
        let version = Version {
            id: version_id,
            document_id: document.id.clone(),
            number,
            original_path: source_path.display().to_string(),
            archive_path: archive_path.display().to_string(),
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

        let destination = if output_path.extension().is_some() {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)?;
            }
            output_path.to_path_buf()
        } else {
            fs::create_dir_all(output_path)?;
            let source_name = Path::new(&version.original_path)
                .file_name()
                .ok_or_else(|| {
                    StorageError::InvalidFileName(PathBuf::from(&version.original_path))
                })?;
            output_path.join(source_name)
        };

        fs::copy(&version.archive_path, &destination)?;
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
            "INSERT INTO versions (id, document_id, number, original_path, archive_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                version.id,
                version.document_id.as_str(),
                version.number,
                version.original_path,
                version.archive_path,
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
            "SELECT id, document_id, number, original_path, archive_path, created_at
             FROM versions WHERE document_id = ?1 ORDER BY number",
        )?;
        let versions = statement
            .query_map([document_id], |row| {
                Ok(Version {
                    id: row.get(0)?,
                    document_id: DocumentId::new(row.get::<_, String>(1)?),
                    number: row.get(2)?,
                    original_path: row.get(3)?,
                    archive_path: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
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
                "SELECT id, document_id, number, original_path, archive_path, created_at
                 FROM versions WHERE document_id = ?1 AND id = ?2",
                params![document_id, version_id],
                |row| {
                    Ok(Version {
                        id: row.get(0)?,
                        document_id: DocumentId::new(row.get::<_, String>(1)?),
                        number: row.get(2)?,
                        original_path: row.get(3)?,
                        archive_path: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn archive_source(
        &self,
        document: &Document,
        version_id: &str,
        source_path: &Path,
    ) -> StorageResult<PathBuf> {
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
        Ok(archive_path)
    }
}

fn default_config(paths: &VaultPaths) -> String {
    format!(
        "[storage]\nbackend = \"local-copy\"\ndata_dir = \"{}\"\nrepo_dir = \"{}\"\nrestic_path = \"\"\n\n[database]\npath = \"{}\"\n\n[logging]\nlevel = \"info\"\n",
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
    fn imports_lists_and_restores_versions() {
        let paths = unique_test_paths("storage");
        let source_dir = paths.root_dir.join("sources");
        fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("report.docx");
        fs::write(&source, b"version one").unwrap();

        let storage = VaultStorage::init(paths.clone()).unwrap();
        let (_, version) = storage.add_document_version("report", &source).unwrap();

        assert_eq!(version.id, "v1");
        assert_eq!(storage.list_documents().unwrap()[0].name, "report");
        assert_eq!(storage.list_versions("report").unwrap().len(), 1);

        let restored = storage
            .restore_version("report", "latest", &paths.root_dir.join("restored"))
            .unwrap();
        assert_eq!(fs::read(restored).unwrap(), b"version one");
    }
}
