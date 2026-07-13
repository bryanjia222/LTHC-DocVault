use docvault_types::{Document, DocumentId, Version};
use rusqlite::{OptionalExtension, params};

use crate::{DocumentRef, StorageError, StorageResult, VaultStorage};

impl VaultStorage {
    pub(crate) fn migrate(&self) -> StorageResult<()> {
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
                manifest_json TEXT NOT NULL DEFAULT '{\"entries\":[]}',
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

    pub(crate) fn insert_document(&self, document: &Document) -> StorageResult<()> {
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

    pub(crate) fn insert_version(&self, version: &Version) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO versions (
                id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, manifest_json, parent_version_id, author, note, created_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                version.id,
                version.document_id.as_str(),
                version.number,
                version.original_filename,
                version.archive_reference,
                version.backup_backend,
                version.snapshot_id,
                serde_json::to_string(&version.manifest)?,
                version.parent_version_id,
                version.author,
                version.note,
                version.created_at
            ],
        )?;
        Ok(())
    }

    pub(crate) fn list_all_documents(&self) -> StorageResult<Vec<Document>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, current_version_id, created_at FROM documents ORDER BY created_at, name, id",
        )?;
        let documents = statement
            .query_map([], document_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(documents)
    }

    pub(crate) fn find_documents_by_name(&self, name: &str) -> StorageResult<Vec<Document>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, current_version_id, created_at FROM documents WHERE name = ?1 ORDER BY created_at, id",
        )?;
        let documents = statement
            .query_map([name], document_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(documents)
    }

    pub(crate) fn document_name_by_id(&self, id: &str) -> StorageResult<String> {
        self.connection
            .query_row("SELECT name FROM documents WHERE id = ?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .ok_or_else(|| StorageError::DocumentIdNotFound(id.to_owned()))
    }

    pub(crate) fn resolve_document_ref(
        &self,
        document_ref: &DocumentRef,
    ) -> StorageResult<Document> {
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

    pub(crate) fn set_current_version(
        &self,
        document_id: &str,
        version_id: &str,
    ) -> StorageResult<()> {
        self.connection.execute(
            "UPDATE documents SET current_version_id = ?1 WHERE id = ?2",
            params![version_id, document_id],
        )?;
        Ok(())
    }

    pub(crate) fn next_version_number(&self, document_id: &str) -> StorageResult<i64> {
        let current = self.connection.query_row(
            "SELECT COALESCE(MAX(number), 0) FROM versions WHERE document_id = ?1",
            [document_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(current + 1)
    }

    pub(crate) fn versions_for_document(&self, document_id: &str) -> StorageResult<Vec<Version>> {
        let mut statement = self.connection.prepare(
            "SELECT id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, manifest_json, parent_version_id, author, note, created_at
             FROM versions WHERE document_id = ?1 ORDER BY number",
        )?;
        let versions = statement
            .query_map([document_id], version_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(versions)
    }

    pub(crate) fn find_version(
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
                "SELECT id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, manifest_json, parent_version_id, author, note, created_at
                 FROM versions WHERE document_id = ?1 AND id = ?2",
                params![document_id, version_id],
                version_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }
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
        manifest: serde_json::from_str(&row.get::<_, String>(7)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        parent_version_id: row.get(8)?,
        author: row.get(9)?,
        note: row.get(10)?,
        created_at: row.get(11)?,
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
