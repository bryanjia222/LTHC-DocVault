mod archive;
mod config;
mod error;
mod paths;
mod restic;
mod sqlite;

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use docvault_types::{CommitMetadata, Document, DocumentId, Version};
use rusqlite::Connection;
use tracing::{debug, info};
use uuid::Uuid;

pub use config::ResticConfig;
pub(crate) use config::StorageSettings;
pub use error::{DatabaseError, ResticError, StorageError, StorageResult};
pub use paths::VaultPaths;

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
    pub(crate) fn as_str(self) -> &'static str {
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
            fs::write(&paths.config_path, config::default_config(&paths)?)?;
        }

        let settings = config::read_settings(&paths)?;
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
        let settings = config::read_settings(&paths)?;
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

    pub(crate) fn add_version_to_document(
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
        self.list_all_documents()
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
}

pub(crate) fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
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

    #[test]
    fn commits_lists_and_restores_versions_with_local_copy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = temp_paths(temp_dir.path());
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

    fn config_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }
}
