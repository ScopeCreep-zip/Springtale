use async_trait::async_trait;

use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::error::ConnectorError;
use springtale_connector::manifest::types::{
    ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl,
};
use springtale_connector::Subscription;

use crate::actions;
use crate::config::ShellConfig;

/// The shell connector.
///
/// Executes allow-listed shell commands with configurable timeouts.
/// Action-only connector — no triggers.
///
/// Requires the `ShellExec` capability, which always triggers a blocking
/// approval prompt from the capability checker.
pub struct ShellConnector {
    config: ShellConfig,
    manifest: ConnectorManifest,
    actions: Vec<ActionDecl>,
}

impl ShellConnector {
    /// Create a new shell connector with the given configuration.
    pub fn new(config: ShellConfig) -> Self {
        let action_decls = actions::action_declarations();
        let manifest = build_manifest(&action_decls);

        Self {
            config,
            manifest,
            actions: action_decls,
        }
    }
}

#[async_trait]
impl Connector for ShellConnector {
    fn triggers(&self) -> &[TriggerDecl] {
        // Shell connector is action-only — no triggers
        &[]
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
            "exec" => actions::exec::execute(&self.config, &input)
                .await
                .map_err(ConnectorError::from),
            unknown => Err(ConnectorError::ExecutionFailed(format!(
                "unknown action: {unknown}"
            ))),
        }
    }

    async fn on_event(
        &self,
        trigger: &str,
        _handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        // Shell connector has no triggers
        Err(ConnectorError::ExecutionFailed(format!(
            "shell connector has no triggers, cannot register handler for: {trigger}"
        )))
    }

    async fn remove_event(&self, _sub: &Subscription) -> Result<(), ConnectorError> {
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

/// Build the connector manifest.
fn build_manifest(actions: &[ActionDecl]) -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-shell".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: "Springtale".to_owned(),
        description:
            "Shell command connector — execute allow-listed commands with configurable timeouts."
                .to_owned(),
        capabilities: vec![Capability::ShellExec],
        triggers: vec![],
        actions: actions.to_vec(),
        data_disclosure: vec![DataDisclosure {
            data_type: "command output".to_owned(),
            purpose: "capturing stdout/stderr from executed commands for automation rules"
                .to_owned(),
            destination: "local only".to_owned(),
        }],
        roles: vec![],
        wasm_hash: None,
        signature: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn test_connector() -> ShellConnector {
        let config = ShellConfig {
            allowed_commands: vec!["echo".to_owned(), "true".to_owned()],
            timeout_secs: 5,
            working_directory: None,
        };
        ShellConnector::new(config)
    }

    #[test]
    fn test_manifest_name() {
        let connector = test_connector();
        assert_eq!(connector.manifest().name, "connector-shell");
    }

    #[test]
    fn test_manifest_requires_shell_exec() {
        let connector = test_connector();
        let has_shell_exec = connector
            .manifest()
            .capabilities
            .iter()
            .any(|c| matches!(c, Capability::ShellExec));
        assert!(has_shell_exec);
    }

    #[test]
    fn test_no_triggers() {
        let connector = test_connector();
        assert!(connector.triggers().is_empty());
    }

    #[test]
    fn test_one_action() {
        let connector = test_connector();
        assert_eq!(connector.actions().len(), 1);
        assert_eq!(connector.actions()[0].name, "exec");
    }

    #[tokio::test]
    async fn test_execute_echo() {
        let connector = test_connector();
        let result = connector
            .execute(
                "exec",
                serde_json::json!({
                    "command": "echo",
                    "args": ["test"]
                }),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output["stdout"].as_str().unwrap().trim(), "test");
    }

    #[tokio::test]
    async fn test_execute_unknown_action() {
        let connector = test_connector();
        let result = connector
            .execute("nonexistent", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_on_event_rejected() {
        let connector = test_connector();
        let handler: EventHandler = Box::new(|_| {});
        let result = connector.on_event("anything", handler).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_data_disclosure() {
        let connector = test_connector();
        let disclosures = &connector.manifest().data_disclosure;
        assert_eq!(disclosures.len(), 1);
        assert_eq!(disclosures[0].destination, "local only");
    }
}
