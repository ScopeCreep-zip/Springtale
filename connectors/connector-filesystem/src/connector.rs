use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};
use springtale_connector::{Subscription, SubscriptionCounter, SubscriptionId};

use crate::actions;
use crate::config::FilesystemConfig;
use crate::error::FilesystemError;
use crate::triggers;
use crate::watcher::{FsConnectorWatcher, TriggerCallback};

/// The filesystem connector.
///
/// Watches directories for file changes (triggers) and provides read/write/list
/// actions with path allow-list enforcement. No network access — all operations
/// are local filesystem only.
pub struct FilesystemConnector {
    config: FilesystemConfig,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    /// Shared trigger callbacks, dispatched by the watcher.
    callbacks: Arc<Mutex<Vec<(SubscriptionId, String, TriggerCallback)>>>,
    /// The filesystem watcher instance. Created lazily when `on_event` is first called.
    watcher: Mutex<Option<FsConnectorWatcher>>,
    sub_counter: SubscriptionCounter,
}

impl FilesystemConnector {
    /// Create a new filesystem connector with the given configuration.
    pub fn new(config: FilesystemConfig) -> Self {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();

        let manifest = build_manifest(&config, &trigger_decls, &action_decls);

        Self {
            config,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            callbacks: Arc::new(Mutex::new(Vec::new())),
            watcher: Mutex::new(None),
            sub_counter: SubscriptionCounter::new(),
        }
    }

    /// Ensure the filesystem watcher is started and watching configured paths.
    async fn ensure_watcher_started(&self) -> Result<(), FilesystemError> {
        let mut watcher_guard = self.watcher.lock().await;
        if watcher_guard.is_some() {
            return Ok(());
        }

        let mut watcher = FsConnectorWatcher::new(&self.config, self.callbacks.clone())?;

        for path in &self.config.watch_paths {
            watcher.watch(path, &self.config)?;
        }

        *watcher_guard = Some(watcher);
        Ok(())
    }
}

#[async_trait]
impl Connector for FilesystemConnector {
    fn triggers(&self) -> &[TriggerDecl] {
        &self.triggers
    }

    fn actions(&self) -> &[ActionDecl] {
        &self.actions
    }

    async fn execute(
        &self,
        action: &str,
        input: serde_json::Value,
    ) -> Result<ActionResult, ConnectorError> {
        match action {
            "read_file" => {
                actions::read_file::execute(&self.config, &input).map_err(ConnectorError::from)
            }
            "write_file" => {
                actions::write_file::execute(&self.config, &input).map_err(ConnectorError::from)
            }
            "list_dir" => {
                actions::list_dir::execute(&self.config, &input).map_err(ConnectorError::from)
            }
            unknown => Err(ConnectorError::ExecutionFailed(format!(
                "unknown action: {unknown}"
            ))),
        }
    }

    async fn on_event(
        &self,
        trigger: &str,
        handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        // Validate trigger name
        let valid_triggers = ["file_created", "file_modified", "file_deleted"];
        if !valid_triggers.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        // Wrap the EventHandler into our TriggerCallback format.
        // EventHandler takes payload only; TriggerCallback takes (trigger_name, payload).
        // We ignore the trigger_name in the wrapper since on_event already filtered it.
        let callback: TriggerCallback = Box::new(move |_trigger_name, payload| {
            handler(payload);
        });

        let id = self.sub_counter.next();
        {
            let mut callbacks = self.callbacks.lock().await;
            callbacks.push((id, trigger.to_owned(), callback));
        }

        // Start the watcher if not already running
        self.ensure_watcher_started()
            .await
            .map_err(ConnectorError::from)?;

        tracing::info!(
            trigger = trigger,
            "registered event handler for filesystem trigger"
        );
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut callbacks = self.callbacks.lock().await;
        callbacks.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed filesystem event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

/// Build the connector manifest from config and declarations.
fn build_manifest(
    config: &FilesystemConfig,
    triggers: &[TriggerDecl],
    actions: &[ActionDecl],
) -> ConnectorManifest {
    let mut capabilities = Vec::new();

    // Declare FilesystemRead capabilities for all read and watch paths
    for path in &config.read_paths {
        capabilities.push(Capability::FilesystemRead {
            path: path.to_string_lossy().into_owned(),
        });
    }
    for path in &config.watch_paths {
        // Watch paths need read access
        let path_str = path.to_string_lossy().into_owned();
        if !capabilities.contains(&Capability::FilesystemRead {
            path: path_str.clone(),
        }) {
            capabilities.push(Capability::FilesystemRead { path: path_str });
        }
    }

    // Declare FilesystemWrite capabilities for all write paths
    for path in &config.write_paths {
        capabilities.push(Capability::FilesystemWrite {
            path: path.to_string_lossy().into_owned(),
        });
    }

    ConnectorManifest {
        name: "connector-filesystem".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description: "Local filesystem connector — watch directories, read/write files with path allow-list enforcement.".to_owned(),
        capabilities,
        triggers: triggers.to_vec(),
        actions: actions.to_vec(),
        data_disclosure: vec![
            DataDisclosure {
                data_type: "file contents".to_owned(),
                purpose: "reading and writing files as requested by automation rules".to_owned(),
                destination: "local only".to_owned(),
            },
            DataDisclosure {
                data_type: "filesystem events".to_owned(),
                purpose: "watching directories for file changes to trigger automation rules".to_owned(),
                destination: "local only".to_owned(),
            },
        ],
        wasm_hash: None,
        signature: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;

    fn test_connector(suffix: &str) -> (std::path::PathBuf, FilesystemConnector) {
        let dir = std::env::temp_dir().join(format!("springtale_connector_{suffix}"));
        fs::create_dir_all(&dir).ok();
        // Canonicalize to resolve macOS /tmp -> /private/tmp symlink
        let dir = dir.canonicalize().unwrap_or(dir);

        let config = FilesystemConfig {
            watch_paths: vec![dir.clone()],
            read_paths: vec![dir.clone()],
            write_paths: vec![dir.clone()],
            debounce_ms: 100,
        };

        (dir, FilesystemConnector::new(config))
    }

    #[test]
    fn test_manifest_name() {
        let (_dir, connector) = test_connector("manifest_name");
        assert_eq!(connector.manifest().name, "connector-filesystem");
        fs::remove_dir_all(&_dir).ok();
    }

    #[test]
    fn test_manifest_capabilities() {
        let (dir, connector) = test_connector("manifest_caps");
        let manifest = connector.manifest();

        // Should have FilesystemRead and FilesystemWrite for the test dir
        let has_read = manifest
            .capabilities
            .iter()
            .any(|c| matches!(c, Capability::FilesystemRead { .. }));
        let has_write = manifest
            .capabilities
            .iter()
            .any(|c| matches!(c, Capability::FilesystemWrite { .. }));
        assert!(has_read);
        assert!(has_write);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_triggers_count() {
        let (dir, connector) = test_connector("triggers_count");
        assert_eq!(connector.triggers().len(), 3);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_actions_count() {
        let (dir, connector) = test_connector("actions_count");
        assert_eq!(connector.actions().len(), 3);
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_execute_read_file() {
        let (dir, connector) = test_connector("exec_read");
        let file = dir.join("read_test.txt");
        fs::write(&file, "test content").ok();

        let result = connector
            .execute(
                "read_file",
                serde_json::json!({ "path": file.to_string_lossy() }),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output["content"], "test content");

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_execute_write_file() {
        let (dir, connector) = test_connector("exec_write");
        let file = dir.join("write_test.txt");

        let result = connector
            .execute(
                "write_file",
                serde_json::json!({
                    "path": file.to_string_lossy(),
                    "content": "written by connector"
                }),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(fs::read_to_string(&file).unwrap(), "written by connector");

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_execute_list_dir() {
        let (dir, connector) = test_connector("exec_listdir");
        fs::write(dir.join("file1.txt"), "a").ok();
        fs::write(dir.join("file2.txt"), "b").ok();

        let result = connector
            .execute(
                "list_dir",
                serde_json::json!({ "path": dir.to_string_lossy() }),
            )
            .await
            .unwrap();

        assert!(result.success);
        let count = result.output["count"].as_u64().unwrap_or(0);
        assert!(count >= 2);

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_execute_unknown_action() {
        let (dir, connector) = test_connector("exec_unknown");
        let result = connector
            .execute("nonexistent", serde_json::json!({}))
            .await;
        assert!(result.is_err());

        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_on_event_unknown_trigger() {
        let (dir, connector) = test_connector("on_event_unknown");
        let handler: EventHandler = Box::new(|_| {});
        let result = connector.on_event("nonexistent", handler).await;
        assert!(result.is_err());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_data_disclosure() {
        let (dir, connector) = test_connector("data_disclosure");
        let disclosures = &connector.manifest().data_disclosure;
        assert_eq!(disclosures.len(), 2);

        // All destinations should be "local only"
        for d in disclosures {
            assert_eq!(d.destination, "local only");
        }

        fs::remove_dir_all(&dir).ok();
    }
}
