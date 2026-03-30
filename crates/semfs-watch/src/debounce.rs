use crate::events::{EventBatch, FsEvent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Debounces file system events to avoid processing rapid successive changes
pub struct EventDebouncer {
    pending: HashMap<PathBuf, (FsEvent, Instant)>,
    debounce_duration: Duration,
}

impl EventDebouncer {
    pub fn new(debounce_duration: Duration) -> Self {
        Self {
            pending: HashMap::new(),
            debounce_duration,
        }
    }

    /// Add an event to the debouncer. Returns None if debouncing, Some(batch) if ready.
    pub fn add_event(&mut self, event: FsEvent) -> Option<EventBatch> {
        let path = event.path().clone();
        self.pending.insert(path, (event, Instant::now()));
        self.flush_ready()
    }

    /// Check for events that have been debounced long enough
    pub fn flush_ready(&mut self) -> Option<EventBatch> {
        let now = Instant::now();
        let ready: Vec<PathBuf> = self.pending.iter()
            .filter(|(_, (_, ts))| now.duration_since(*ts) >= self.debounce_duration)
            .map(|(path, _)| path.clone())
            .collect();

        if ready.is_empty() {
            return None;
        }

        let events: Vec<FsEvent> = ready.iter()
            .filter_map(|path| self.pending.remove(path))
            .map(|(event, _)| event)
            .collect();

        Some(EventBatch::new(events))
    }

    /// Force flush all pending events
    pub fn flush_all(&mut self) -> Option<EventBatch> {
        if self.pending.is_empty() {
            return None;
        }

        let events: Vec<FsEvent> = self.pending.drain()
            .map(|(_, (event, _))| event)
            .collect();

        Some(EventBatch::new(events))
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debounce_deduplicates() {
        // Use a longer debounce so events stay pending
        let mut debouncer = EventDebouncer::new(Duration::from_secs(10));

        // Same file modified twice — HashMap deduplicates by path
        debouncer.add_event(FsEvent::Modified(PathBuf::from("/a.txt")));
        debouncer.add_event(FsEvent::Modified(PathBuf::from("/a.txt")));

        // Should have exactly 1 pending (deduplicated by path)
        assert_eq!(debouncer.pending_count(), 1);

        let batch = debouncer.flush_all().unwrap();
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn test_debounce_holds_events() {
        let mut debouncer = EventDebouncer::new(Duration::from_secs(10));
        debouncer.add_event(FsEvent::Created(PathBuf::from("/a.txt")));

        // Should not be ready yet
        let batch = debouncer.flush_ready();
        assert!(batch.is_none());
        assert_eq!(debouncer.pending_count(), 1);
    }

    #[test]
    fn test_flush_all() {
        let mut debouncer = EventDebouncer::new(Duration::from_secs(10));
        debouncer.add_event(FsEvent::Created(PathBuf::from("/a.txt")));
        debouncer.add_event(FsEvent::Modified(PathBuf::from("/b.txt")));

        let batch = debouncer.flush_all().unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(debouncer.pending_count(), 0);
    }
}
