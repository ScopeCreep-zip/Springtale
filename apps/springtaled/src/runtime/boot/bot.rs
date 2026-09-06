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

/// In-app messaging wiring for the bot event loop: the connector-ingress
/// channel (incoming messages → bot) and the in-app chat broadcast
/// (bot replies + fired notifications → `/chat/stream` SSE).
pub(super) struct BotChannels {
    pub(super) bot_msg_rx: mpsc::Receiver<springtale_connector::chat::ChatMessage>,
    pub(super) chat_tx: tokio::sync::broadcast::Sender<crate::api::chat::ChatStreamMessage>,
}

/// Initialize the bot runtime.
///
/// Spawns the bot event loop and the response dispatcher. Connector chat
/// loops are NOT started here — they follow the registry, wired by
/// `springtale_runtime::operations::connectors::wire_chat` (plan 6.4).
pub(super) async fn init_bot(
    runtime: &springtale_runtime::RuntimeState,
    scheduler: springtale_runtime::EmbeddedScheduler,
    channels: BotChannels,

    formation_cmd_rx: tokio::sync::mpsc::Receiver<
        springtale_cooperation::command::FormationCommand,
    >,
    formations_handle: Arc<RwLock<Vec<Formation>>>,
) -> Result<tokio::task::JoinHandle<()>> {
    let BotChannels {
        bot_msg_rx,
        chat_tx,
    } = channels;
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
    let response_runtime = runtime.clone();
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
            // Outbound half of the connector's ChatSource. Connectors
            // with no chat surface fall back to the generic
            // `send_message` action.
            match springtale_runtime::operations::connectors::send_chat(
                &response_runtime,
                &response.connector,
                &response.channel_id,
                &response.text,
            )
            .await
            {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => {
                    tracing::error!(
                        connector = %response.connector,
                        error = %e,
                        "failed to send bot response"
                    );
                    continue;
                }
            }
            let reg = response_runtime.registry.read().await;
            let input = serde_json::json!({
                "chat_id": response.channel_id,
                "text": response.text,
            });
            if let Err(e) = reg
                .execute(&response.connector, "send_message", input)
                .await
            {
                tracing::error!(
                    connector = %response.connector,
                    error = %e,
                    "failed to send bot response"
                );
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

    Ok(bot_handle)
}
