//! The ONLY code path that materializes live `Formation` structs from
//! database rows or removes them from the active set. Triggered by
//! `FormationCommand`s posted on `bot.formation_cmd_rx` from the runtime
//! operations layer (HTTP / Tauri IPC).
//!
//! Distinct from the per-tick pipeline: command handling mutates the
//! formation set itself (add/remove/change-intent) whereas the tick
//! pipeline mutates each formation's internal state. Keeping them in
//! separate modules avoids accidentally letting tick logic leak into
//! command-driven mutations.

use crate::cooperation::lifecycle;
use crate::runtime::lifecycle::Bot;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::command::FormationCommand;
use springtale_cooperation::rally::{RallyResult, cascade};

pub async fn handle_formation_command(bot: &mut Bot, cmd: FormationCommand) {
    match cmd {
        FormationCommand::Deploy { formation_id } => {
            match lifecycle::spawn_formation(
                &formation_id.to_string(),
                &bot.store,
                &bot.registry,
                &bot.cadence,
                &bot.gossip_store,
                bot.formation_gossip.as_ref(),
                bot.knowledge_store.as_ref(),
            )
            .await
            {
                Ok(formation) => {
                    bot.formations.write().await.push(formation);
                    tracing::info!(id = %formation_id, "formation materialized from DB");
                }
                Err(e) => {
                    tracing::error!(id = %formation_id, error = %e, "formation spawn failed");
                }
            }
        }
        FormationCommand::Dissolve { formation_id, reason } => {
            let mut formations = bot.formations.write().await;
            // Persist mental-model state BEFORE dropping the formation so
            // accumulated conventions / patterns / vocabulary survive the
            // dissolve (`COOPERATION.md §21`).
            if let Some(f) = formations.iter().find(|f| f.id == formation_id) {
                if let Err(e) = lifecycle::persist_mental_model(f, &bot.store).await {
                    tracing::warn!(
                        id = %formation_id,
                        error = %e,
                        "failed to persist mental model on dissolve"
                    );
                }
                // G6 — broadcast the terminal `FormationOutcome` so peer
                // formations on the same gossip bus see the dissolve and
                // can adapt their own intent (Quickwit chitchat-style
                // sticky entry; subscribers that come online later still
                // see this via the bus's outcome replay).
                if let Some(bus) = f.formation_gossip.clone() {
                    let outcome = springtale_cooperation::gossip::FormationOutcome {
                        formation_id: f.id,
                        final_intent: f.intent.clone(),
                        success_count: f.momentum.consecutive_successes,
                        failure_count: 0,
                        dissolve_reason: reason.clone(),
                        at: chrono::Utc::now(),
                    };
                    tokio::spawn(async move { bus.publish_outcome(outcome).await });
                }
                // G2 — durable cross-formation record. Same data as the
                // gossip-bus outcome above, plus the connector set the
                // formation actually used (so retrieval can rank future
                // formations by connector overlap).
                if let Some(ks) = bot.knowledge_store.clone() {
                    let connectors: Vec<String> = f
                        .members
                        .iter()
                        .flat_map(|m| m.capabilities.iter())
                        .map(|c| c.name.clone())
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();
                    let note = springtale_cooperation::memory::OutcomeNote {
                        formation_id: f.id,
                        intent: f.intent.clone(),
                        peak_tier: f.momentum.tier,
                        connectors,
                        success_count: f.momentum.consecutive_successes,
                        failure_count: 0,
                        dissolve_reason: reason.clone(),
                        at: chrono::Utc::now(),
                    };
                    tokio::spawn(async move { ks.record_outcome(note).await });
                }
            }
            let before = formations.len();
            formations.retain(|f| f.id != formation_id);
            let removed = before - formations.len();
            if removed > 0 {
                tracing::info!(id = %formation_id, %reason, "formation dissolved");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for dissolve");
            }
        }
        FormationCommand::Pause { formation_id } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                formation.paused = true;
                formation.broadcast_context();
                tracing::info!(id = %formation_id, "formation paused");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for pause");
            }
        }
        FormationCommand::Resume { formation_id } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                formation.paused = false;
                formation.broadcast_context();
                tracing::info!(id = %formation_id, "formation resumed");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for resume");
            }
        }
        FormationCommand::ChangeIntent { formation_id, intent } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                formation.intent = intent;
                tracing::info!(id = %formation_id, "formation intent updated");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for intent change");
            }
        }
        FormationCommand::AddMember { formation_id, connector_name } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                let agent_id = AgentId::new();
                let member = crate::cooperation::formation::FormationMember::from_strings(
                    agent_id,
                    vec![connector_name.clone()],
                );
                let new_id = member.agent_id;
                formation.join(member);
                // Spawn the per-member runner so the new agent can respond
                // to L4 CFPs (B2). Idempotent — safe even if the member
                // somehow already has a runner.
                formation.start_runner_for(new_id);
                tracing::info!(
                    id = %formation_id,
                    connector = %connector_name,
                    "member added to formation"
                );
            } else {
                tracing::warn!(id = %formation_id, "formation not found for AddMember");
            }
        }
        FormationCommand::RemoveMember { formation_id, connector_name } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                if let Some(agent_id) = formation
                    .members
                    .iter()
                    .find(|m| m.capabilities.iter().any(|c| c == &connector_name))
                    .map(|m| m.agent_id)
                {
                    formation.leave(agent_id);
                    tracing::info!(
                        id = %formation_id,
                        connector = %connector_name,
                        "member removed from formation"
                    );
                } else {
                    tracing::warn!(
                        id = %formation_id,
                        connector = %connector_name,
                        "member not found for RemoveMember"
                    );
                }
            } else {
                tracing::warn!(id = %formation_id, "formation not found for RemoveMember");
            }
        }
        FormationCommand::Rally { formation_id } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                // Manual rally — find the lowest-attention agent and rally
                // around them. Mirrors the cascade-driven path
                // (`tick_steps/check_cascade.rs`) but is operator-initiated.
                let attn = formation.attention_broker.current();
                let weakest = formation
                    .members
                    .iter()
                    .filter(|m| m.is_operational())
                    .min_by(|a, b| {
                        let la = attn.load(&a.agent_id);
                        let lb = attn.load(&b.agent_id);
                        la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|m| m.agent_id);

                if let Some(agent) = weakest {
                    let rally_result = cascade::attempt_self_rally(
                        &formation.rally,
                        &formation.attention_broker,
                        &mut formation.momentum,
                        agent,
                    );
                    match &rally_result {
                        RallyResult::StabilizedWithCost { tokens_remaining } => {
                            tracing::info!(
                                id = %formation_id,
                                tokens_remaining,
                                "manual rally: formation stabilized"
                            );
                            let rally_row = springtale_store::FormationRallyRow {
                                formation_id: formation.id.0.to_string(),
                                tokens_remaining: formation.rally.tokens.remaining() as i64,
                                max_tokens: formation.rally.tokens.max() as i64,
                            };
                            if let Err(e) =
                                bot.store.upsert_formation_rally(&rally_row).await
                            {
                                tracing::warn!(error = %e, "failed to persist rally state");
                            }
                        }
                        RallyResult::EscalateToOrchestrator { reason } => {
                            tracing::error!(
                                id = %formation_id,
                                reason = %reason,
                                "manual rally: exhausted"
                            );
                        }
                        RallyResult::Recovered => {
                            tracing::info!(id = %formation_id, "manual rally: recovered");
                        }
                    }
                } else {
                    tracing::warn!(id = %formation_id, "manual rally: no operational members");
                }
            } else {
                tracing::warn!(id = %formation_id, "formation not found for rally");
            }
        }
    }
}
