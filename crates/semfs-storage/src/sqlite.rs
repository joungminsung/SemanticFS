use crate::error::{Result, StorageError};
use crate::types::*;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                extension TEXT,
                size INTEGER NOT NULL,
                hash TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                modified_at INTEGER NOT NULL,
                indexed_at INTEGER NOT NULL,
                mime_type TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
            CREATE INDEX IF NOT EXISTS idx_files_hash ON files(hash);
            CREATE INDEX IF NOT EXISTS idx_files_extension ON files(extension);
            CREATE INDEX IF NOT EXISTS idx_files_modified ON files(modified_at);

            CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
                name, path, content,
                content_rowid='id'
            );

            CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL,
                parent_chunk_id INTEGER REFERENCES chunks(id),
                content TEXT NOT NULL,
                chunk_type TEXT NOT NULL,
                start_line INTEGER,
                end_line INTEGER
            );

            CREATE INDEX IF NOT EXISTS idx_chunks_file ON chunks(file_id);

            CREATE TABLE IF NOT EXISTS acl_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                permission TEXT NOT NULL,
                mount_point TEXT NOT NULL
            );
        ")?;
        info!("SQLite schema initialized");
        Ok(())
    }

    // -- File operations --

    pub fn insert_file(&self, meta: &FileMeta) -> Result<FileId> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO files (path, name, extension, size, hash, created_at, modified_at, indexed_at, mime_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                meta.path.to_string_lossy().to_string(),
                meta.name,
                meta.extension,
                meta.size as i64,
                meta.hash,
                meta.created_at,
                meta.modified_at,
                meta.indexed_at,
                meta.mime_type,
            ],
        )?;
        let id = conn.last_insert_rowid();
        debug!(file_id = id, path = %meta.path.display(), "Inserted file");
        Ok(id)
    }

    pub fn update_file(&self, id: FileId, meta: &FileMeta) -> Result<()> {
        let conn = self.conn.lock();
        let rows = conn.execute(
            "UPDATE files SET path=?1, name=?2, extension=?3, size=?4, hash=?5,
             created_at=?6, modified_at=?7, indexed_at=?8, mime_type=?9
             WHERE id=?10",
            params![
                meta.path.to_string_lossy().to_string(),
                meta.name,
                meta.extension,
                meta.size as i64,
                meta.hash,
                meta.created_at,
                meta.modified_at,
                meta.indexed_at,
                meta.mime_type,
                id,
            ],
        )?;
        if rows == 0 {
            return Err(StorageError::FileNotFound(format!("file id: {}", id)));
        }
        Ok(())
    }

    pub fn delete_file(&self, id: FileId) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM files WHERE id=?1", params![id])?;
        // FTS cleanup
        conn.execute("DELETE FROM files_fts WHERE rowid=?1", params![id])?;
        Ok(())
    }

    pub fn get_file(&self, id: FileId) -> Result<FileMeta> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, path, name, extension, size, hash, created_at, modified_at, indexed_at, mime_type
             FROM files WHERE id=?1",
            params![id],
            |row| {
                Ok(FileMeta {
                    id: Some(row.get(0)?),
                    path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                    name: row.get(2)?,
                    extension: row.get(3)?,
                    size: row.get::<_, i64>(4)? as u64,
                    hash: row.get(5)?,
                    created_at: row.get(6)?,
                    modified_at: row.get(7)?,
                    indexed_at: row.get(8)?,
                    mime_type: row.get(9)?,
                })
            },
        ).map_err(|_| StorageError::FileNotFound(format!("file id: {}", id)))
    }

    pub fn get_file_by_path(&self, path: &Path) -> Result<FileMeta> {
        let conn = self.conn.lock();
        let path_str = path.to_string_lossy().to_string();
        conn.query_row(
            "SELECT id, path, name, extension, size, hash, created_at, modified_at, indexed_at, mime_type
             FROM files WHERE path=?1",
            params![path_str],
            |row| {
                Ok(FileMeta {
                    id: Some(row.get(0)?),
                    path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                    name: row.get(2)?,
                    extension: row.get(3)?,
                    size: row.get::<_, i64>(4)? as u64,
                    hash: row.get(5)?,
                    created_at: row.get(6)?,
                    modified_at: row.get(7)?,
                    indexed_at: row.get(8)?,
                    mime_type: row.get(9)?,
                })
            },
        ).map_err(|_| StorageError::FileNotFound(path_str))
    }

    pub fn get_file_hash(&self, path: &Path) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let path_str = path.to_string_lossy().to_string();
        let result = conn.query_row(
            "SELECT hash FROM files WHERE path=?1",
            params![path_str],
            |row| row.get(0),
        );
        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Sqlite(e)),
        }
    }

    pub fn list_all_files(&self) -> Result<Vec<FileMeta>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, path, name, extension, size, hash, created_at, modified_at, indexed_at, mime_type FROM files"
        )?;
        let files = stmt.query_map([], |row| {
            Ok(FileMeta {
                id: Some(row.get(0)?),
                path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                name: row.get(2)?,
                extension: row.get(3)?,
                size: row.get::<_, i64>(4)? as u64,
                hash: row.get(5)?,
                created_at: row.get(6)?,
                modified_at: row.get(7)?,
                indexed_at: row.get(8)?,
                mime_type: row.get(9)?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(files)
    }

    // -- FTS operations --

    pub fn index_content(&self, file_id: FileId, name: &str, path: &str, content: &str) -> Result<()> {
        let conn = self.conn.lock();
        // Delete existing FTS entry
        conn.execute("DELETE FROM files_fts WHERE rowid=?1", params![file_id])?;
        // Insert new FTS entry
        conn.execute(
            "INSERT INTO files_fts (rowid, name, path, content) VALUES (?1, ?2, ?3, ?4)",
            params![file_id, name, path, content],
        )?;
        Ok(())
    }

    pub fn search_fts(&self, query: &str) -> Result<Vec<(FileId, f64)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT rowid, rank FROM files_fts WHERE files_fts MATCH ?1 ORDER BY rank LIMIT 100"
        )?;
        let results = stmt.query_map(params![query], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
        })?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(results)
    }

    // -- Filter operations --

    pub fn filter_by(&self, filters: &[MetadataFilter]) -> Result<Vec<FileId>> {
        let conn = self.conn.lock();
        let mut conditions = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        for filter in filters {
            match filter {
                MetadataFilter::DateRange { start, end } => {
                    conditions.push(format!("modified_at >= ?{} AND modified_at <= ?{}",
                        param_values.len() + 1, param_values.len() + 2));
                    param_values.push(Box::new(*start));
                    param_values.push(Box::new(*end));
                }
                MetadataFilter::Extension(exts) => {
                    let placeholders: Vec<String> = exts.iter().enumerate()
                        .map(|(i, _)| format!("?{}", param_values.len() + i + 1))
                        .collect();
                    conditions.push(format!("extension IN ({})", placeholders.join(",")));
                    for ext in exts {
                        param_values.push(Box::new(ext.clone()));
                    }
                }
                MetadataFilter::Size { min, max } => {
                    if let Some(min_val) = min {
                        conditions.push(format!("size >= ?{}", param_values.len() + 1));
                        param_values.push(Box::new(*min_val as i64));
                    }
                    if let Some(max_val) = max {
                        conditions.push(format!("size <= ?{}", param_values.len() + 1));
                        param_values.push(Box::new(*max_val as i64));
                    }
                }
                MetadataFilter::MimeType(types) => {
                    let placeholders: Vec<String> = types.iter().enumerate()
                        .map(|(i, _)| format!("?{}", param_values.len() + i + 1))
                        .collect();
                    conditions.push(format!("mime_type IN ({})", placeholders.join(",")));
                    for t in types {
                        param_values.push(Box::new(t.clone()));
                    }
                }
                MetadataFilter::PathPrefix(prefix) => {
                    conditions.push(format!("path LIKE ?{}", param_values.len() + 1));
                    param_values.push(Box::new(format!("{}%", prefix)));
                }
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!("SELECT id FROM files {}", where_clause);
        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let ids = stmt.query_map(params_refs.as_slice(), |row| {
            row.get::<_, i64>(0)
        })?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    // -- Chunk operations --

    pub fn insert_chunk(&self, chunk: &Chunk) -> Result<ChunkId> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO chunks (file_id, chunk_index, parent_chunk_id, content, chunk_type, start_line, end_line)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                chunk.file_id,
                chunk.chunk_index as i64,
                chunk.parent_chunk_id,
                chunk.content,
                chunk.chunk_type.as_str(),
                chunk.start_line.map(|l| l as i64),
                chunk.end_line.map(|l| l as i64),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_chunks_for_file(&self, file_id: FileId) -> Result<Vec<Chunk>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, file_id, chunk_index, parent_chunk_id, content, chunk_type, start_line, end_line
             FROM chunks WHERE file_id=?1 ORDER BY chunk_index"
        )?;
        let chunks = stmt.query_map(params![file_id], |row| {
            Ok(Chunk {
                id: Some(row.get(0)?),
                file_id: row.get(1)?,
                chunk_index: row.get::<_, i64>(2)? as usize,
                parent_chunk_id: row.get(3)?,
                content: row.get(4)?,
                chunk_type: ChunkType::from_str(&row.get::<_, String>(5)?),
                start_line: row.get::<_, Option<i64>>(6)?.map(|l| l as usize),
                end_line: row.get::<_, Option<i64>>(7)?.map(|l| l as usize),
                metadata: std::collections::HashMap::new(),
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(chunks)
    }

    pub fn delete_chunks_for_file(&self, file_id: FileId) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM chunks WHERE file_id=?1", params![file_id])?;
        Ok(())
    }

    // -- ACL operations --

    pub fn insert_acl_rule(&self, rule: &AclRule) -> Result<i64> {
        let conn = self.conn.lock();
        let perm = match &rule.permission {
            Permission::Read => "read",
            Permission::Write => "write",
            Permission::Deny => "deny",
        };
        conn.execute(
            "INSERT INTO acl_rules (pattern, permission, mount_point) VALUES (?1, ?2, ?3)",
            params![rule.pattern, perm, rule.mount_point],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_acl_rules(&self, mount_point: &str) -> Result<Vec<AclRule>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, pattern, permission, mount_point FROM acl_rules WHERE mount_point=?1"
        )?;
        let rules = stmt.query_map(params![mount_point], |row| {
            let perm_str: String = row.get(2)?;
            let permission = match perm_str.as_str() {
                "read" => Permission::Read,
                "write" => Permission::Write,
                "deny" => Permission::Deny,
                _ => Permission::Read,
            };
            Ok(AclRule {
                id: Some(row.get(0)?),
                pattern: row.get(1)?,
                permission,
                mount_point: row.get(3)?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rules)
    }

    pub fn file_count(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_meta() -> FileMeta {
        FileMeta {
            id: None,
            path: PathBuf::from("/test/file.rs"),
            name: "file.rs".to_string(),
            extension: Some("rs".to_string()),
            size: 1024,
            hash: "abc123".to_string(),
            created_at: 1000,
            modified_at: 2000,
            indexed_at: 3000,
            mime_type: Some("text/x-rust".to_string()),
        }
    }

    #[test]
    fn test_insert_and_get() {
        let store = SqliteStore::in_memory().unwrap();
        let meta = test_meta();
        let id = store.insert_file(&meta).unwrap();
        let retrieved = store.get_file(id).unwrap();
        assert_eq!(retrieved.name, "file.rs");
        assert_eq!(retrieved.size, 1024);
    }

    #[test]
    fn test_fts_search() {
        let store = SqliteStore::in_memory().unwrap();
        let meta = test_meta();
        let id = store.insert_file(&meta).unwrap();
        store.index_content(id, "file.rs", "/test/file.rs", "fn main() { println!(\"hello\"); }").unwrap();
        let results = store.search_fts("main").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, id);
    }

    #[test]
    fn test_filter_by_extension() {
        let store = SqliteStore::in_memory().unwrap();
        let meta = test_meta();
        store.insert_file(&meta).unwrap();

        let results = store.filter_by(&[MetadataFilter::Extension(vec!["rs".to_string()])]).unwrap();
        assert_eq!(results.len(), 1);

        let results = store.filter_by(&[MetadataFilter::Extension(vec!["py".to_string()])]).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_delete_file() {
        let store = SqliteStore::in_memory().unwrap();
        let meta = test_meta();
        let id = store.insert_file(&meta).unwrap();
        store.delete_file(id).unwrap();
        assert!(store.get_file(id).is_err());
    }
}
