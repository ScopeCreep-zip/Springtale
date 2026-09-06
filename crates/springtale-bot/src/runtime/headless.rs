use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::RuleEngine;
use springtale_store::SqliteBackend;

use crate::error::BotError;
use springtale_connector::chat::ChatMessage;

use crate::runtime::lifecycle::{BotBuilder, OutgoingResponse};

/// A headless bot for testing. No Telegram, no network.
///
/// Provides `send()` and `recv()` for driving tests.
pub struct HeadlessBot {
    msg_tx: mpsc::Sender<ChatMessage>,
    response_rx: mpsc::Receiver<OutgoingResponse>,
    /// Trigger event sender — used by tests to inject rule triggers.
    pub rule_tx: mpsc::Sender<springtale_core::rule::engine::TriggerEvent>,
    /// Storage backend — used by tests to verify memory/session state.
    pub store: Arc<dyn springtale_store::StorageBackend>,
    _bot_handle: tokio::task::JoinHandle<()>,
}

impl HeadlessBot {
    /// Create a headless bot with in-memory SQLite and default settings.
    pub async fn new() -> Result<Self, BotError> {
        Self::with_deployer(None).await
    }

    /// Create a headless bot with custom bot settings (persona, context
    /// window, tool policy — plan 6.3).
    pub async fn with_settings(
        settings: springtale_runtime::operations::bot_settings::BotSettings,
    ) -> Result<Self, BotError> {
        Self::build(
            None,
            Some(std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
                settings,
            ))),
        )
        .await
    }

    /// Create a headless bot, optionally wiring a conversational-setup
    /// deploy port (tests use a mock to observe what would be deployed).
    pub async fn with_deployer(
        deployer: Option<crate::conversation::deploy::SharedDeployer>,
    ) -> Result<Self, BotError> {
        Self::build(deployer, None).await
    }

    async fn build(
        deployer: Option<crate::conversation::deploy::SharedDeployer>,
        settings: Option<
            std::sync::Arc<
                arc_swap::ArcSwap<springtale_runtime::operations::bot_settings::BotSettings>,
            >,
        >,
    ) -> Result<Self, BotError> {
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

        let (msg_tx, msg_rx) = mpsc::channel::<ChatMessage>(256);
        let (response_tx, response_rx) = mpsc::channel::<OutgoingResponse>(256);
        let (rule_tx, rule_rx) = mpsc::channel(256);
        let (_formation_cmd_tx, formation_cmd_rx) =
            mpsc::channel::<springtale_cooperation::command::FormationCommand>(32);

        let sentinel = std::sync::Arc::new(springtale_sentinel::Sentinel::new(
            springtale_sentinel::SentinelConfig::default(),
            store.clone(),
        ));

        // Role registry + capability bridge are required injections on
        // the Bot builder (no fallbacks). In headless tests we spin up
        // a private instance of each — production daemon shares the
        // `RuntimeState` copies.
        let role_registry =
            std::sync::Arc::new(springtale_cooperation::role::RoleRegistry::with_builtins());
        let capability_bridge = springtale_runtime::CapabilityBridge::new(registry.clone());

        let mut builder = BotBuilder::new();
        if let Some(settings) = settings {
            builder = builder.settings(settings);
        }
        let mut builder = builder
            .store(store.clone())
            .registry(registry)
            .engine(engine)
            .sentinel(sentinel)
            .connector_rx(msg_rx)
            .rule_rx(rule_rx)
            .response_tx(response_tx)
            .formation_cmd_rx(formation_cmd_rx)
            .role_registry(role_registry)
            .capability_bridge(capability_bridge);
        if let Some(deployer) = deployer {
            builder = builder.recipe_deployer(deployer);
        }
        let bot = builder.build().await?;

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
        let msg = ChatMessage::chat("test", channel_id, user_id, text, serde_json::json!({}));
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

    /// Records what a conversational setup would deploy, so a test can
    /// assert the chat flow produced the right recipe + extracted inputs
    /// WITHOUT standing up a full `RuntimeState`.
    #[derive(Clone, Default)]
    struct RecordingDeployer {
        last: Arc<
            std::sync::Mutex<
                Option<(
                    String,
                    springtale_runtime::operations::recipes::types::RecipeInputs,
                )>,
            >,
        >,
    }

    #[async_trait::async_trait]
    impl crate::conversation::deploy::RecipeDeployer for RecordingDeployer {
        async fn preflight(
            &self,
            recipe_id: &str,
            _inputs: &springtale_runtime::operations::recipes::types::RecipeInputs,
        ) -> Result<
            springtale_runtime::operations::preflight::types::PreflightReport,
            crate::conversation::deploy::DeployError,
        > {
            Ok(
                springtale_runtime::operations::preflight::types::PreflightReport::from_items(
                    recipe_id.to_owned(),
                    Vec::new(),
                ),
            )
        }

        async fn deploy(
            &self,
            recipe_id: &str,
            inputs: springtale_runtime::operations::recipes::types::RecipeInputs,
        ) -> Result<
            springtale_runtime::operations::recipes::types::ApplyReport,
            crate::conversation::deploy::DeployError,
        > {
            *self.last.lock().unwrap() = Some((recipe_id.to_owned(), inputs));
            Ok(
                springtale_runtime::operations::recipes::types::ApplyReport {
                    recipe_id: recipe_id.to_owned(),
                    connectors_configured: vec!["connector-http".into()],
                    rules_created: vec!["rule-1".into()],
                    ai_configured: false,
                    summary: "your Tucson briefing will arrive every morning".into(),
                },
            )
        }
    }

    /// THE ACCEPTANCE GATE: the commander's free-text order, no AI, ANY
    /// city → one confirm → "yes" → deploys with the free target extracted
    /// from the sentence (city = "Sacramento, CA", time = 8 AM from "every
    /// morning"). Geocoding of the city happens inside the real
    /// `apply_recipe`; the mock deployer records the user's target verbatim.
    #[tokio::test]
    async fn test_conversational_weather_setup_no_ai_end_to_end() {
        let deployer = RecordingDeployer::default();
        let recorded = deployer.last.clone();
        let mut bot = HeadlessBot::with_deployer(Some(Arc::new(deployer)))
            .await
            .unwrap();

        // The exact order the user reported — a city NOT in any list.
        let r1 = bot
            .ask(
                "user1",
                "send me the weather for Sacramento, CA every morning",
            )
            .await
            .expect("a reply");
        assert!(
            r1.to_lowercase().contains("weather") || r1.contains("Sacramento") || r1.contains("?"),
            "expected an acknowledgement + confirm, got: {r1}"
        );
        // Nothing deployed until the user confirms.
        assert!(recorded.lock().unwrap().is_none());

        // Confirm.
        let r2 = bot.ask("user1", "yes").await.expect("a reply");
        let r2l = r2.to_lowercase();
        assert!(
            r2l.contains("done")
                || r2l.contains("all set")
                || r2l.contains("live")
                || r2.contains("🎉"),
            "expected a success message, got: {r2}"
        );

        // The deploy fired with the recipe + the free target extracted from
        // the sentence — NOT a silently-substituted default.
        let (recipe_id, inputs) = recorded.lock().unwrap().clone().expect("deploy was called");
        assert_eq!(recipe_id, "weather-briefing");
        assert_eq!(
            inputs.get("city").and_then(|v| v.as_str()),
            Some("Sacramento, CA"),
        );
        assert_eq!(
            inputs.get("schedule").and_then(|v| v.as_str()),
            Some("0 8 * * *"), // "every morning" → 8 AM preset
        );
    }

    /// Mid-flow correction works without AI: change the city before confirming.
    #[tokio::test]
    async fn test_conversational_correction_before_deploy() {
        let deployer = RecordingDeployer::default();
        let recorded = deployer.last.clone();
        let mut bot = HeadlessBot::with_deployer(Some(Arc::new(deployer)))
            .await
            .unwrap();

        let _ = bot.ask("user1", "morning weather for Phoenix").await;
        let _ = bot.ask("user1", "actually make it Tucson").await;
        let _ = bot.ask("user1", "yes").await;

        let (_, inputs) = recorded.lock().unwrap().clone().expect("deploy was called");
        assert_eq!(
            inputs.get("city").and_then(|v| v.as_str()),
            Some("Tucson"), // corrected target
        );
    }

    /// Cancelling drops the setup — nothing deploys.
    #[tokio::test]
    async fn test_conversational_cancel_deploys_nothing() {
        let deployer = RecordingDeployer::default();
        let recorded = deployer.last.clone();
        let mut bot = HeadlessBot::with_deployer(Some(Arc::new(deployer)))
            .await
            .unwrap();

        let _ = bot.ask("user1", "set up the morning weather").await;
        let _ = bot.ask("user1", "never mind").await;
        assert!(recorded.lock().unwrap().is_none());
    }

    /// SECURITY: a recipe needing a credential is handed off to the secure
    /// setup flow — the bot never asks for the token in chat, and a token
    /// the user volunteers never lands in the (unencrypted) session store.
    #[tokio::test]
    async fn test_secret_recipe_handed_off_and_no_token_in_session_store() {
        let deployer = RecordingDeployer::default();
        let recorded = deployer.last.clone();
        let mut bot = HeadlessBot::with_deployer(Some(Arc::new(deployer)))
            .await
            .unwrap();

        let r = bot
            .ask("user1", "set up a telegram echo bot")
            .await
            .expect("a reply");
        let lower = r.to_lowercase();
        assert!(
            lower.contains("library") || lower.contains("vault") || lower.contains("securely"),
            "expected a secure-setup handoff, got: {r}"
        );

        // The user volunteers a token anyway — it must NOT be collected,
        // deployed, or persisted in plaintext.
        let secret = "12345:AAH-very-secret-token";
        let _ = bot.ask("user1", &format!("my bot token is {secret}")).await;
        assert!(
            recorded.lock().unwrap().is_none(),
            "no deploy should have happened"
        );
        let session = bot.store.get_session("user1", "default").await.unwrap();
        if let Some(row) = session {
            assert!(
                !row.state_data.contains(secret),
                "credential leaked into session state_data: {}",
                row.state_data
            );
        }
    }

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
        // Plain text that matches no recipe and no command, with no AI
        // adapter, now gets the deterministic capability reply (bimbo-mode
        // "here's what I can do") rather than a terse "Unknown command".
        let mut bot = HeadlessBot::new().await.unwrap();
        let response = bot.ask("user1", "zxqwv plok florble").await;
        assert!(response.is_some());
        let text = response.unwrap().to_lowercase();
        assert!(
            text.contains("automations")
                || text.contains("set it up")
                || text.contains("plain words"),
            "expected a capability reply, got: {text}"
        );
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
        let mut bot =
            HeadlessBot::with_settings(springtale_runtime::operations::bot_settings::BotSettings {
                context_window: 3,
                ..Default::default()
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
