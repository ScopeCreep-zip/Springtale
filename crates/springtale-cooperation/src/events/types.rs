//! `CooperationEvent` taxonomy + `CooperationEventEnvelope` (Phase H1).
//!
//! Cooperation events are the user-observable side of internal cooperation
//! state changes — interventions firing, sacrifices yielded, votes opened,
//! roles transformed, members marked down, supervisor escalations, pacing
//! phase changes, cascade hits, recovery actions, surface deposits,
//! interference events, supervision actions, transformation events, replan
//! stalls, commit barriers. Without this taxonomy these only live in
//! `tracing::info!` calls, invisible to the colony canvas.
//!
//! Mirrors the `EventEntry` shape that `apps/springtaled/src/api/events_stream.rs`
//! already pipes through SSE (`/events/stream`) — same broadcast → filter →
//! serialize chain. New stream endpoint `/cooperation/events` (Phase H3) plus
//! Tauri Channel<CooperationEventEnvelope> (Phase H4) carry these to UI.
//!
//! Design lineage:
//! - Bevy ECS `bevy_ecs::event::Events<T>` — typed enum + double-buffered
//!   ring; per-reader cursors so late readers don't miss events.
//! - Spring RTS Lua callins — `UnitFinished`, `UnitDamaged`, etc. as
//!   typed enumerated callouts; closest game-engine analog.
//! - Microsoft AutoGen v0.4 OpenTelemetry tracing — production reference
//!   for tracing-Layer-as-event-bus.
//!
//! Definition lives in `springtale-cooperation` (no bot-crate dep) so
//! `InterventionKind` is a coarse mirror of
//! `bot::orchestrator::intervention::types::Intervention` — the
//! cooperation crate stays bot-agnostic.

use serde::Serialize;
use uuid::Uuid;

use crate::cadence::AgentId;
use crate::types::FormationId;

/// Coarse-grained tag for L6 commander-override interventions.
///
/// Mirrors the four `Intervention` variants in
/// `springtale-bot::orchestrator::intervention::types::Intervention` —
/// enumerated locally so the events module doesn't depend on the bot
/// crate (which depends on this one).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "intervention", rename_all = "snake_case")]
pub enum InterventionKind {
    /// `Intervention::ChangeIntent` — replaced formation intent.
    ChangeIntent,
    /// `Intervention::InjectFuel` — refilled fuel budget.
    InjectFuel { amount: u64 },
    /// `Intervention::ForcedDissolve` — unrecoverable, tear down formation.
    ForcedDissolve,
    /// `Intervention::EscalateToUser` — orchestrator can't decide;
    /// surface to the human user.
    EscalateToUser,
}

/// Outcome of a consensus vote (§11).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoteOutcome {
    Approved,
    Denied,
    Timeout,
}

/// Coarse interference type — mirrors `interference::types::InterferenceType`
/// plus blackboard-claim races. Subscribers don't need the full struct,
/// just the kind for tally/badge display.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterferenceKind {
    ResourceConflict,
    ActionNegation,
    CollateralDamage,
    TaskAlreadyClaimed,
    DuplicateAction,
}

/// Compact summary of a CBBA replan outcome.
///
/// Distills `replan::cbba::orchestrator::ReplanOutcome` into a flat
/// JSON-friendly shape — full `ReplanOutcome` carries `HashMap<TaskId, AgentId>`
/// which is too verbose for the events stream.
#[derive(Debug, Clone, Serialize)]
pub struct ReplanOutcomeSummary {
    /// `"converged"` | `"stalled"` | `"unauthorized"`.
    pub status: &'static str,
    pub sweeps: u32,
    pub assigned: u32,
    pub unassigned: u32,
}

/// Cooperation event variant. 16 kinds covering every internal-state
/// change the user might want to see live.
///
/// Each variant uses snake_case in `kind`, matching the existing canvas
/// event format and SolidJS discriminated-union convention.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CooperationEvent {
    /// L6 intervention dispatched against a formation.
    InterventionFired {
        formation_id: FormationId,
        intervention: InterventionKind,
        summary: String,
    },
    /// Pacing phase transition (§22).
    PacingPhaseChanged {
        formation_id: FormationId,
        from: &'static str,
        to: &'static str,
    },
    /// Cascade detected this tick — streak counter incremented.
    CascadeHit {
        formation_id: FormationId,
        streak: u32,
        members_affected: u32,
    },
    /// Consensus vote opened — Fever-tier RequireConsensus action gated.
    ConsensusVoteOpened {
        formation_id: FormationId,
        vote_id: Uuid,
        deadline_ms: u64,
    },
    /// Consensus vote resolved (deadline drained or quorum reached).
    ConsensusVoteResolved {
        formation_id: FormationId,
        vote_id: Uuid,
        outcome: VoteOutcome,
    },
    /// Synchronized commit barrier moved between phases (§12).
    CommitPhaseChanged {
        formation_id: FormationId,
        barrier_id: Uuid,
        /// `"prepare"` | `"ready"` | `"countdown"` | `"execute"` |
        /// `"committed"` | `"aborted"`.
        phase: &'static str,
    },
    /// Voluntary self-yield from sacrifice step (§24).
    SacrificeYield {
        formation_id: FormationId,
        sacrificer: AgentId,
        beneficiary: AgentId,
        utility: f32,
    },
    /// Role transformation evaluated and applied (§14).
    RoleTransformed {
        formation_id: FormationId,
        agent: AgentId,
        from: String,
        to: String,
    },
    /// Supervisor marked a member as Liveness::Down.
    MemberMarkedDown {
        formation_id: FormationId,
        agent: AgentId,
        since_tick: u64,
    },
    /// Supervisor flagged escalation_pending (read by L6 next tick).
    SupervisorEscalated {
        formation_id: FormationId,
        reason: String,
    },
    /// Recovery action selected for a distressed agent (§18).
    RecoveryActionTaken {
        formation_id: FormationId,
        helper: AgentId,
        in_distress: AgentId,
        action: String,
    },
    /// Stigmergy surface deposited after successful action (§10).
    SurfaceDeposited {
        formation_id: FormationId,
        agent: AgentId,
        surface_kind: String,
        ttl_ms: u64,
    },
    /// Interference detected by tick processor (§13).
    InterferenceDetected {
        formation_id: FormationId,
        /// Renamed from `kind` because the outer `#[serde(tag = "kind")]`
        /// reserves that field name for the variant discriminator.
        interference_kind: InterferenceKind,
        agents: Vec<AgentId>,
    },
    /// L4 Contract Net round opened (cascade-driven capability auction).
    CfpRoundStarted {
        formation_id: FormationId,
        cfp_id: Uuid,
        capability: String,
    },
    /// L4 Contract Net round resolved.
    CfpRoundResolved {
        formation_id: FormationId,
        cfp_id: Uuid,
        winner: Option<AgentId>,
    },
    /// L5 CBBA replan requested by supervisor (`needs_replan = true`).
    CbbaReplanRequested {
        formation_id: FormationId,
        reason: String,
    },
    /// L5 CBBA replan resolved.
    CbbaReplanResolved {
        formation_id: FormationId,
        outcome: ReplanOutcomeSummary,
    },
}

/// Wire envelope for cooperation events — adds monotonic sequence + UTC
/// timestamp to every emitted event.
///
/// Matches the existing `EventEntry` SSE shape so the dashboard's existing
/// `BroadcastStream::filter_map` pattern works verbatim.
#[derive(Debug, Clone, Serialize)]
pub struct CooperationEventEnvelope {
    /// Per-bot monotonic sequence — frontend uses to detect missed events.
    pub seq: u64,
    /// UTC timestamp of emission.
    pub at: chrono::DateTime<chrono::Utc>,
    /// The event itself.
    pub event: CooperationEvent,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn event_serializes_with_kind_tag() {
        let env = CooperationEventEnvelope {
            seq: 1,
            at: chrono::Utc::now(),
            event: CooperationEvent::CascadeHit {
                formation_id: FormationId::new(),
                streak: 3,
                members_affected: 2,
            },
        };
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["event"]["kind"], "cascade_hit");
        assert_eq!(json["event"]["streak"], 3);
        assert_eq!(json["event"]["members_affected"], 2);
    }

    #[test]
    fn intervention_kind_nests_under_intervention_tag() {
        let env = CooperationEventEnvelope {
            seq: 2,
            at: chrono::Utc::now(),
            event: CooperationEvent::InterventionFired {
                formation_id: FormationId::new(),
                intervention: InterventionKind::EscalateToUser,
                summary: "rally exhausted".into(),
            },
        };
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["event"]["kind"], "intervention_fired");
        assert_eq!(json["event"]["intervention"]["intervention"], "escalate_to_user");
    }

    #[test]
    fn vote_outcome_round_trips() {
        let env = CooperationEventEnvelope {
            seq: 3,
            at: chrono::Utc::now(),
            event: CooperationEvent::ConsensusVoteResolved {
                formation_id: FormationId::new(),
                vote_id: Uuid::new_v4(),
                outcome: VoteOutcome::Approved,
            },
        };
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["event"]["outcome"], "approved");
    }
}
