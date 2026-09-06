//! Formation commands — sent from runtime operations to the bot event loop.
//!
//! Runtime writes these; bot reads and materializes/removes formations.
//! This is the ONLY bridge between the persistence layer (runtime operations)
//! and the live formation state (bot event loop).

use crate::cadence::IntentPattern;
use crate::types::FormationId;

/// Commands sent from runtime operations to the bot event loop.
pub enum FormationCommand {
    /// Materialize a live Formation struct from database rows.
    Deploy { formation_id: FormationId },
    /// Pause a live formation (stops tick processing).
    Pause { formation_id: FormationId },
    /// Resume a paused formation.
    Resume { formation_id: FormationId },
    /// Engage or disengage guard mode on a live formation.
    ///
    /// Guard mode lives in two places: the `guard:{formation_id}` config row
    /// (durable, read back at deploy into `constraints.guard_mode`) and the
    /// live `Formation` the bot ticks. `operations::config::toggle_formation_guard`
    /// writes the row and posts this command in the same call, so the two can
    /// never disagree and the toggle takes effect without a redeploy.
    SetGuard {
        formation_id: FormationId,
        engaged: bool,
    },
    /// Dissolve a formation and remove from memory.
    Dissolve {
        formation_id: FormationId,
        reason: String,
    },
    /// Change the formation's intent pattern.
    ChangeIntent {
        formation_id: FormationId,
        intent: IntentPattern,
    },
    /// Add a member (connector) to a live formation.
    AddMember {
        formation_id: FormationId,
        connector_name: String,
    },
    /// Recruit a new member at Fever tier (§7 momentum unlock). Like
    /// `AddMember`, but the bot only honors it when the formation has earned
    /// Fever momentum (`MomentumState::can_recruit`) and guard mode is off — the
    /// recruit is the formation's own earned capability, not an operator add.
    Recruit {
        formation_id: FormationId,
        connector_name: String,
    },
    /// Manually trigger self-rally (from RALLY button in UI).
    Rally { formation_id: FormationId },
    /// Remove a member (connector) from a live formation.
    RemoveMember {
        formation_id: FormationId,
        connector_name: String,
    },
    /// §5.5 source 2 — formation self-governance: open a consensus vote
    /// to change the formation's intent. Joint Intention Theory: the
    /// joint goal changes only via mutual belief (a vote), never by one
    /// member's private belief. Honored only at Fever
    /// (`MomentumState::can_consensus`); below that the command is
    /// rejected with a log line. Contrast `ChangeIntent`, which is the
    /// §3.2 orchestrator/user path and applies immediately — the user
    /// outranks the formation.
    ProposeIntentChange {
        formation_id: FormationId,
        intent: IntentPattern,
    },
    /// Cast a ballot on an open consensus vote (§11). The vote resolves
    /// in the `resolve_consensus` tick step once quorum, override, or
    /// deadline is reached. `approve = false` votes for the "deny"
    /// option on two-option votes.
    CastVote {
        formation_id: FormationId,
        vote_id: uuid::Uuid,
        voter: crate::cadence::AgentId,
        approve: bool,
    },
}

/// Parse an intent string (as stored in DB) into an IntentPattern.
///
/// Used by runtime operations to convert the stored string into a typed
/// command for the bot event loop. Single source of truth for this conversion.
pub fn parse_intent(intent_str: &str) -> IntentPattern {
    match intent_str {
        "Reconnoiter" => {
            tracing::debug!("parse_intent: Reconnoiter without target metadata");
            IntentPattern::Reconnoiter {
                target: "unspecified".into(),
            }
        }
        "Execute" => IntentPattern::Execute { plan_id: None },
        "Stabilize" => {
            tracing::debug!("parse_intent: Stabilize without reason metadata");
            IntentPattern::Stabilize {
                reason: "unspecified".into(),
            }
        }
        "Surge" => {
            tracing::debug!("parse_intent: Surge without objective metadata");
            IntentPattern::Surge {
                objective: "unspecified".into(),
            }
        }
        _ => IntentPattern::Stabilize {
            reason: format!("unknown intent: {intent_str}").into(),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reconnoiter() {
        assert!(matches!(
            parse_intent("Reconnoiter"),
            IntentPattern::Reconnoiter { .. }
        ));
    }

    #[test]
    fn test_parse_execute() {
        assert!(matches!(
            parse_intent("Execute"),
            IntentPattern::Execute { plan_id: None }
        ));
    }

    #[test]
    fn test_parse_stabilize() {
        assert!(matches!(
            parse_intent("Stabilize"),
            IntentPattern::Stabilize { .. }
        ));
    }

    #[test]
    fn test_parse_surge() {
        assert!(matches!(parse_intent("Surge"), IntentPattern::Surge { .. }));
    }

    #[test]
    fn test_parse_unknown_defaults_to_stabilize() {
        let result = parse_intent("FooBar");
        assert!(
            matches!(result, IntentPattern::Stabilize { reason } if reason.contains("unknown"))
        );
    }

    #[test]
    fn test_parse_empty_defaults_to_stabilize() {
        let result = parse_intent("");
        assert!(
            matches!(result, IntentPattern::Stabilize { reason } if reason.contains("unknown"))
        );
    }
}
