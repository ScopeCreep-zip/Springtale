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
use crate::client::ChromeClient;
use crate::config::BrowserConfig;
use crate::triggers;

/// Browser automation connector — headless Chromium with domain allow-list.
///
/// Uses `chromiumoxide` crate (async, tokio-native) rather than `headless_chrome`
/// (sync, thread-based). chromiumoxide was chosen for better async integration
/// with our tokio runtime and active maintenance. Both crates use Chrome
/// DevTools Protocol — the connector pattern is the same either way.
///
/// Each allowed domain is declared as Capability::NetworkOutbound in the
/// manifest. The capability check runs BEFORE every navigate action —
/// the browser CANNOT navigate to unapproved sites.
///
/// Granular browser-specific capabilities (BrowserNavigate, BrowserFormFill,
/// BrowserScreenshot) are deferred to Phase 3 — Phase 2b uses the generic
/// NetworkOutbound capability for domain-level enforcement.
///
/// Privacy: Chrome telemetry is disabled by default. Browser profile
/// is created in a temp directory and deleted on shutdown. No persistent
/// cookies or browsing history.
///
/// WARNING: Web pages may execute JavaScript when navigated to.
/// Only allow-list domains you trust.
pub struct BrowserConnector {
    client: ChromeClient,
    manifest: ConnectorManifest,
    triggers: Vec<TriggerDecl>,
    actions: Vec<ActionDecl>,
    handlers: Arc<Mutex<Vec<(SubscriptionId, String, EventHandler)>>>,
    sub_counter: SubscriptionCounter,
}

impl BrowserConnector {
    pub fn new(config: &BrowserConfig) -> Result<Self, crate::error::BrowserError> {
        let trigger_decls = triggers::trigger_declarations();
        let action_decls = actions::action_declarations();

        let capabilities: Vec<Capability> = config
            .allowed_domains
            .iter()
            .map(|host| Capability::NetworkOutbound { host: host.clone() })
            .collect();

        let client = ChromeClient::new(config.allowed_domains.clone(), config.message_jitter_secs);

        let manifest = ConnectorManifest {
            name: "connector-browser".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            author: "Springtale".to_owned(),
            description: "Browser automation — headless Chromium with domain allow-list. \
                         WARNING: Navigated pages may execute JavaScript."
                .to_owned(),
            capabilities,
            triggers: trigger_decls.clone(),
            actions: action_decls.clone(),
            data_disclosure: vec![
                DataDisclosure {
                    data_type: "web page content from allowed domains".to_owned(),
                    purpose: "browser automation (navigate, fill forms, extract text)".to_owned(),
                    destination: "local process only — headless Chrome runs on this machine. \
                                 Web pages are loaded in memory, not persisted."
                        .to_owned(),
                },
                DataDisclosure {
                    data_type: "network requests to allowed domains".to_owned(),
                    purpose: "loading web pages".to_owned(),
                    destination: "direct HTTPS connections to allowed domains. \
                                 Server operators see your IP address. Use a VPN."
                        .to_owned(),
                },
                DataDisclosure {
                    data_type: "JavaScript execution context".to_owned(),
                    purpose: "page rendering and interaction".to_owned(),
                    destination: "local process — JavaScript runs in headless Chrome sandbox. \
                                 Pages may attempt to load resources from third-party domains."
                        .to_owned(),
                },
            ],
            roles: vec![],
            wasm_hash: None,
            signature: None,
        };

        Ok(Self {
            client,
            manifest,
            triggers: trigger_decls,
            actions: action_decls,
            handlers: Arc::new(Mutex::new(Vec::new())),
            sub_counter: SubscriptionCounter::new(),
        })
    }
}

#[async_trait]
impl Connector for BrowserConnector {
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
            "navigate" => actions::navigate::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "fill_form" => actions::fill_form::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "click" => actions::click::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "screenshot" => actions::screenshot::execute(&self.client, &input)
                .await
                .map_err(ConnectorError::from),
            "extract_text" => actions::extract_text::execute(&self.client, &input)
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
        handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        let valid = ["page_loaded", "element_found"];
        if !valid.contains(&trigger) {
            return Err(ConnectorError::ExecutionFailed(format!(
                "unknown trigger: {trigger}"
            )));
        }

        let id = self.sub_counter.next();
        let mut handlers = self.handlers.lock().await;
        handlers.push((id, trigger.to_owned(), handler));
        tracing::info!(trigger = trigger, "registered browser event handler");
        Ok(Subscription {
            id,
            trigger: trigger.to_owned(),
        })
    }

    async fn remove_event(&self, sub: &Subscription) -> Result<(), ConnectorError> {
        let mut handlers = self.handlers.lock().await;
        handlers.retain(|(id, _, _)| *id != sub.id);
        tracing::info!(id = ?sub.id, trigger = %sub.trigger, "removed browser event handler");
        Ok(())
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_count() {
        assert_eq!(triggers::trigger_declarations().len(), 2);
    }

    #[test]
    fn test_action_count() {
        assert_eq!(actions::action_declarations().len(), 5);
    }
}
