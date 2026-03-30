pub mod debounce;
pub mod error;
pub mod events;
pub mod watcher;

pub use error::{Result, WatchError};
pub use events::{EventBatch, FsEvent};
pub use watcher::FileSystemWatcher;
