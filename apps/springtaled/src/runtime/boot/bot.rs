use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{RwLock, mpsc};

use springtale_bot::cooperation::formation::Formation;
use springtale_runtime::operations::formations::{AgentHealthDetail, FormationMemberDetail};

/// Live formation reader backed by the bot's in-memory formation list.
///
/// Reads `Arc<RwLock<Vec<Formation>>>` shared with the bot event loop
/// to provide enriched per-member data to the `get_formation()` API.
pub(crate) struct BotFormationReader {
    formations: Arc<RwLock<Vec<Formation>>>,
}

impl BotFormationReader {
    pub(crate) fn new(formations: Arc<RwLock<Vec<Formation>>>) -> Self {
        Self { formations }
    }
}

#[async_trait::async_trait]
impl springtale_runtime::LiveFormationReader for BotFormationReader {
    async fn read_member_details(&self, formation_id: &str) -> Vec<FormationMemberDetail> {
        let formations = self.formations.read().await;
        let Some(formation) = formations.iter().find(|f| f.id.to_string() == formation_id) else {
            return Vec::new();
        };

        let attention = formation.attention_broker.current();

        formation
            .members
            .iter()
            .map(|m| {
                let connector_name = m
                    .capabilities
                    .first()
                    .map(|c| c.to_string())
                    .unwrap_or_default();

                let agent_id = m.agent_id.0.to_string();
                let role = m.role.name().to_owned();
                // Structured health — `From<&AgentHealth>` is in the
                // runtime crate so the serialized shape stays stable
                // even if the cooperation enum gains variants.
                let health = AgentHealthDetail::from(&m.health);
                let fuel_remaining = m.fuel_remaining.remaining();
                let liveness = format!("{:?}", m.liveness);
                let attention_load = attention.load(&m.agent_id);
                let active_task = m.active_task.as_ref().map(|t| {
                    format!("{}:{}", t.task.target_connector, t.task.action_name)
                });
                let consecutive_failures = m.consecutive_failures;

                FormationMemberDetail {
                    agent_id,
                    connector_name,
                    role,
                    health,
                    fuel_remaining,
                    liveness,
                    attention_load,
                    active_task,
                    consecutive_failures,
                }
            })
            .collect()
    }
}

/// Holds optional connector configs for wiring during boot.
/// Avoids partial-move issues with the top-level `SpringtaleConfig`.
pub(super) struct ConnectorWiring {
    pub(super) telegram: Option<connector_telegram::TelegramConfig>,
    pub(super) nostr: Option<connector_nostr::NostrConfig>,
    pub(super) irc: Option<connector_irc::IrcConfig>,
    pub(super) discord: Option<connector_discord::DiscordConfig>,
    pub(super) slack: Option<connector_slack::SlackConfig>,
    pub(super) signal: Option<connector_signal::SignalConfig>,
}

/// Initialize bot runtime and wire connector gateways.
///
/// Spawns the bot event loop, response dispatcher, and all configured connector
/// gateway loops. Returns the bot task handle and connector shutdown senders.
pub(super) async fn init_bot(
    runtime: &springtale_runtime::RuntimeState,
    bot_config: Option<springtale_bot::BotConfig>,
    connectors: &ConnectorWiring,
    bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    bot_msg_rx: mpsc::Receiver<springtale_bot::IncomingMessage>,
    formation_cmd_rx: tokio::sync::mpsc::Receiver<springtale_cooperation::command::FormationCommand>,
    formations_handle: Arc<RwLock<Vec<Formation>>>,
) -> Result<(
    tokio::task::JoinHandle<()>,
    Vec<tokio::sync::watch::Sender<bool>>,
)> {
    let (bot_response_tx, mut bot_response_rx) =
        mpsc::channel::<springtale_bot::OutgoingResponse>(256);
    let (_bot_rule_tx, bot_rule_rx) =
        mpsc::channel::<springtale_core::rule::engine::TriggerEvent>(256);

    let bot_config = bot_config.unwrap_or_default();

    let bot = springtale_bot::BotBuilder::new()
        .store(runtime.store.clone())
        .registry(runtime.registry.clone())
        .engine(runtime.engine.clone())
        .ai_adapter((**runtime.ai_adapter.load()).clone())
        .sentinel(runtime.sentinel.clone())
        .config(bot_config)
        .connector_rx(bot_msg_rx)
        .rule_rx(bot_rule_rx)
        .response_tx(bot_response_tx)
        .formation_cmd_rx(formation_cmd_rx)
        .formations_handle(formations_handle)
        .role_registry(runtime.role_registry.clone())
        .capability_bridge(runtime.capability_bridge.clone())
        .canvas_tx(runtime.canvas_tx.clone())
        .cooperation_tx(runtime.cooperation_tx.clone())
        .build()
        .await
        .context("failed to initialize bot runtime")?;

    // Spawn bot event loop
    let bot_handle = tokio::spawn(async move {
        bot.start().await;
    });

    // Spawn response dispatcher: routes bot responses to connectors
    let response_registry = runtime.registry.clone();
    let _response_handle = tokio::spawn(async move {
        while let Some(response) = bot_response_rx.recv().await {
            let reg = response_registry.read().await;
            let input = serde_json::json!({
                "chat_id": response.channel_id,
                "text": response.text,
            });
            match reg
                .execute(&response.connector, "send_message", input)
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        connector = %response.connector,
                        error = %e,
                        "failed to send bot response"
                    );
                }
            }
        }
    });

    tracing::info!("bot runtime started");

    // ── Step 7b: Start connector gateways ──
    // Connectors are already registered in the registry by the factory system
    // (via inventory::submit! in each connector crate). Gateway loops bridge
    // incoming messages from chat platforms to the bot runtime.
    let mut connector_shutdowns: Vec<tokio::sync::watch::Sender<bool>> = Vec::new();

    if let Some(ref tg_config) = connectors.telegram {
        // Polling mode returns Some(shutdown_tx); webhook mode returns None.
        let shutdown = crate::runtime::connectors::wire_telegram(
            tg_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Telegram connector")?;
        if let Some(tx) = shutdown {
            connector_shutdowns.push(tx);
        }
    }
    if let Some(ref nostr_config) = connectors.nostr {
        let shutdown_tx = crate::runtime::connectors::wire_nostr(
            nostr_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Nostr connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref irc_config) = connectors.irc {
        let shutdown_tx =
            crate::runtime::connectors::wire_irc(irc_config, &runtime.registry, bot_msg_tx.clone())
                .await
                .context("failed to wire IRC connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref discord_config) = connectors.discord {
        let shutdown_tx = crate::runtime::connectors::wire_discord(
            discord_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Discord connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref slack_config) = connectors.slack {
        let shutdown_tx = crate::runtime::connectors::wire_slack(
            slack_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Slack connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref signal_config) = connectors.signal {
        let shutdown_tx = crate::runtime::connectors::wire_signal(
            signal_config,
            &runtime.registry,
            bot_msg_tx.clone(),
        )
        .await
        .context("failed to wire Signal connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    // connector-matrix: DEFERRED — matrix-sdk 0.16 requires rusqlite 0.37
    // which has CVE-2025-70873 (heap info disclosure). Waiting for update.

    Ok((bot_handle, connector_shutdowns))
}
