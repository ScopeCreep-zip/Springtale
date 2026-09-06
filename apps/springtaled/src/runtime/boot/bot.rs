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
                let fuel_initial = m.fuel_remaining.initial();
                let liveness = format!("{:?}", m.liveness);
                let attention_load = attention.load(&m.agent_id);
                let active_task = m
                    .active_task
                    .as_ref()
                    .map(|t| format!("{}:{}", t.task.target_connector, t.task.action_name));
                let consecutive_failures = m.consecutive_failures;

                FormationMemberDetail {
                    agent_id,
                    connector_name,
                    role,
                    health,
                    fuel_remaining,
                    fuel_initial,
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
    pub(super) bluesky: Option<connector_bluesky::BlueskyConfig>,
}

/// In-app messaging wiring for the bot event loop: the connector-ingress
/// channel (incoming messages → bot) and the in-app chat broadcast
/// (bot replies + fired notifications → `/chat/stream` SSE).
pub(super) struct BotChannels {
    pub(super) bot_msg_tx: mpsc::Sender<springtale_bot::IncomingMessage>,
    pub(super) bot_msg_rx: mpsc::Receiver<springtale_bot::IncomingMessage>,
    pub(super) chat_tx: tokio::sync::broadcast::Sender<crate::api::chat::ChatStreamMessage>,
}

/// Initialize bot runtime and wire connector gateways.
///
/// Spawns the bot event loop, response dispatcher, and all configured connector
/// gateway loops. Returns the bot task handle and connector shutdown senders.
pub(super) async fn init_bot(
    runtime: &springtale_runtime::RuntimeState,
    scheduler: springtale_runtime::EmbeddedScheduler,
    connectors: &ConnectorWiring,
    channels: BotChannels,
    formation_cmd_rx: tokio::sync::mpsc::Receiver<
        springtale_cooperation::command::FormationCommand,
    >,
    formations_handle: Arc<RwLock<Vec<Formation>>>,
) -> Result<(
    tokio::task::JoinHandle<()>,
    Vec<tokio::sync::watch::Sender<bool>>,
)> {
    let BotChannels {
        bot_msg_tx,
        bot_msg_rx,
        chat_tx,
    } = channels;
    // Rule-engine ingress for connector gateways: polling gateways emit
    // ConnectorEvents here so their event recipes fire (the scheduler owns
    // the `trigger_tx` the embedded trigger loop drains). Cloned before
    // `scheduler` moves into the recipe deployer below.
    let gateway_trigger_tx = scheduler.trigger_tx.clone();
    let (bot_response_tx, mut bot_response_rx) =
        mpsc::channel::<springtale_bot::OutgoingResponse>(256);
    let (_bot_rule_tx, bot_rule_rx) =
        mpsc::channel::<springtale_core::rule::engine::TriggerEvent>(256);

    // Conversational task-setup deploy port: lets the chat bot apply +
    // schedule a recipe a user configured by chatting (no AI needed).
    let recipe_deployer = Arc::new(
        springtale_bot::conversation::deploy::RuntimeRecipeDeployer::new(
            runtime.clone(),
            scheduler,
        ),
    );

    let bot = springtale_bot::BotBuilder::new()
        .recipe_deployer(recipe_deployer)
        .store(runtime.store.clone())
        .registry(runtime.registry.clone())
        .engine(runtime.engine.clone())
        .ai_adapter((**runtime.ai_adapter.load()).clone())
        .sentinel(runtime.sentinel.clone())
        .settings(runtime.bot_settings.clone())
        .connector_rx(bot_msg_rx)
        .rule_rx(bot_rule_rx)
        .response_tx(bot_response_tx)
        .formation_cmd_rx(formation_cmd_rx)
        .formations_handle(formations_handle)
        .role_registry(runtime.role_registry.clone())
        .capability_bridge(runtime.capability_bridge.clone())
        .canvas_tx(runtime.canvas_tx.clone())
        .cooperation_tx(runtime.cooperation_tx.clone())
        .utterance_defs(runtime.utterance_defs.clone())
        .cadence_tick(runtime.cadence_tick.clone())
        .formation_gossip(runtime.formation_gossip.clone())
        .knowledge_store(runtime.knowledge_store.clone())
        .build()
        .await
        .context("failed to initialize bot runtime")?;

    // Spawn bot event loop
    let bot_handle = tokio::spawn(async move {
        bot.start().await;
    });

    // Spawn response dispatcher: routes bot responses to connectors, or —
    // for the W5 in-app chat panel — to the chat broadcast (the desktop /
    // web / PWA `GET /chat/stream` subscribers). The `in-app` connector is
    // synthetic and has no `send_message`, so it MUST branch here.
    let response_registry = runtime.registry.clone();
    let response_chat_tx = chat_tx.clone();
    let _response_handle = tokio::spawn(async move {
        while let Some(response) = bot_response_rx.recv().await {
            if response.connector == crate::api::chat::IN_APP_CONNECTOR {
                // Broadcast to in-app chat clients. A send error just means
                // no panel is currently subscribed — drop silently.
                let _ = response_chat_tx.send(crate::api::chat::ChatStreamMessage {
                    session: response.channel_id,
                    text: response.text,
                });
                continue;
            }
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

    // Delivery forwarder: a fired Notify/SendMessage step is broadcast
    // on `runtime.notification_tx` by the embedded job consumer. Mirror
    // it into the in-app chat stream so a scheduled recipe (weather
    // briefing, hydration reminder, cron-runner, …) actually reaches
    // the user's chat panel instead of vanishing into a log line.
    let notif_chat_tx = chat_tx.clone();
    let mut notif_rx = runtime.notification_tx.subscribe();
    let _notif_handle = tokio::spawn(async move {
        loop {
            match notif_rx.recv().await {
                Ok(event) => {
                    let text = if event.body.is_empty() {
                        event.title
                    } else {
                        format!("{}\n{}", event.title, event.body)
                    };
                    // Send error = no panel subscribed right now; drop.
                    let _ = notif_chat_tx.send(crate::api::chat::ChatStreamMessage {
                        session: crate::api::chat::IN_APP_SESSION.to_owned(),
                        text,
                    });
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "notification forwarder lagged");
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
            gateway_trigger_tx.clone(),
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
            gateway_trigger_tx.clone(),
        )
        .await
        .context("failed to wire Nostr connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref irc_config) = connectors.irc {
        let shutdown_tx = crate::runtime::connectors::wire_irc(
            irc_config,
            &runtime.registry,
            bot_msg_tx.clone(),
            gateway_trigger_tx.clone(),
        )
        .await
        .context("failed to wire IRC connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref discord_config) = connectors.discord {
        let shutdown_tx = crate::runtime::connectors::wire_discord(
            discord_config,
            &runtime.registry,
            bot_msg_tx.clone(),
            gateway_trigger_tx.clone(),
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
            gateway_trigger_tx.clone(),
        )
        .await
        .context("failed to wire Signal connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    if let Some(ref bluesky_config) = connectors.bluesky {
        let shutdown_tx = crate::runtime::connectors::wire_bluesky(
            bluesky_config,
            &runtime.registry,
            gateway_trigger_tx.clone(),
        )
        .await
        .context("failed to wire Bluesky connector")?;
        connector_shutdowns.push(shutdown_tx);
    }
    // connector-matrix: DEFERRED — matrix-sdk 0.16 requires rusqlite 0.37
    // which has CVE-2025-70873 (heap info disclosure). Waiting for update.

    Ok((bot_handle, connector_shutdowns))
}
