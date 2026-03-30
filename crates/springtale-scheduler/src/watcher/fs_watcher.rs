use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::EventKind;
use notify::RecommendedWatcher;
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use tokio::sync::mpsc;

use crate::error::SchedulerError;
use springtale_core::rule::engine::TriggerEvent;

/// Default debounce interval for filesystem events.
const DEFAULT_DEBOUNCE_MS: u64 = 500;

/// Filesystem watcher that emits debounced `TriggerEvent` for file changes.
///
/// Uses `notify-debouncer-full` (v0.7) which wraps `notify` v8 with
/// time-based event debouncing. Events within the debounce window are
/// coalesced. The full `EventKind` (Create/Modify/Remove) is preserved.
pub struct FsWatcher {
    /// The debouncer wraps the watcher and provides watch/unwatch.
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    /// Paths being watched.
    watched_paths: Vec<PathBuf>,
}

impl FsWatcher {
    /// Create a new filesystem watcher with default debounce (500ms).
    pub fn new(trigger_tx: mpsc::Sender<TriggerEvent>) -> Result<Self, SchedulerError> {
        Self::with_debounce(trigger_tx, Duration::from_millis(DEFAULT_DEBOUNCE_MS))
    }

    /// Create a new filesystem watcher with a custom debounce interval.
    pub fn with_debounce(
        trigger_tx: mpsc::Sender<TriggerEvent>,
        debounce: Duration,
    ) -> Result<Self, SchedulerError> {
        let debouncer = new_debouncer(
            debounce,
            None, // tick_rate: use default
            move |result: DebounceEventResult| {
                match result {
                    Ok(events) => {
                        for debounced in events {
                            let event_type = match debounced.event.kind {
                                EventKind::Create(_) => "create",
                                EventKind::Modify(_) => "modify",
                                EventKind::Remove(_) => "delete",
                                _ => continue, // ignore access, other events
                            };

                            for path in &debounced.event.paths {
                                let trigger = TriggerEvent {
                                    trigger_type: "FileWatch".to_owned(),
                                    connector: None,
                                    event: Some(event_type.to_owned()),
                                    payload: serde_json::json!({
                                        "path": path.to_string_lossy(),
                                        "event": event_type,
                                        "filename": path.file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default(),
                                        "extension": path.extension()
                                            .map(|e| e.to_string_lossy().into_owned())
                                            .unwrap_or_default(),
                                    }),
                                };

                                // try_send: don't block the notify callback thread
                                let _ = trigger_tx.try_send(trigger);
                            }
                        }
                    }
                    Err(errors) => {
                        for e in errors {
                            tracing::warn!(error = %e, "filesystem watcher error");
                        }
                    }
                }
            },
        )
        .map_err(|e| SchedulerError::Watcher(e.to_string()))?;

        Ok(Self {
            debouncer,
            watched_paths: Vec::new(),
        })
    }

    /// Watch a path for changes (recursive).
    pub fn watch(&mut self, path: impl AsRef<Path>) -> Result<(), SchedulerError> {
        let path = path.as_ref();
        self.debouncer
            .watch(path, notify::RecursiveMode::Recursive)
            .map_err(|e| SchedulerError::Watcher(format!("{}: {e}", path.display())))?;

        self.watched_paths.push(path.to_owned());
        tracing::info!(path = %path.display(), "watching filesystem path");
        Ok(())
    }

    /// Stop watching a path.
    pub fn unwatch(&mut self, path: impl AsRef<Path>) -> Result<(), SchedulerError> {
        let path = path.as_ref();
        self.debouncer
            .unwatch(path)
            .map_err(|e| SchedulerError::Watcher(format!("{}: {e}", path.display())))?;

        self.watched_paths.retain(|p| p != path);
        tracing::info!(path = %path.display(), "stopped watching filesystem path");
        Ok(())
    }

    /// List currently watched paths.
    pub fn watched_paths(&self) -> &[PathBuf] {
        &self.watched_paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_watch_directory() {
        let dir = std::env::temp_dir().join("springtale_fs_debounce_test");
        fs::create_dir_all(&dir).ok();

        let (tx, mut rx) = mpsc::channel(100);
        // Use short debounce for testing
        let mut watcher = FsWatcher::with_debounce(tx, Duration::from_millis(100)).unwrap();
        watcher.watch(&dir).unwrap();

        assert_eq!(watcher.watched_paths().len(), 1);

        // Create a file — should trigger a debounced event
        let test_file = dir.join("test_file.txt");
        fs::write(&test_file, "hello").unwrap();

        // Wait for debounced event (debounce + processing time)
        let result = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;

        // Clean up
        fs::remove_file(&test_file).ok();
        fs::remove_dir(&dir).ok();

        assert!(result.is_ok(), "timed out waiting for debounced fs event");
        let event = result.ok().flatten();
        assert!(event.is_some());
        assert_eq!(
            event.as_ref().map(|e| e.trigger_type.as_str()),
            Some("FileWatch")
        );
    }

    #[tokio::test]
    async fn test_unwatch() {
        let dir = std::env::temp_dir().join("springtale_fs_unwatch_debounce_test");
        fs::create_dir_all(&dir).ok();

        let (tx, _rx) = mpsc::channel(100);
        let mut watcher = FsWatcher::new(tx).unwrap();
        watcher.watch(&dir).unwrap();
        assert_eq!(watcher.watched_paths().len(), 1);

        watcher.unwatch(&dir).unwrap();
        assert!(watcher.watched_paths().is_empty());

        fs::remove_dir(&dir).ok();
    }

    #[tokio::test]
    async fn test_default_debounce() {
        let (tx, _rx) = mpsc::channel(100);
        let watcher = FsWatcher::new(tx);
        assert!(watcher.is_ok(), "default debounce creation failed");
    }
}
