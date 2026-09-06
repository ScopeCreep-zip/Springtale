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

use crate::cooperation::formation::Formation;
use crate::cooperation::lifecycle;
use crate::runtime::lifecycle::Bot;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::command::FormationCommand;
use springtale_cooperation::rally::{RallyResult, cascade};

/// Guard-mode veto for destructive/disruptive formation commands (finding
/// 78, `formations.md`): guard blocks Dissolve, ChangeIntent, RemoveMember,
/// and Rally even when momentum/autonomy would otherwise allow them.
/// Mirrors the existing `Recruit` arm's `constraints.guard_mode` check.
fn guarded(formation: &Formation, verb: &str) -> bool {
    if formation.constraints.guard_mode {
        tracing::info!(
            id = %formation.id.0,
            verb,
            "denied — guard mode engaged; run formation:guard first"
        );
        true
    } else {
        false
    }
}

/// Engage or disengage guard mode on a live formation and republish the
/// formation context so members see the new constraint on their next read.
///
/// The `guard:{formation_id}` config row is the durable copy (read back into
/// `constraints.guard_mode` at deploy); this is the live copy that
/// [`guarded`] enforces. `operations::config::toggle_formation_guard` writes
/// the row and posts `FormationCommand::SetGuard` together, so engaging guard
/// protects the formation immediately rather than at the next redeploy.
fn set_guard(formation: &mut Formation, engaged: bool) {
    formation.constraints.guard_mode = engaged;
    formation.broadcast_context();
}

/// Failures a formation recorded over its whole life, for the dissolve
/// outcome that gossip and the knowledge store publish.
///
/// The momentum FSM is the only place that counts a formation's failures:
/// `record_interference` bumps `interference_total` on every interference and
/// never resets it, while `interference_count` and `consecutive_successes` are
/// per-clean-run and reset on every break (Patapon combo). A dissolve is a
/// lifetime summary, so `interference_total` is the count that matches
/// `success_count`'s question — "how did this formation do" — and a hardcoded
/// zero read as "nothing ever failed here", which skewed the retrieval
/// scorer's success/total ratio in `cooperation::lifecycle`.
fn dissolve_failure_count(momentum: &springtale_cooperation::momentum::MomentumState) -> u32 {
    momentum.interference_total
}

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
        FormationCommand::Dissolve {
            formation_id,
            reason,
        } => {
            let mut formations = bot.formations.write().await;
            // Persist mental-model state BEFORE dropping the formation so
            // accumulated conventions / patterns / vocabulary survive the
            // dissolve (`COOPERATION.md §21`).
            if let Some(f) = formations.iter().find(|f| f.id == formation_id) {
                if guarded(f, "dissolve") {
                    return;
                }
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
                        failure_count: dissolve_failure_count(&f.momentum),
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
                        failure_count: dissolve_failure_count(&f.momentum),
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
        FormationCommand::SetGuard {
            formation_id,
            engaged,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                set_guard(formation, engaged);
                tracing::info!(id = %formation_id, engaged, "formation guard mode set");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for SetGuard");
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
        FormationCommand::ChangeIntent {
            formation_id,
            intent,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                if guarded(formation, "intent") {
                    return;
                }
                crate::orchestrator::intent::apply_intent(formation, intent);
                tracing::info!(id = %formation_id, "formation intent updated");
            } else {
                tracing::warn!(id = %formation_id, "formation not found for intent change");
            }
        }
        FormationCommand::ProposeIntentChange {
            formation_id,
            intent,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                // §5.5 source 2 / §7 capability table: formation
                // self-governance requires earned Fever momentum.
                if !formation.momentum.can_consensus() {
                    tracing::info!(
                        id = %formation_id,
                        tier = ?formation.momentum.tier,
                        "intent-change proposal denied — consensus requires Fever tier"
                    );
                } else {
                    let voters: Vec<AgentId> = formation
                        .members
                        .iter()
                        .filter(|m| m.is_operational())
                        .map(|m| m.agent_id)
                        .collect();
                    let voter_count = voters.len() as u32;
                    use springtale_cooperation::consensus::{DecisionDescriptor, DecisionSubject};
                    let vote_id = formation.consensus.propose(
                        DecisionDescriptor {
                            description: format!("change intent to {intent:?}"),
                            options: vec!["approve".into(), "deny".into()],
                            required_participants: voter_count,
                            subject: DecisionSubject::IntentChange { proposed: intent },
                        },
                        std::time::Duration::from_secs(10),
                        &voters,
                        1,
                    );
                    tracing::info!(
                        id = %formation_id,
                        vote_id = %vote_id,
                        voters = voter_count,
                        "intent-change consensus vote opened (joint-intention protocol)"
                    );
                }
            } else {
                tracing::warn!(id = %formation_id, "formation not found for ProposeIntentChange");
            }
        }
        FormationCommand::CastVote {
            formation_id,
            vote_id,
            voter,
            approve,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                use springtale_cooperation::consensus::VoteChoice;
                // Options are ["approve", "deny"] by §11 convention.
                let choice = VoteChoice::Option(if approve { 0 } else { 1 });
                match formation.consensus.vote(&vote_id, voter, choice) {
                    Ok(()) => tracing::info!(
                        id = %formation_id,
                        vote = %vote_id,
                        voter = %voter,
                        approve,
                        "ballot cast"
                    ),
                    Err(e) => tracing::warn!(
                        id = %formation_id,
                        vote = %vote_id,
                        error = %e,
                        "ballot rejected"
                    ),
                }
            } else {
                tracing::warn!(id = %formation_id, "formation not found for CastVote");
            }
        }
        FormationCommand::AddMember {
            formation_id,
            connector_name,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                let agent_id = AgentId::new();
                let member = crate::cooperation::formation::FormationMember::from_strings(
                    agent_id,
                    vec![connector_name.clone()],
                );
                formation.join(member);
                tracing::info!(
                    id = %formation_id,
                    connector = %connector_name,
                    "member added to formation"
                );
            } else {
                tracing::warn!(id = %formation_id, "formation not found for AddMember");
            }
        }
        FormationCommand::Recruit {
            formation_id,
            connector_name,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                // §7 momentum unlock: recruit is only available once the
                // formation has earned Fever. Guard mode vetoes it.
                if !formation.momentum.can_recruit() {
                    tracing::info!(
                        id = %formation_id,
                        tier = ?formation.momentum.tier,
                        "recruit denied — formation has not earned Fever tier"
                    );
                } else if guarded(formation, "recruit") {
                    // `guarded` already logged the denial.
                } else {
                    let member = crate::cooperation::formation::FormationMember::from_strings(
                        AgentId::new(),
                        vec![connector_name.clone()],
                    );
                    formation.join(member);
                    tracing::info!(
                        id = %formation_id,
                        connector = %connector_name,
                        "formation recruited a new member at Fever tier"
                    );
                }
            } else {
                tracing::warn!(id = %formation_id, "formation not found for Recruit");
            }
        }
        FormationCommand::RemoveMember {
            formation_id,
            connector_name,
        } => {
            let mut formations = bot.formations.write().await;
            if let Some(formation) = formations.iter_mut().find(|f| f.id == formation_id) {
                if guarded(formation, "remove_member") {
                    return;
                }
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
                if guarded(formation, "rally") {
                    return;
                }
                // Manual rally — rally around the member carrying the most
                // attention, which is the one a rally can relieve.
                // `attempt_self_rally` calls `attention.release(agent, 0.2)`
                // so the others absorb that load (Army of Two aggro shift),
                // so releasing the least-loaded member's load does nothing.
                // The cascade-driven path (`tick_steps/check_cascade.rs`)
                // picks the member whose report missed the intent; the
                // operator-initiated path has no reports to hand, and load is
                // the standing measure of who is struggling.
                let attn = formation.attention_broker.current();
                let target = rally_target(&formation.members, &attn);

                if let Some(agent) = target {
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
                            if let Err(e) = bot.store.upsert_formation_rally(&rally_row).await {
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

/// The member a manual rally should relieve: the operational member
/// carrying the most attention.
///
/// `cascade::attempt_self_rally` releases a fifth of the target's attention
/// so the rest of the formation absorbs it, the aggro shift from Army of
/// Two that COOPERATION.md cites. Releasing load from the member that has
/// the least of it moves nothing, which is what this path used to do. The
/// cascade-driven path picks the member whose report missed the intent; an
/// operator-initiated rally has no reports to hand, so standing load is the
/// measure of who is struggling.
fn rally_target(
    members: &[crate::cooperation::formation::FormationMember],
    attn: &springtale_cooperation::attention::AttentionEconomy,
) -> Option<AgentId> {
    members
        .iter()
        .filter(|m| m.is_operational())
        .max_by(|a, b| {
            let la = attn.load(&a.agent_id);
            let lb = attn.load(&b.agent_id);
            la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|m| m.agent_id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_cooperation::attention::AttentionEconomy;

    fn member(id: AgentId) -> crate::cooperation::formation::FormationMember {
        crate::cooperation::formation::FormationMember::new(id, vec!["connector-telegram".into()])
    }

    /// Toggling guard on a *live* formation blocks a guarded verb straight
    /// away. The guard flag used to be read only at deploy, so engaging it
    /// left the running formation unprotected until a redeploy;
    /// `FormationCommand::SetGuard` (posted by `toggle_formation_guard`) lands
    /// here instead.
    #[test]
    fn test_set_guard_blocks_guarded_verbs_without_redeploy() {
        use springtale_cooperation::cadence::IntentPattern;
        use springtale_cooperation::types::FormationConstraints;

        // Deployed with guard off — the default every spawn used to get.
        let mut formation = crate::cooperation::formation::Formation::new_disconnected(
            vec![member(AgentId::new())],
            IntentPattern::Stabilize {
                reason: "test".into(),
            },
            FormationConstraints::default(),
        );
        assert!(!formation.constraints.guard_mode);
        for verb in ["dissolve", "intent", "remove_member", "rally", "recruit"] {
            assert!(!guarded(&formation, verb), "{verb} blocked before toggle");
        }

        set_guard(&mut formation, true);

        assert!(formation.constraints.guard_mode);
        for verb in ["dissolve", "intent", "remove_member", "rally", "recruit"] {
            assert!(guarded(&formation, verb), "{verb} not blocked under guard");
        }

        set_guard(&mut formation, false);
        assert!(!guarded(&formation, "dissolve"));
    }

    /// The dissolve outcome reports the failures the formation actually
    /// recorded, not a hardcoded zero.
    #[test]
    fn test_dissolve_failure_count_matches_recorded_interferences() {
        use springtale_cooperation::momentum::MomentumState;

        let mut momentum = MomentumState::default();
        assert_eq!(dissolve_failure_count(&momentum), 0);

        momentum.record_interference();
        momentum.record_success();
        momentum.record_interference();
        momentum.record_interference();

        // Three interferences over the formation's life; the per-run counter
        // reset behind each one, which is why it cannot be the source.
        assert_eq!(momentum.interference_total, 3);
        assert_eq!(momentum.interference_count, 0);
        assert_eq!(dissolve_failure_count(&momentum), 3);
    }

    /// A rally relieves the member carrying the most attention. Picking the
    /// least-loaded member, as this path once did, releases load that was
    /// never there.
    #[test]
    fn test_rally_target_is_the_most_loaded_member() {
        let busy = AgentId::new();
        let idle = AgentId::new();
        let members = vec![member(busy), member(idle)];

        // Equal shares, then push attention onto one member.
        let mut economy = AttentionEconomy::new(&[busy, idle]);
        economy.shift_toward(&busy, 0.4);

        assert_eq!(rally_target(&members, &economy), Some(busy));
    }
}
