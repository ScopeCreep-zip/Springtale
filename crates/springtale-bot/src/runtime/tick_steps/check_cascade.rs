//! Step 9 — cascade detection + self-rally (`COOPERATION.md §15`).
//!
//! When a tick has any failure, we look for a multi-agent failure pattern
//! (cascade) via `cascade::detect_cascade`. If detected, the formation
//! attempts self-rally on the weakest agent — redistribute attention,
//! consume one rally token, and try again. If rally tokens are exhausted
//! the next pipeline step (`check_interventions`, B1) escalates to the
//! orchestrator intervention layer.
//!
//! Rally state is persisted on consumption so token counts survive restarts.

use std::collections::HashMap;
use std::time::Duration;

use crate::cooperation::formation::Formation;
use springtale_cooperation::awareness::LocalAwareness;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::contract_net::{
    coordinator, types::CallForProposals, RoundOutcome,
};
use springtale_cooperation::rally::{RallyResult, cascade};
use springtale_cooperation::tick_processor::FormationTickResult;
use springtale_store::{FormationRallyRow, StorageBackend};
use uuid::Uuid;

pub async fn run(
    formation: &mut Formation,
    result: &FormationTickResult,
    store: &dyn StorageBackend,
) {
    if result.all_succeeded {
        // Successful tick clears the cascade streak so the L6 evaluator
        // (`check_interventions.rs`) doesn't trip on a long-resolved
        // sequence of past failures.
        formation.cascade_hit_streak = 0;
        return;
    }

    let awareness_map: HashMap<AgentId, &LocalAwareness> = formation
        .members
        .iter()
        .filter(|m| m.is_operational())
        .map(|m| (m.agent_id, &m.awareness))
        .collect();

    let Some(risk) = cascade::detect_cascade(&awareness_map, result) else {
        return;
    };

    // Increment the streak — this is the cascade_hits signal the L6
    // intervention evaluator reads next. Saturating add so a perpetually
    // unhealthy formation never wraps.
    formation.cascade_hit_streak = formation.cascade_hit_streak.saturating_add(1);

    tracing::warn!(formation = %formation.id.0, ?risk, "cascade risk detected");

    let Some(failing_agent) = result
        .reports
        .iter()
        .find(|r| r.intent_alignment <= 0.5)
        .map(|r| r.agent_id)
    else {
        return;
    };

    let rally_result = cascade::attempt_self_rally(
        &formation.rally,
        &formation.attention_broker,
        &mut formation.momentum,
        failing_agent,
    );
    log_rally_result(&formation.id.0.to_string(), &rally_result);

    // Persist rally state after token consumption so restarts see the
    // updated count.
    let rally_row = FormationRallyRow {
        formation_id: formation.id.0.to_string(),
        tokens_remaining: formation.rally.tokens.remaining() as i64,
        max_tokens: formation.rally.tokens.max() as i64,
    };
    if let Err(e) = store.upsert_formation_rally(&rally_row).await {
        tracing::warn!(
            formation_id = %formation.id.0,
            error = %e,
            "failed to persist rally state"
        );
    }

    // B2 — L4 Contract Net: when self-rally exhausted and the failing
    // agent had a task in flight, broadcast a CFP so a peer can take it
    // over. Only runs on `EscalateToOrchestrator` (rally tokens spent) so
    // CFPs aren't broadcast for transient stumbles.
    if matches!(rally_result, RallyResult::EscalateToOrchestrator { .. }) {
        run_takeover_cfp(formation, failing_agent).await;
    }
}

/// Build a CFP for the failing agent's active task and run a Contract Net
/// round. Returns silently on no active task / no bids (logged) — the L6
/// intervention layer handles persistent escalation separately.
async fn run_takeover_cfp(formation: &mut Formation, failing_agent: AgentId) {
    // Snapshot what we need without holding any locks across the round.
    let (task, initiator_agent) = {
        let Some(member) = formation.member(&failing_agent) else { return };
        let Some(active) = member.active_task.as_ref() else { return };
        (active.task.clone(), failing_agent)
    };
    let cfp = CallForProposals {
        id: Uuid::new_v4(),
        initiator: initiator_agent,
        required_capability: Some(task.target_connector.clone()),
        task,
        deadline: Duration::from_millis(50),
        scoring_hint: Some("rally-exhaustion-takeover".to_owned()),
    };
    let cfp_id = cfp.id;
    let tier = formation.momentum.tier;
    let initiator = formation.cfp_initiator.clone();
    let mut handle = initiator.lock().await;
    let outcome = coordinator::run_round(&mut handle, cfp, tier).await;
    drop(handle);
    match outcome {
        RoundOutcome::Awarded { award, bids_seen } => {
            tracing::info!(
                formation = %formation.id.0,
                cfp = %cfp_id,
                winner = %award.winner.0,
                utility = award.utility,
                bids_seen,
                "CFP takeover awarded"
            );
        }
        RoundOutcome::NoBids => {
            tracing::warn!(formation = %formation.id.0, cfp = %cfp_id, "CFP takeover: no bids");
        }
        RoundOutcome::Unauthorized(reason) => {
            tracing::debug!(
                formation = %formation.id.0,
                cfp = %cfp_id,
                ?reason,
                "CFP takeover unauthorized at current tier"
            );
        }
        RoundOutcome::AnnounceFailed | RoundOutcome::NotifyFailed => {
            tracing::error!(
                formation = %formation.id.0,
                cfp = %cfp_id,
                ?outcome,
                "CFP takeover transport failure"
            );
        }
    }
}

fn log_rally_result(formation_id: &str, result: &RallyResult) {
    match result {
        RallyResult::StabilizedWithCost { tokens_remaining } => {
            tracing::info!(formation = formation_id, tokens_remaining, "formation self-rallied");
        }
        RallyResult::EscalateToOrchestrator { reason } => {
            tracing::error!(
                formation = formation_id,
                reason = %reason,
                "formation rally exhausted — escalating"
            );
        }
        RallyResult::Recovered => {
            tracing::info!(formation = formation_id, "formation recovered");
        }
    }
}
