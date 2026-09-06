use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use springtale_connector::connector::trait_::EventHandler;
use springtale_connector::registry::store::ConnectorRegistry;

use crate::error::BotError;

const CONNECTOR_NAME: &str = "connector-bluesky";

/// ATProto bot bridge — wraps connector-bluesky triggers/actions into
/// typed, high-level methods for bot automation.
///
/// Derived from malwarevangelist-bot's enCore engine session management
/// patterns. Provides `on_mention`, `on_follow`, `post`, `reply` —
/// the four core operations for a Bluesky bot.
///
/// This bridge works through the `ConnectorRegistry` and `Connector`
/// trait abstraction — it does NOT depend on `connector-bluesky`
/// directly (per crate-structure.md dependency rules).
///
/// Privacy: ATProto is decentralized. Users choose their PDS (Personal
/// Data Server). Posts are public by default.
pub struct ATProtoBotBridge {
    registry: Arc<RwLock<ConnectorRegistry>>,
    /// Registered event handlers for mention and follow triggers.
    /// Stored locally and dispatched when the bridge processes incoming events.
    handlers: Arc<Mutex<Vec<(String, EventHandler)>>>,
}

impl ATProtoBotBridge {
    /// Create a new ATProto bridge.
    ///
    /// The connector-bluesky must be installed in the registry before
    /// calling any bridge methods.
    pub fn new(registry: Arc<RwLock<ConnectorRegistry>>) -> Self {
        Self {
            registry,
            handlers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a handler for mention events.
    ///
    /// The handler is called when a post mentioning the bot's DID is
    /// received via the Jetstream firehose. The payload contains the
    /// post content, author DID, and post URI.
    pub async fn on_mention(&self, handler: EventHandler) {
        let mut handlers = self.handlers.lock().await;
        handlers.push(("mention".to_owned(), handler));
    }

    /// Register a handler for follow events.
    ///
    /// The handler is called when a user follows the bot's account.
    /// The payload contains the follower's DID and the follow record URI.
    pub async fn on_follow(&self, handler: EventHandler) {
        let mut handlers = self.handlers.lock().await;
        handlers.push(("follow".to_owned(), handler));
    }

    /// Dispatch an incoming event to registered handlers.
    ///
    /// Called by the bot event loop when an ChatMessage arrives from
    /// connector-bluesky. The trigger name is extracted from the raw
    /// payload's "trigger" field.
    pub async fn dispatch(&self, trigger: &str, payload: serde_json::Value) {
        let handlers = self.handlers.lock().await;
        for (registered_trigger, handler) in handlers.iter() {
            if registered_trigger == trigger {
                handler(payload.clone());
            }
        }
    }

    /// Create a post on Bluesky.
    ///
    /// Returns the created post's URI and CID as JSON.
    pub async fn post(&self, content: &str) -> Result<serde_json::Value, BotError> {
        let input = serde_json::json!({ "text": content });
        let reg = self.registry.read().await;
        let result = reg.execute(CONNECTOR_NAME, "create_post", input).await?;
        Ok(result.output)
    }

    /// Reply to a post on Bluesky.
    ///
    /// `parent_uri` and `parent_cid` identify the post being replied to.
    /// `root_uri` and `root_cid` identify the thread root (same as parent
    /// for top-level replies).
    pub async fn reply(
        &self,
        content: &str,
        parent_uri: &str,
        parent_cid: &str,
        root_uri: &str,
        root_cid: &str,
    ) -> Result<serde_json::Value, BotError> {
        let input = serde_json::json!({
            "text": content,
            "parent_uri": parent_uri,
            "parent_cid": parent_cid,
            "root_uri": root_uri,
            "root_cid": root_cid,
        });
        let reg = self.registry.read().await;
        let result = reg.execute(CONNECTOR_NAME, "reply", input).await?;
        Ok(result.output)
    }

    /// Like a post on Bluesky.
    pub async fn like(
        &self,
        subject_uri: &str,
        subject_cid: &str,
    ) -> Result<serde_json::Value, BotError> {
        let input = serde_json::json!({
            "subject_uri": subject_uri,
            "subject_cid": subject_cid,
        });
        let reg = self.registry.read().await;
        let result = reg.execute(CONNECTOR_NAME, "like", input).await?;
        Ok(result.output)
    }

    /// Repost a post on Bluesky.
    pub async fn repost(
        &self,
        subject_uri: &str,
        subject_cid: &str,
    ) -> Result<serde_json::Value, BotError> {
        let input = serde_json::json!({
            "subject_uri": subject_uri,
            "subject_cid": subject_cid,
        });
        let reg = self.registry.read().await;
        let result = reg.execute(CONNECTOR_NAME, "repost", input).await?;
        Ok(result.output)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_connector_name() {
        assert_eq!(CONNECTOR_NAME, "connector-bluesky");
    }

    #[tokio::test]
    async fn test_on_mention_registers_handler() {
        use springtale_connector::capability::grant::CapabilityPolicy;
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::Interactive,
        )));
        let bridge = ATProtoBotBridge::new(registry);

        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();
        bridge
            .on_mention(Box::new(move |_payload| {
                called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }))
            .await;

        bridge
            .dispatch("mention", serde_json::json!({"text": "hello @bot"}))
            .await;
        assert!(called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_on_follow_registers_handler() {
        use springtale_connector::capability::grant::CapabilityPolicy;
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::Interactive,
        )));
        let bridge = ATProtoBotBridge::new(registry);

        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();
        bridge
            .on_follow(Box::new(move |_payload| {
                called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }))
            .await;

        bridge
            .dispatch("follow", serde_json::json!({"did": "did:plc:abc"}))
            .await;
        assert!(called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_dispatch_ignores_unregistered_triggers() {
        use springtale_connector::capability::grant::CapabilityPolicy;
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::Interactive,
        )));
        let bridge = ATProtoBotBridge::new(registry);

        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();
        bridge
            .on_mention(Box::new(move |_payload| {
                called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }))
            .await;

        // Dispatch a "follow" event — mention handler should NOT fire
        bridge
            .dispatch("follow", serde_json::json!({"did": "did:plc:abc"}))
            .await;
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    }
}
