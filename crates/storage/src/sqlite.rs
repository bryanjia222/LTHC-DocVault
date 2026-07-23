use docvault_types::{Document, DocumentId, Version};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

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
                archive_status TEXT NOT NULL DEFAULT 'archived',
                PRIMARY KEY (document_id, id),
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );
            ",
        )?;
        // Existing vaults created before the async commit path have a versions
        // table without `archive_status`; backfill it (idempotent). New vaults
        // get the column from the CREATE TABLE above, so this is a no-op for
        // them. Every pre-existing version was archived synchronously, so the
        // default 'archived' is correct for backfilled rows.
        self.ensure_archive_status_column()?;
        Ok(())
    }

    /// Add `archive_status` to a pre-async versions table if it is missing.
    /// Guarded by `PRAGMA table_info` so it never re-runs on an already-
    /// migrated vault (and never errors on a fresh one).
    fn ensure_archive_status_column(&self) -> StorageResult<()> {
        let mut statement = self.connection.prepare("PRAGMA table_info(versions)")?;
        let has_column = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .any(|column| column.is_ok_and(|name| name == "archive_status"));
        if !has_column {
            self.connection.execute(
                "ALTER TABLE versions ADD COLUMN archive_status TEXT NOT NULL DEFAULT 'archived'",
                [],
            )?;
        }
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
        insert_version_into(&self.connection, version)
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

    /// Delete a document's row and all of its version rows. Callers orchestrate
    /// restic forget + local archive cleanup around this so the DB and the
    /// backend stay consistent.
    pub(crate) fn remove_document(&self, document_id: &str) -> StorageResult<()> {
        self.connection
            .execute("DELETE FROM versions WHERE document_id = ?1", [document_id])?;
        self.connection
            .execute("DELETE FROM documents WHERE id = ?1", [document_id])?;
        Ok(())
    }

    /// Delete specific version rows of one document by id, leaving the document
    /// and its other versions intact. Callers orchestrate restic forget + local
    /// archive cleanup around this. Empty `version_ids` is a no-op. Ids that do
    /// not exist simply match nothing - the caller is expected to have validated
    /// them first (and surface `VersionNotFound` if not).
    pub(crate) fn remove_versions(
        &self,
        document_id: &str,
        version_ids: &[String],
    ) -> StorageResult<()> {
        if version_ids.is_empty() {
            return Ok(());
        }
        let placeholders = std::iter::repeat("?")
            .take(version_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "DELETE FROM versions WHERE document_id = ? AND id IN ({placeholders})"
        );
        // document_id first, then each version id, as the bound params.
        let mut params: Vec<String> = Vec::with_capacity(version_ids.len() + 1);
        params.push(document_id.to_owned());
        params.extend(version_ids.iter().cloned());
        self.connection
            .execute(&sql, params_from_iter(params.iter().map(|s| s.as_str())))?;
        Ok(())
    }

    /// Update a document's display name. Versions (and their `original_filename`)
    /// are historical and untouched; archives are keyed by document id, so their
    /// references are unaffected by a rename.
    pub(crate) fn set_document_name(&self, document_id: &str, new_name: &str) -> StorageResult<()> {
        self.connection.execute(
            "UPDATE documents SET name = ?1 WHERE id = ?2",
            params![new_name, document_id],
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
            "SELECT id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, manifest_json, parent_version_id, author, note, created_at, archive_status
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
                "SELECT id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, manifest_json, parent_version_id, author, note, created_at, archive_status
                 FROM versions WHERE document_id = ?1 AND id = ?2",
                params![document_id, version_id],
                version_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Every version row still in the `pending` archive state, ordered so the
    /// oldest is recovered first. Used by startup recovery to finish the
    /// archive that a crash interrupted (the intake copy is durable, so the
    /// data is safe - recovery just completes the compress step idempotently).
    pub(crate) fn pending_versions(&self) -> StorageResult<Vec<Version>> {
        let mut statement = self.connection.prepare(
            "SELECT id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, manifest_json, parent_version_id, author, note, created_at, archive_status
             FROM versions WHERE archive_status = 'pending' ORDER BY created_at, document_id, id",
        )?;
        let versions = statement
            .query_map([], version_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(versions)
    }

    /// Flip a pending version to `archived` and record its real archive
    /// reference + restic snapshot id (both `None`/placeholder until the
    /// archive job completes). The async commit path inserts the row with a
    /// placeholder `archive_reference` and `archive_status = 'pending'`; this
    /// finalizes it once the archive is durable.
    pub(crate) fn set_version_archived(
        &self,
        document_id: &str,
        version_id: &str,
        archive_reference: &str,
        snapshot_id: Option<&str>,
    ) -> StorageResult<()> {
        self.connection.execute(
            "UPDATE versions SET archive_reference = ?1, snapshot_id = ?2, archive_status = 'archived'
             WHERE document_id = ?3 AND id = ?4",
            params![archive_reference, snapshot_id, document_id, version_id],
        )?;
        Ok(())
    }

    /// Update a version's `note` (its commit message). `None` clears it (sets
    /// the column to NULL). Callers verify the version exists first (via
    /// [`find_version`]) so a missing version surfaces as
    /// [`StorageError::VersionNotFound`] instead of a silent no-op; this helper
    /// is a plain `UPDATE` and affects zero rows for a missing version.
    pub(crate) fn update_version_note(
        &self,
        document_id: &str,
        version_id: &str,
        note: Option<&str>,
    ) -> StorageResult<()> {
        self.connection.execute(
            "UPDATE versions SET note = ?1 WHERE document_id = ?2 AND id = ?3",
            params![note, document_id, version_id],
        )?;
        Ok(())
    }

    /// `true` when the version row exists and is still `pending` (archive not
    /// finalized). Used by [`VaultStorage::gc_intake`] to decide whether an
    /// intake copy is still in flight (keep) or an orphan (reclaim). A missing
    /// row (document/version deleted) is `false`, so orphaned intake is swept.
    pub(crate) fn is_version_pending(
        &self,
        document_id: &str,
        version_id: &str,
    ) -> StorageResult<bool> {
        let status: Option<String> = self
            .connection
            .query_row(
                "SELECT archive_status FROM versions WHERE document_id = ?1 AND id = ?2",
                params![document_id, version_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(status.is_some_and(|value| value == "pending"))
    }
}

/// Insert a version row on a specific connection (the commit transaction
/// reuses this so the version insert + current-pointer update are atomic).
pub(crate) fn insert_version_into(conn: &Connection, version: &Version) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO versions (
            id, document_id, number, original_filename, archive_reference, backup_backend, snapshot_id, manifest_json, parent_version_id, author, note, created_at, archive_status
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
            version.created_at,
            version.archive_status
        ],
    )?;
    Ok(())
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
        archive_status: row.get(12)?,
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
