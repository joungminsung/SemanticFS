use thiserror::Error;

#[derive(Error, Debug)]
pub enum FuseError {
    #[error("Mount error: {0}")]
    Mount(String),

    #[error("Unmount error: {0}")]
    Unmount(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Core error: {0}")]
    Core(#[from] semfs_core::CoreError),

    #[error("Not supported on this platform")]
    UnsupportedPlatform,
}

pub type Result<T> = std::result::Result<T, FuseError>;
