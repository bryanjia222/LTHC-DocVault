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
use rusqlite::Connection;
use tracing::{debug, info};
use uuid::Uuid;

pub use config::ResticConfig;
pub(crate) use config::StorageSettings;
pub use error::{DatabaseError, ResticError, StorageError, StorageResult};
pub use paths::VaultPaths;

/// A cancellation flag that is never set. Used for restic calls that run
/// outside a job (vault init/open), where there is no job to cancel.
pub static NEVER_CANCELLED: AtomicBool = AtomicBool::new(false);

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
        debug!(root_dir = %paths.root_dir.display(), "opening vault storage");
        let settings = config::read_settings(&paths)?;
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
        Ok(storage)
    }

    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub fn backend(&self) -> BackupBackend {
        self.settings.backend
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
        let manifest = docvault_ooxml::package_manifest(source_path)?;
        let archive = self.archive_source(&document, &version_id, source_path, cancel)?;
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

    fn config_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }
}
