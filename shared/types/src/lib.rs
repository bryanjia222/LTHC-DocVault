#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub source_path: String,
    pub current_version_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub id: String,
    pub document_id: DocumentId,
    pub number: i64,
    pub original_path: String,
    pub archive_path: String,
    pub backup_backend: String,
    pub snapshot_id: Option<String>,
    pub parent_version_id: Option<String>,
    pub author: Option<String>,
    pub note: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitMetadata {
    pub author: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedPath {
    pub id: String,
    pub document_id: Option<DocumentId>,
    pub path: String,
    pub stat_fingerprint: Option<String>,
    pub content_fingerprint: Option<String>,
    pub last_scanned_at: Option<i64>,
    pub last_deep_scanned_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedScan {
    pub tracked_path: TrackedPath,
    pub stat_fingerprint: Option<String>,
    pub content_fingerprint: Option<String>,
    pub status: String,
    pub changed: bool,
    pub exists: bool,
    pub deep: bool,
    pub scanned_at: i64,
}
