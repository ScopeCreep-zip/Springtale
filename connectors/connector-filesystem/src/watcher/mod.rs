use std::path::{Path, PathBuf};
use std::sync::Arc;

use notify::EventKind;
use notify::RecommendedWatcher;
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use tokio::sync::Mutex;

use crate::config::FilesystemConfig;
use crate::error::FilesystemError;
use springtale_connector::SubscriptionId;

/// Maps a notify `EventKind` to the trigger name used by the connector.
///
/// Returns `None` for events we don't care about (access, other).
fn event_kind_to_trigger(kind: &EventKind) -> Option<&'static str> {
    match kind {
        EventKind::Create(_) => Some("file_created"),
        EventKind::Modify(_) => Some("file_modified"),
        EventKind::Remove(_) => Some("file_deleted"),
        _ => None,
    }
}

/// Maps a notify `EventKind` to the short event string used in payloads.
fn event_kind_to_str(kind: &EventKind) -> Option<&'static str> {
    match kind {
        EventKind::Create(_) => Some("create"),
        EventKind::Modify(_) => Some("modify"),
        EventKind::Remove(_) => Some("delete"),
        _ => None,
    }
}

/// Callback type for trigger events. The connector registers these via `on_event`.
/// First argument is trigger name, second is payload.
pub type TriggerCallback = Box<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// Filesystem watcher that emits debounced events to registered trigger handlers.
///
/// Uses `notify-debouncer-full` (v0.7) wrapping `notify` v8 with time-based
/// event debouncing. Events within the debounce window are coalesced.
///
/// Path validation: the watcher only accepts paths that are within the
/// configured `watch_paths` allow-list. Symlinks are resolved before
/// comparison to prevent traversal attacks.
pub struct FsConnectorWatcher {
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    watched_paths: Vec<PathBuf>,
}

impl FsConnectorWatcher {
    /// Create a new filesystem watcher with the given config and trigger callbacks.
    ///
    /// The `callbacks` are shared between the watcher thread and the connector.
    /// When a filesystem event fires, each registered callback is invoked with
    /// the trigger name and event payload.
    pub fn new(
        config: &FilesystemConfig,
        callbacks: Arc<Mutex<Vec<(SubscriptionId, String, TriggerCallback)>>>,
    ) -> Result<Self, FilesystemError> {
        let debouncer = new_debouncer(
            config.debounce_duration(),
            None, // tick_rate: use default
            move |result: DebounceEventResult| {
                match result {
                    Ok(events) => {
                        for debounced in events {
                            let trigger_name = match event_kind_to_trigger(&debounced.event.kind) {
                                Some(name) => name,
                                None => continue,
                            };
                            let event_str = match event_kind_to_str(&debounced.event.kind) {
                                Some(s) => s,
                                None => continue,
                            };

                            for path in &debounced.event.paths {
                                let payload = serde_json::json!({
                                    "path": path.to_string_lossy(),
                                    "event": event_str,
                                    "filename": path.file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_default(),
                                    "extension": path.extension()
                                        .map(|e| e.to_string_lossy().into_owned())
                                        .unwrap_or_default(),
                                });

                                // Dispatch to all handlers registered for this trigger.
                                // Use try_lock to avoid blocking the notify callback thread.
                                if let Ok(handlers) = callbacks.try_lock() {
                                    for (_id, registered_trigger, handler) in handlers.iter() {
                                        if registered_trigger == trigger_name {
                                            handler(trigger_name, payload.clone());
                                        }
                                    }
                                } else {
                                    tracing::warn!(
                                        "could not acquire handler lock in watcher callback"
                                    );
                                }
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
        .map_err(|e| FilesystemError::WatcherFailed(e.to_string()))?;

        Ok(Self {
            debouncer,
            watched_paths: Vec::new(),
        })
    }

    /// Start watching a path for filesystem events (recursive).
    ///
    /// The path must be within the configured `watch_paths` allow-list.
    /// Symlinks are resolved before comparison.
    pub fn watch(&mut self, path: &Path, config: &FilesystemConfig) -> Result<(), FilesystemError> {
        if !config.is_watch_allowed(path) {
            return Err(FilesystemError::PathNotAllowed(path.display().to_string()));
        }

        self.debouncer
            .watch(path, notify::RecursiveMode::Recursive)
            .map_err(|e| FilesystemError::WatcherFailed(format!("{}: {e}", path.display())))?;

        self.watched_paths.push(path.to_owned());
        tracing::info!(path = %path.display(), "watching filesystem path");
        Ok(())
    }

    /// Stop watching a path.
    pub fn unwatch(&mut self, path: &Path) -> Result<(), FilesystemError> {
        self.debouncer
            .unwatch(path)
            .map_err(|e| FilesystemError::WatcherFailed(format!("{}: {e}", path.display())))?;

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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn test_config(dir: &Path) -> FilesystemConfig {
        FilesystemConfig {
            watch_paths: vec![dir.to_owned()],
            read_paths: vec![dir.to_owned()],
            write_paths: vec![dir.to_owned()],
            debounce_ms: 100,
        }
    }

    #[tokio::test]
    async fn test_watch_and_receive_event() {
        let dir = std::env::temp_dir().join("springtale_connector_fs_watch_test");
        fs::create_dir_all(&dir).ok();

        let config = test_config(&dir);
        let received = Arc::new(Mutex::new(Vec::<(String, serde_json::Value)>::new()));
        let received_clone = received.clone();

        let callbacks: Arc<Mutex<Vec<(SubscriptionId, String, TriggerCallback)>>> =
            Arc::new(Mutex::new(vec![(
                SubscriptionId(1),
                "file_created".to_owned(),
                Box::new(move |trigger: &str, payload: serde_json::Value| {
                    // Use try_lock since we're in a sync callback
                    if let Ok(mut v) = received_clone.try_lock() {
                        v.push((trigger.to_owned(), payload));
                    }
                }),
            )]));

        let mut watcher = FsConnectorWatcher::new(&config, callbacks).unwrap();
        watcher.watch(&dir, &config).unwrap();

        // Create a file to trigger event
        let test_file = dir.join("test_event.txt");
        fs::write(&test_file, "hello").ok();

        // Wait for debounced event
        tokio::time::sleep(Duration::from_secs(2)).await;

        let events = received.lock().await;
        // We should have at least one event (may have create + modify)
        assert!(!events.is_empty(), "expected at least one filesystem event");
        assert_eq!(events[0].0, "file_created");

        // Clean up
        drop(events);
        fs::remove_file(&test_file).ok();
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_watch_rejected_outside_allowlist() {
        let allowed = std::env::temp_dir().join("springtale_connector_fs_allowed");
        let forbidden = std::env::temp_dir().join("springtale_connector_fs_forbidden");
        fs::create_dir_all(&allowed).ok();
        fs::create_dir_all(&forbidden).ok();

        let config = FilesystemConfig {
            watch_paths: vec![allowed.clone()],
            read_paths: vec![],
            write_paths: vec![],
            debounce_ms: 100,
        };

        let callbacks: Arc<Mutex<Vec<(SubscriptionId, String, TriggerCallback)>>> =
            Arc::new(Mutex::new(vec![]));
        let mut watcher = FsConnectorWatcher::new(&config, callbacks).unwrap();

        // Should succeed for allowed path
        assert!(watcher.watch(&allowed, &config).is_ok());

        // Should fail for forbidden path
        let result = watcher.watch(&forbidden, &config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FilesystemError::PathNotAllowed(_)
        ));

        fs::remove_dir_all(&allowed).ok();
        fs::remove_dir_all(&forbidden).ok();
    }

    #[tokio::test]
    async fn test_unwatch() {
        let dir = std::env::temp_dir().join("springtale_connector_fs_unwatch_test");
        fs::create_dir_all(&dir).ok();

        let config = test_config(&dir);
        let callbacks: Arc<Mutex<Vec<(SubscriptionId, String, TriggerCallback)>>> =
            Arc::new(Mutex::new(vec![]));
        let mut watcher = FsConnectorWatcher::new(&config, callbacks).unwrap();

        watcher.watch(&dir, &config).unwrap();
        assert_eq!(watcher.watched_paths().len(), 1);

        watcher.unwatch(&dir).unwrap();
        assert!(watcher.watched_paths().is_empty());

        fs::remove_dir_all(&dir).ok();
    }
}
