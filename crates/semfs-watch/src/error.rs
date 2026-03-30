use thiserror::Error;

#[derive(Error, Debug)]
pub enum WatchError {
    #[error("Watcher error: {0}")]
    Notify(#[from] notify::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Channel send error")]
    ChannelSend,

    #[error("Watcher already running")]
    AlreadyRunning,

    #[error("Watcher not running")]
    NotRunning,
}

pub type Result<T> = std::result::Result<T, WatchError>;
