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
    /// Manually trigger self-rally (from RALLY button in UI).
    Rally { formation_id: FormationId },
    /// Remove a member (connector) from a live formation.
    RemoveMember {
        formation_id: FormationId,
        connector_name: String,
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
            IntentPattern::Reconnoiter { target: "unspecified".to_owned() }
        },
        "Execute" => IntentPattern::Execute { plan_id: None },
        "Stabilize" => {
            tracing::debug!("parse_intent: Stabilize without reason metadata");
            IntentPattern::Stabilize { reason: "unspecified".to_owned() }
        },
        "Surge" => {
            tracing::debug!("parse_intent: Surge without objective metadata");
            IntentPattern::Surge { objective: "unspecified".to_owned() }
        },
        _ => IntentPattern::Stabilize {
            reason: format!("unknown intent: {intent_str}"),
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
        assert!(matches!(
            parse_intent("Surge"),
            IntentPattern::Surge { .. }
        ));
    }

    #[test]
    fn test_parse_unknown_defaults_to_stabilize() {
        let result = parse_intent("FooBar");
        assert!(matches!(result, IntentPattern::Stabilize { reason } if reason.contains("unknown")));
    }

    #[test]
    fn test_parse_empty_defaults_to_stabilize() {
        let result = parse_intent("");
        assert!(matches!(result, IntentPattern::Stabilize { reason } if reason.contains("unknown")));
    }
}
