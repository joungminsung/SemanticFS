use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Storage error: {0}")]
    Storage(#[from] semfs_storage::StorageError),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Query parse error: {0}")]
    QueryParse(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("VFS error: {0}")]
    Vfs(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File not supported: {0}")]
    UnsupportedFile(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
