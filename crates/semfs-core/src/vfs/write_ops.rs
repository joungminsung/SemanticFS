use crate::error::{CoreError, Result};
use semfs_storage::{FileOperation, WalStore};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info};

/// Handles write operations with WAL protection
pub struct WriteHandler {
    wal: Arc<WalStore>,
    #[allow(dead_code)]
    source_root: PathBuf,
}

impl WriteHandler {
    pub fn new(wal: Arc<WalStore>, source_root: PathBuf) -> Self {
        Self { wal, source_root }
    }

    /// Move a file (rename)
    pub fn handle_rename(&self, from: &Path, to: &Path) -> Result<()> {
        let op = FileOperation::Move {
            source: from.to_path_buf(),
            dest: to.to_path_buf(),
        };
        let wal_id = self.wal.log_operation(&op)?;
        self.wal.mark_executing(wal_id)?;

        match std::fs::rename(from, to) {
            Ok(()) => {
                self.wal.mark_completed(wal_id)?;
                info!(from = %from.display(), to = %to.display(), "File moved");
                Ok(())
            }
            Err(e) => {
                self.wal.mark_failed(wal_id)?;
                error!(error = %e, "Move failed");
                Err(CoreError::Io(e))
            }
        }
    }

    /// Copy a file
    pub fn handle_copy(&self, from: &Path, to: &Path) -> Result<()> {
        let op = FileOperation::Copy {
            source: from.to_path_buf(),
            dest: to.to_path_buf(),
        };
        let wal_id = self.wal.log_operation(&op)?;
        self.wal.mark_executing(wal_id)?;

        match std::fs::copy(from, to) {
            Ok(_) => {
                self.wal.mark_completed(wal_id)?;
                info!(from = %from.display(), to = %to.display(), "File copied");
                Ok(())
            }
            Err(e) => {
                self.wal.mark_failed(wal_id)?;
                error!(error = %e, "Copy failed");
                Err(CoreError::Io(e))
            }
        }
    }

    /// Delete a file (soft delete -- moves to trash directory)
    pub fn handle_unlink(&self, path: &Path) -> Result<()> {
        let op = FileOperation::Delete {
            path: path.to_path_buf(),
        };
        let wal_id = self.wal.log_operation(&op)?;
        self.wal.mark_executing(wal_id)?;

        // Soft delete: move to .semanticfs/trash/
        let trash_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".semanticfs")
            .join("trash");
        std::fs::create_dir_all(&trash_dir)?;

        let trash_name = format!(
            "{}_{}",
            chrono::Utc::now().timestamp(),
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
        );
        let trash_path = trash_dir.join(trash_name);

        match std::fs::rename(path, &trash_path) {
            Ok(()) => {
                self.wal.mark_completed(wal_id)?;
                info!(path = %path.display(), trash = %trash_path.display(), "File soft-deleted");
                Ok(())
            }
            Err(_) => {
                // Rename may fail across filesystems, fall back to copy+delete
                match std::fs::copy(path, &trash_path).and_then(|_| std::fs::remove_file(path)) {
                    Ok(()) => {
                        self.wal.mark_completed(wal_id)?;
                        info!(path = %path.display(), trash = %trash_path.display(), "File soft-deleted (copy+delete)");
                        Ok(())
                    }
                    Err(e) => {
                        self.wal.mark_failed(wal_id)?;
                        error!(error = %e, "Delete failed");
                        Err(CoreError::Io(e))
                    }
                }
            }
        }
    }

    /// Write data to a file
    pub fn handle_write(&self, path: &Path, data: &[u8]) -> Result<()> {
        let op = FileOperation::Write {
            path: path.to_path_buf(),
            data: data.to_vec(),
        };
        let wal_id = self.wal.log_operation(&op)?;
        self.wal.mark_executing(wal_id)?;

        match std::fs::write(path, data) {
            Ok(()) => {
                self.wal.mark_completed(wal_id)?;
                info!(path = %path.display(), bytes = data.len(), "File written");
                Ok(())
            }
            Err(e) => {
                self.wal.mark_failed(wal_id)?;
                error!(error = %e, "Write failed");
                Err(CoreError::Io(e))
            }
        }
    }

    /// Recover from crash -- replay or rollback pending operations
    pub fn recover(&self) -> Result<()> {
        let pending = self.wal.recover_pending()?;
        if pending.is_empty() {
            return Ok(());
        }

        info!(count = pending.len(), "Recovering pending WAL operations");

        for entry in pending {
            match &entry.operation {
                FileOperation::Move { source, dest } => {
                    // If dest exists but source doesn't, operation completed
                    if dest.exists() && !source.exists() {
                        self.wal.mark_completed(entry.id)?;
                    } else if source.exists() {
                        // Retry the move
                        if let Err(e) = std::fs::rename(source, dest) {
                            error!(error = %e, "Recovery: move retry failed");
                            self.wal.mark_failed(entry.id)?;
                        } else {
                            self.wal.mark_completed(entry.id)?;
                        }
                    } else {
                        // Both gone, mark failed
                        self.wal.mark_failed(entry.id)?;
                    }
                }
                FileOperation::Copy { source, dest } => {
                    if dest.exists() {
                        self.wal.mark_completed(entry.id)?;
                    } else if source.exists() {
                        if let Err(e) = std::fs::copy(source, dest) {
                            error!(error = %e, "Recovery: copy retry failed");
                            self.wal.mark_failed(entry.id)?;
                        } else {
                            self.wal.mark_completed(entry.id)?;
                        }
                    } else {
                        self.wal.mark_failed(entry.id)?;
                    }
                }
                FileOperation::Delete { path } => {
                    if !path.exists() {
                        self.wal.mark_completed(entry.id)?;
                    } else {
                        self.wal.mark_failed(entry.id)?;
                    }
                }
                FileOperation::Write { path, data } => {
                    if data.is_empty() {
                        // Cannot recover write without data
                        self.wal.mark_failed(entry.id)?;
                    } else if let Err(_e) = std::fs::write(path, data) {
                        self.wal.mark_failed(entry.id)?;
                    } else {
                        self.wal.mark_completed(entry.id)?;
                    }
                }
            }
        }

        Ok(())
    }
}
