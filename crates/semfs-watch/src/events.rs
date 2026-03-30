use std::path::PathBuf;

/// File system events that SemanticFS cares about
#[derive(Debug, Clone, PartialEq)]
pub enum FsEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

impl FsEvent {
    pub fn path(&self) -> &PathBuf {
        match self {
            FsEvent::Created(p) | FsEvent::Modified(p) | FsEvent::Deleted(p) => p,
            FsEvent::Renamed { to, .. } => to,
        }
    }

    pub fn is_modification(&self) -> bool {
        matches!(self, FsEvent::Created(_) | FsEvent::Modified(_))
    }
}

/// A batch of debounced events
#[derive(Debug, Clone)]
pub struct EventBatch {
    pub events: Vec<FsEvent>,
    pub timestamp: std::time::Instant,
}

impl EventBatch {
    pub fn new(events: Vec<FsEvent>) -> Self {
        Self {
            events,
            timestamp: std::time::Instant::now(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}
