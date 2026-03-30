use crate::error::Result;
use crate::types::*;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

pub struct WalStore {
    conn: Arc<Mutex<Connection>>,
}

impl WalStore {
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
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = FULL;

            CREATE TABLE IF NOT EXISTS wal_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                operation TEXT NOT NULL,
                source_path TEXT NOT NULL,
                dest_path TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at INTEGER NOT NULL,
                completed_at INTEGER
            );

            CREATE INDEX IF NOT EXISTS idx_wal_status ON wal_entries(status);
        ",
        )?;
        info!("WAL store initialized");
        Ok(())
    }

    pub fn log_operation(&self, op: &FileOperation) -> Result<WalEntryId> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        let (op_type, source, dest) = match op {
            FileOperation::Move { source, dest } => (
                "move",
                source.to_string_lossy().to_string(),
                Some(dest.to_string_lossy().to_string()),
            ),
            FileOperation::Copy { source, dest } => (
                "copy",
                source.to_string_lossy().to_string(),
                Some(dest.to_string_lossy().to_string()),
            ),
            FileOperation::Delete { path } => ("delete", path.to_string_lossy().to_string(), None),
            FileOperation::Write { path, .. } => {
                ("write", path.to_string_lossy().to_string(), None)
            }
        };
        conn.execute(
            "INSERT INTO wal_entries (operation, source_path, dest_path, status, created_at)
             VALUES (?1, ?2, ?3, 'pending', ?4)",
            params![op_type, source, dest, now],
        )?;
        let id = conn.last_insert_rowid();
        info!(wal_id = id, operation = op_type, "WAL entry logged");
        Ok(id)
    }

    pub fn mark_executing(&self, id: WalEntryId) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE wal_entries SET status='executing' WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn mark_completed(&self, id: WalEntryId) -> Result<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE wal_entries SET status='completed', completed_at=?1 WHERE id=?2",
            params![now, id],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, id: WalEntryId) -> Result<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE wal_entries SET status='failed', completed_at=?1 WHERE id=?2",
            params![now, id],
        )?;
        Ok(())
    }

    pub fn recover_pending(&self) -> Result<Vec<WalEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, operation, source_path, dest_path, status, created_at, completed_at
             FROM wal_entries WHERE status IN ('pending', 'executing')
             ORDER BY id ASC",
        )?;
        let entries = stmt
            .query_map([], |row| {
                let op_type: String = row.get(1)?;
                let source: String = row.get(2)?;
                let dest: Option<String> = row.get(3)?;
                let status_str: String = row.get(4)?;

                // Note: Write WAL entries are advisory-only and cannot be replayed.
                // The file data is not persisted to the WAL table, so recovery
                // produces an empty `data` vec. This is by design -- the WAL
                // protects move/copy/delete atomicity, but write-data recovery
                // requires the application to re-write.
                let operation = match op_type.as_str() {
                    "move" => FileOperation::Move {
                        source: source.into(),
                        dest: dest.unwrap_or_default().into(),
                    },
                    "copy" => FileOperation::Copy {
                        source: source.into(),
                        dest: dest.unwrap_or_default().into(),
                    },
                    "delete" => FileOperation::Delete {
                        path: source.into(),
                    },
                    "write" => FileOperation::Write {
                        path: source.into(),
                        data: Vec::new(),
                    },
                    _ => FileOperation::Delete {
                        path: source.into(),
                    },
                };

                Ok(WalEntry {
                    id: row.get(0)?,
                    operation,
                    status: OperationStatus::from_str_lossy(&status_str),
                    created_at: row.get(5)?,
                    completed_at: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if !entries.is_empty() {
            warn!(
                count = entries.len(),
                "Found pending WAL entries for recovery"
            );
        }
        Ok(entries)
    }

    pub fn cleanup_completed(&self, before_timestamp: i64) -> Result<usize> {
        let conn = self.conn.lock();
        let rows = conn.execute(
            "DELETE FROM wal_entries WHERE status='completed' AND completed_at < ?1",
            params![before_timestamp],
        )?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_wal_lifecycle() {
        let wal = WalStore::in_memory().unwrap();
        let op = FileOperation::Move {
            source: PathBuf::from("/a"),
            dest: PathBuf::from("/b"),
        };
        let id = wal.log_operation(&op).unwrap();

        let pending = wal.recover_pending().unwrap();
        assert_eq!(pending.len(), 1);

        wal.mark_executing(id).unwrap();
        wal.mark_completed(id).unwrap();

        let pending = wal.recover_pending().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_wal_recovery() {
        let wal = WalStore::in_memory().unwrap();
        let op1 = FileOperation::Delete {
            path: PathBuf::from("/x"),
        };
        let op2 = FileOperation::Copy {
            source: PathBuf::from("/a"),
            dest: PathBuf::from("/b"),
        };
        wal.log_operation(&op1).unwrap();
        let id2 = wal.log_operation(&op2).unwrap();
        wal.mark_executing(id2).unwrap();

        let pending = wal.recover_pending().unwrap();
        assert_eq!(pending.len(), 2);
    }
}
