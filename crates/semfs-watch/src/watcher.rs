use crate::debounce::EventDebouncer;
use crate::error::{Result, WatchError};
use crate::events::{EventBatch, FsEvent};
use crossbeam_channel::{Receiver, Sender, bounded};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Watches file system for changes and sends debounced event batches
pub struct FileSystemWatcher {
    event_tx: Sender<EventBatch>,
    event_rx: Receiver<EventBatch>,
    running: Arc<AtomicBool>,
    debounce_duration: Duration,
    watcher: Option<RecommendedWatcher>,
    ignored_patterns: Vec<String>,
}

impl FileSystemWatcher {
    pub fn new(debounce_secs: u64) -> Self {
        let (event_tx, event_rx) = bounded(1000);
        Self {
            event_tx,
            event_rx,
            running: Arc::new(AtomicBool::new(false)),
            debounce_duration: Duration::from_secs(debounce_secs),
            watcher: None,
            ignored_patterns: Vec::new(),
        }
    }

    pub fn with_ignored(mut self, patterns: Vec<String>) -> Self {
        self.ignored_patterns = patterns;
        self
    }

    /// Get the receiver for event batches
    pub fn receiver(&self) -> Receiver<EventBatch> {
        self.event_rx.clone()
    }

    /// Start watching a directory
    pub fn watch(&mut self, path: &Path) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(WatchError::AlreadyRunning);
        }

        if !path.exists() {
            return Err(WatchError::PathNotFound(path.display().to_string()));
        }

        let event_tx = self.event_tx.clone();
        let debounce_duration = self.debounce_duration;
        let ignored = self.ignored_patterns.clone();

        // Internal channel for raw notify events
        let (raw_tx, raw_rx) = bounded::<FsEvent>(10000);

        let mut watcher = notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    let fs_events = convert_notify_event(event);
                    for fs_event in fs_events {
                        if let Err(e) = raw_tx.try_send(fs_event) {
                            warn!("Failed to send event: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Watch error: {}", e);
                }
            }
        })?;

        watcher.watch(path, RecursiveMode::Recursive)?;
        self.watcher = Some(watcher);
        self.running.store(true, Ordering::SeqCst);

        // Spawn debouncer thread
        let running_clone = self.running.clone();
        std::thread::Builder::new()
            .name("semfs-watcher-debounce".to_string())
            .spawn(move || {
                let mut debouncer = EventDebouncer::new(debounce_duration);
                let tick = Duration::from_millis(500);

                while running_clone.load(Ordering::SeqCst) {
                    // Drain raw events
                    while let Ok(event) = raw_rx.try_recv() {
                        // Check ignore patterns
                        let path_str = event.path().to_string_lossy();
                        let should_ignore = ignored.iter().any(|p| path_str.contains(p));
                        if !should_ignore {
                            debouncer.add_event(event);
                        }
                    }

                    // Flush ready events
                    if let Some(batch) = debouncer.flush_ready() {
                        if let Err(e) = event_tx.try_send(batch) {
                            warn!("Failed to send event batch: {}", e);
                        }
                    }

                    std::thread::sleep(tick);
                }

                // Final flush
                if let Some(batch) = debouncer.flush_all() {
                    let _ = event_tx.try_send(batch);
                }

                debug!("Watcher debounce thread stopped");
            })
            .map_err(|e| WatchError::Notify(notify::Error::generic(&e.to_string())))?;

        info!(path = %path.display(), "Started watching directory");
        Ok(())
    }

    /// Stop watching
    pub fn stop(&mut self) -> Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(WatchError::NotRunning);
        }
        self.running.store(false, Ordering::SeqCst);
        self.watcher = None;
        info!("Stopped watching");
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

fn convert_notify_event(event: Event) -> Vec<FsEvent> {
    let mut fs_events = Vec::new();

    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                fs_events.push(FsEvent::Created(path));
            }
        }
        EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
            // Handle rename: notify provides two paths for rename events
            if event.paths.len() == 2 {
                fs_events.push(FsEvent::Renamed {
                    from: event.paths[0].clone(),
                    to: event.paths[1].clone(),
                });
            } else {
                for path in event.paths {
                    fs_events.push(FsEvent::Modified(path));
                }
            }
        }
        EventKind::Modify(_) => {
            for path in event.paths {
                fs_events.push(FsEvent::Modified(path));
            }
        }
        EventKind::Remove(_) => {
            for path in event.paths {
                fs_events.push(FsEvent::Deleted(path));
            }
        }
        _ => {}
    }

    fs_events
}

impl Drop for FileSystemWatcher {
    fn drop(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            let _ = self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_watcher_lifecycle() {
        let mut watcher = FileSystemWatcher::new(1);
        let dir = TempDir::new().unwrap();

        assert!(!watcher.is_running());
        watcher.watch(dir.path()).unwrap();
        assert!(watcher.is_running());
        watcher.stop().unwrap();
        assert!(!watcher.is_running());
    }

    #[test]
    fn test_watcher_invalid_path() {
        let mut watcher = FileSystemWatcher::new(1);
        let result = watcher.watch(Path::new("/nonexistent/path/12345"));
        assert!(result.is_err());
    }
}
