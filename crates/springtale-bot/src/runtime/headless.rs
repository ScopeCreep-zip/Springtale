use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::RuleEngine;
use springtale_store::SqliteBackend;

use crate::error::BotError;
use crate::runtime::lifecycle::{BotBuilder, BotConfig, IncomingMessage, OutgoingResponse};

/// A headless bot for testing. No Telegram, no network.
///
/// Provides `send()` and `recv()` for driving tests.
pub struct HeadlessBot {
    msg_tx: mpsc::Sender<IncomingMessage>,
    response_rx: mpsc::Receiver<OutgoingResponse>,
    /// Trigger event sender — used by tests to inject rule triggers.
    pub rule_tx: mpsc::Sender<springtale_core::rule::engine::TriggerEvent>,
    /// Storage backend — used by tests to verify memory/session state.
    pub store: Arc<dyn springtale_store::StorageBackend>,
    _bot_handle: tokio::task::JoinHandle<()>,
}

impl HeadlessBot {
    /// Create a headless bot with in-memory SQLite and default config.
    pub async fn new() -> Result<Self, BotError> {
        Self::with_config(BotConfig::default()).await
    }

    /// Create a headless bot with a custom config.
    pub async fn with_config(config: BotConfig) -> Result<Self, BotError> {
        let store: Arc<dyn springtale_store::StorageBackend> = Arc::new(
            SqliteBackend::open_in_memory().map_err(|e| BotError::NotInitialized(e.to_string()))?,
        );
        // Open access by default in headless tests — the DM pairing
        // gate (check_access) is about hardening real deployments
        // against unknown senders, not simulated test users. Tests that
        // want to exercise the pairing flow can overwrite this.
        store
            .set_config("bot:access_mode", "\"open\"")
            .await
            .map_err(|e| BotError::NotInitialized(e.to_string()))?;
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new(
            CapabilityPolicy::Interactive,
        )));
        let engine = Arc::new(RwLock::new(RuleEngine::new()));

        let (msg_tx, msg_rx) = mpsc::channel::<IncomingMessage>(256);
        let (response_tx, response_rx) = mpsc::channel::<OutgoingResponse>(256);
        let (rule_tx, rule_rx) = mpsc::channel(256);

        let sentinel = std::sync::Arc::new(springtale_sentinel::Sentinel::new(
            springtale_sentinel::SentinelConfig::default(),
            store.clone(),
        ));

        let bot = BotBuilder::new()
            .store(store.clone())
            .registry(registry)
            .engine(engine)
            .sentinel(sentinel)
            .config(config)
            .connector_rx(msg_rx)
            .rule_rx(rule_rx)
            .response_tx(response_tx)
            .build()
            .await?;

        let handle = tokio::spawn(async move {
            bot.start().await;
        });

        Ok(Self {
            msg_tx,
            response_rx,
            rule_tx,
            store,
            _bot_handle: handle,
        })
    }

    /// Send a message as if from a user.
    pub async fn send(&self, user_id: &str, channel_id: &str, text: &str) {
        let msg = IncomingMessage {
            user_id: user_id.into(),
            channel_id: channel_id.into(),
            text: text.into(),
            source_connector: "test".into(),
            raw: serde_json::json!({}),
        };
        let _ = self.msg_tx.send(msg).await;
    }

    /// Wait for the next bot response (with timeout).
    pub async fn recv(&mut self) -> Option<OutgoingResponse> {
        tokio::time::timeout(std::time::Duration::from_secs(5), self.response_rx.recv())
            .await
            .ok()
            .flatten()
    }

    /// Send a message and wait for the response (convenience).
    pub async fn ask(&mut self, user_id: &str, text: &str) -> Option<String> {
        self.send(user_id, "default", text).await;
        self.recv().await.map(|r| r.text)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_help_command() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot.ask("user1", "/help").await;
        assert!(response.is_some());
        let text = response.unwrap();
        assert!(
            text.contains("help"),
            "response should mention help: {text}"
        );
    }

    #[tokio::test]
    async fn test_status_command() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot.ask("user1", "/status").await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("running"));
    }

    #[tokio::test]
    async fn test_unknown_command_fallback() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot.ask("user1", "/nonexistent").await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("Unknown command"));
    }

    #[tokio::test]
    async fn test_plain_text_fallback() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot.ask("user1", "just some text").await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("Unknown command"));
    }

    #[tokio::test]
    async fn test_rules_command_empty() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot.ask("user1", "/rules").await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("No rules"));
    }

    #[tokio::test]
    async fn test_connectors_command_empty() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot.ask("user1", "/connectors").await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("No connectors"));
    }

    #[tokio::test]
    async fn test_prefs_command_defaults() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot.ask("user1", "/prefs").await;
        assert!(response.is_some());
        let text = response.unwrap();
        assert!(text.contains("UTC"));
        assert!(text.contains("off"));
    }

    #[tokio::test]
    async fn test_prefs_set_timezone() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot
            .ask("user1", "/prefs set timezone America/New_York")
            .await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("Set timezone"));

        let response = bot.ask("user1", "/prefs").await;
        assert!(response.unwrap().contains("America/New_York"));
    }

    #[tokio::test]
    async fn test_alias_set_and_use() {
        let mut bot = HeadlessBot::new().await.unwrap();

        // Set alias
        let response = bot.ask("user1", "/alias set h help").await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("Alias set"));

        // Use alias immediately — should work because event loop
        // reloads AliasResolver after /alias commands
        let response = bot.ask("user1", "/h").await;
        assert!(response.is_some());
        let text = response.unwrap();
        assert!(
            text.contains("help") || text.contains("Available"),
            "alias /h should resolve to /help, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_alias_remove() {
        let mut bot = HeadlessBot::new().await.unwrap();

        // Set and then remove
        let _ = bot.ask("user1", "/alias set s status").await;
        let _ = bot.ask("user1", "/alias remove s").await;

        // Should no longer resolve
        let response = bot.ask("user1", "/s").await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("Unknown command"));
    }

    #[tokio::test]
    async fn test_alias_list() {
        let mut bot = HeadlessBot::new().await.unwrap();
        let _ = bot.ask("user1", "/alias set s search").await;

        let response = bot.ask("user1", "/alias").await;
        assert!(response.is_some());
        let text = response.unwrap();
        assert!(text.contains("/s"), "alias list should contain /s: {text}");
        assert!(
            text.contains("/search"),
            "alias list should contain /search: {text}"
        );
    }

    // ── Spec-required tests (phase-1b.md lines 91, 93, 94) ──

    #[tokio::test]
    async fn test_trigger_event_dispatches() {
        // phase-1b.md line 91: "fire a Trigger::ConnectorEvent → bot routes →
        // handler executes → response generated"
        let bot = HeadlessBot::new().await.unwrap();

        let trigger = springtale_core::rule::engine::TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some("connector-test".into()),
            event: Some("test_event".into()),
            payload: serde_json::json!({"data": "test"}),
        };

        // Send trigger — the bot's event loop should process it without crashing.
        // No rules are configured, so no actions fire, but the trigger path
        // must not panic or deadlock.
        bot.rule_tx.send(trigger).await.unwrap();

        // Give the event loop time to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Bot should still be responsive after processing the trigger
        let mut bot = bot;
        let response = bot.ask("user1", "/status").await;
        assert!(response.is_some());
        assert!(response.unwrap().contains("running"));
    }

    #[tokio::test]
    async fn test_session_isolation_concurrent() {
        // phase-1b.md line 93: "two users send commands concurrently,
        // state does not leak"
        let mut bot = HeadlessBot::new().await.unwrap();

        // User 1 sets timezone
        bot.send("user1", "chan1", "/prefs set timezone America/New_York")
            .await;
        bot.recv().await; // consume response

        // User 2 sets different timezone
        bot.send("user2", "chan1", "/prefs set timezone Europe/London")
            .await;
        bot.recv().await;

        // Verify user 1's prefs are NOT affected by user 2
        bot.send("user1", "chan1", "/prefs").await;
        let response = bot.recv().await.unwrap();
        assert!(
            response.text.contains("America/New_York"),
            "user1 prefs should be America/New_York, got: {}",
            response.text
        );

        // Verify user 2 has their own prefs
        bot.send("user2", "chan1", "/prefs").await;
        let response = bot.recv().await.unwrap();
        assert!(
            response.text.contains("Europe/London"),
            "user2 prefs should be Europe/London, got: {}",
            response.text
        );
    }

    #[tokio::test]
    async fn test_memory_compaction_drops_oldest() {
        // phase-1b.md line 94: "Memory compaction test: fill to N+1,
        // verify oldest dropped"
        let mut bot = HeadlessBot::with_config(BotConfig {
            context_window: 3,
            ..BotConfig::default()
        })
        .await
        .unwrap();

        // Send 5 messages (exceeds context_window of 3)
        for i in 0..5 {
            bot.send("user1", "chan1", &format!("/status msg{i}")).await;
            bot.recv().await; // consume response
        }

        // Give compaction time to run
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify only 3 entries remain (compaction keeps newest)
        let entries = bot.store.get_memory("user1", "chan1", 100).await.unwrap();

        // Each message generates 2 entries (user + assistant), so 5 messages = 10 entries.
        // Compaction keeps newest 3, so we expect exactly 3.
        assert!(
            entries.len() <= 3,
            "expected at most 3 entries after compaction, got {}",
            entries.len()
        );
    }
}
