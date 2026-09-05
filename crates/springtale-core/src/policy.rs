//! Approval policy and autonomy level — the two knobs the sentinel reads
//! alongside an action's impact classification.
//!
//! They live in `springtale-core` (not `springtale-cooperation`) because
//! `springtale-sentinel` depends on core only, per the crate dependency
//! rules. `springtale-cooperation` re-exports both from `types` so the
//! existing paths keep compiling.

use serde::{Deserialize, Serialize};
use specta::Type;

/// How a formation gates actions by risk class (§3.3).
///
/// Modeled on Microsoft Agent Governance Toolkit's 3-category classification
/// (DESTRUCTIVE_DATA / DATA_EXFILTRATION / PRIVILEGE_ESCALATION) extended
/// with Springtale's Sentinel integration and the OpenAI Agents SDK
/// `needsApproval` pattern (pause → approve/reject → resume).
///
/// Maps to RTS fire stances:
///   AutoApprove     = AoE "Aggressive" / Spring "Fire at Will"
///   ApproveOnce     = AoE "Defensive" / OpenAI `alwaysApprove: true`
///   AlwaysRequire   = AoE "Stand Ground" / Claude Code modification gate
///   RequireConsensus = AoE "No Attack" / Microsoft AGT Ring 0 quorum
///
/// `Default` is `AutoApprove` — the value an `ExecutionContext` carries
/// for global (non-formation) rules. Destructive actions still hit the
/// approval gate under `AutoApprove`; the policy only decides *how* the
/// gate is consulted (every time, once per session, or by consensus).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ApprovalPolicy {
    /// Auto-approve. Read-only, non-mutating actions.
    #[default]
    AutoApprove,
    /// Approve once per action type, then remember for the session.
    ApproveOnce,
    /// Require explicit approval every time.
    AlwaysRequire,
    /// Require consensus vote from formation members (§11).
    RequireConsensus,
}

/// Agent autonomy level — RTS stance mapped to bot behavior (§3.3, §7).
///
/// Cross-referenced across 5 RTS games:
///   Observe         = AoE "No Attack"    = SC "Stop"        = 0AD "Passive"
///   Suggest         = AoE "Stand Ground" = SC "Hold Pos"    = 0AD "Stand Ground"
///   ActWithApproval = AoE "Defensive"    = SC "Patrol"      = 0AD "Defensive"
///   ActAutonomously = AoE "Aggressive"   = SC "Attack-Move" = 0AD "Aggressive"
///
/// Also maps to Microsoft AGT trust rings:
///   Observe = Ring 3 (read-only sandbox)
///   Suggest = Ring 2 (standard tool access)
///   ActWithApproval = Ring 1 (elevated, cross-agent)
///   ActAutonomously = Ring 0 (full system access)
///
/// `Default` is `ActAutonomously` — the value an `ExecutionContext`
/// carries for global (non-formation) rules, which have no member whose
/// stance could hold them. [`AutonomyLevel::parse`] still falls back to
/// `Suggest` for unrecognized *stored* input.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
pub enum AutonomyLevel {
    /// Holdfire + holdpos. Watch and report only.
    Observe,
    /// Returnfire + holdpos. Scan, report what WOULD be done, don't claim.
    Suggest,
    /// Fireatwill + maneuver. Scan, claim, but hold for human/consensus OK.
    ActWithApproval,
    /// Fireatwill + roam. Full autonomy — scan, claim, execute.
    #[default]
    ActAutonomously,
}

impl AutonomyLevel {
    /// Parse from the string form stored in config store. Falls back to
    /// `Suggest` on unrecognized input (safe default — reports but doesn't act).
    pub fn parse(s: &str) -> Self {
        match s {
            "observe" => Self::Observe,
            "suggest" => Self::Suggest,
            "act-with-approval" => Self::ActWithApproval,
            "act-autonomously" => Self::ActAutonomously,
            _ => Self::Suggest,
        }
    }

    /// Serialize to the string form for config store persistence.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Suggest => "suggest",
            Self::ActWithApproval => "act-with-approval",
            Self::ActAutonomously => "act-autonomously",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_policy_default_is_auto_approve() {
        assert_eq!(ApprovalPolicy::default(), ApprovalPolicy::AutoApprove);
    }

    #[test]
    fn test_autonomy_level_default_is_act_autonomously() {
        assert_eq!(AutonomyLevel::default(), AutonomyLevel::ActAutonomously);
    }

    #[test]
    fn test_autonomy_level_parse_unknown_falls_back_to_suggest() {
        assert_eq!(AutonomyLevel::parse("garbage"), AutonomyLevel::Suggest);
    }
}
