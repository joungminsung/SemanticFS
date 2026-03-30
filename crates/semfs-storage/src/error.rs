use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Duplicate file path: {0}")]
    DuplicatePath(String),

    #[error("WAL recovery failed: {0}")]
    WalRecovery(String),

    #[error("Vector store error: {0}")]
    VectorStore(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;
