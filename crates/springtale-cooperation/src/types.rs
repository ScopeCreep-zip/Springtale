//! Pure data types extracted from formation — shared across cooperation modules.
//!
//! These types have zero external dependencies beyond serde/uuid.
//! They live here (not in formation.rs) because they're referenced by
//! awareness, recovery, transformation, and other cooperation modules
//! that must not depend on springtale-ai or the orchestrator.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;

/// Typed fuel amount — wraps a raw `u64` so fuel values are unambiguous.
///
/// `FuelBudget` in the bot crate is the runtime tracker; this newtype is
/// the cooperation-layer representation that travels in `RecoveryCost` and
/// `FormationConstraints` without pulling in bot dependencies.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
pub struct FuelAmount(pub u64);

impl From<u64> for FuelAmount {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl std::fmt::Display for FuelAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Workspace/environment key — identifies entries in the shared workspace.
///
/// Used in interference detection (read_set/write_set intersection) and
/// environment-mediated handoffs. Per Kubernetes: typed key prevents mixing
/// workspace keys with unrelated strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct WorkspaceKey(pub String);

impl From<&str> for WorkspaceKey {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}
impl From<String> for WorkspaceKey {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl std::fmt::Display for WorkspaceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl AsRef<str> for WorkspaceKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl std::ops::Deref for WorkspaceKey {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl PartialEq<str> for WorkspaceKey {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

/// Shared resource identifier — DRG Red Sugar, Helldivers resupply pod.
///
/// Used in BroadcastTrigger::ResourceFound, EnvironmentalRecovery,
/// ResourceInvestment. Distinct from WorkspaceKey (environment entries)
/// and CapabilityDecl (agent capabilities).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct ResourceId(pub String);

impl From<&str> for ResourceId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}
impl From<String> for ResourceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl PartialEq<str> for ResourceId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

/// Cooperation pattern trigger — identifies what triggers a learned pattern.
///
/// MH: "monster_topple", Siege: "post_plant_retake".
/// Used in CooperationPattern.trigger and mental model graph Concept::Pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct PatternId(pub String);

impl From<&str> for PatternId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}
impl From<String> for PatternId {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl std::fmt::Display for PatternId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl PartialEq<str> for PatternId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

/// Unique identifier for a formation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct FormationId(pub uuid::Uuid);

impl Default for FormationId {
    fn default() -> Self {
        Self::new()
    }
}

impl FormationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }
}

impl std::fmt::Display for FormationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Health state of an agent in a formation.
///
/// Per COOPERATION.pdf §18.3 (L4D-inspired escalating fragility):
/// Quick-fix recovery leaves the agent degraded. Proper recovery
/// restores full capability. Repeated quick-fixes increase fragility.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub enum AgentHealth {
    /// Full operational capability.
    #[default]
    Operational,
    /// Reduced capability after quick-fix recovery.
    Degraded { recovery_count: u32 },
    /// Incapacitated — needs peer revive (L4D downed state).
    Incapacitated,
    /// Disconnected/dead — can be redeployed (Helldivers reinforce).
    Dead { recoverable: bool },
}

/// Dynamic role of an agent — emerges from context, not assignment.
///
/// Per §23 (Specialization vs Generalization): "The role_hint in
/// the composer (§3.1) should bias, not mandate." Roles are
/// tendencies, not locks. Like Army of Two's weapon-based specialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub enum DynamicRole {
    /// Default — role not yet determined.
    #[default]
    Unassigned,
    /// Primary task executor.
    Primary { task: CapabilityDecl },
    /// Support role (emerged from context, not assigned).
    Support { supporting: AgentId },
    /// Information gatherer (Siege dead→intel pattern).
    Information,
    /// Custom role (connector-specific).
    Custom { name: String },
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ApprovalPolicy {
    /// Auto-approve. Read-only, non-mutating actions.
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
pub enum AutonomyLevel {
    /// Holdfire + holdpos. Watch and report only.
    Observe,
    /// Returnfire + holdpos. Scan, report what WOULD be done, don't claim.
    #[default]
    Suggest,
    /// Fireatwill + maneuver. Scan, claim, but hold for human/consensus OK.
    ActWithApproval,
    /// Fireatwill + roam. Full autonomy — scan, claim, execute.
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

/// Constraints on formation behavior — set by the orchestrator (§3.3).
///
/// Per Total War: supply lines are per-army, not per-unit. The `fuel_budget`
/// is the initial allocation consumed by actions during the formation's
/// lifetime. `destructive_action_policy` gates high-risk actions through
/// approval. `autonomy_ceiling` caps how autonomous any member can be.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FormationConstraints {
    /// Maximum time the formation can run.
    pub timeout: Duration,
    /// Maximum concurrent actions across all members.
    pub max_concurrent_actions: usize,
    /// Whether the formation is in guard mode (Total War: don't pursue).
    pub guard_mode: bool,
    /// Initial fuel allocation for the formation (Total War supply model).
    pub fuel_budget: FuelAmount,
    /// How destructive actions are gated (Sentinel integration point).
    pub destructive_action_policy: ApprovalPolicy,
    /// Maximum autonomy any member can reach, regardless of their individual
    /// setting. A ceiling of `Suggest` overrides a member set to `ActAutonomously`.
    pub autonomy_ceiling: AutonomyLevel,
}

impl Default for FormationConstraints {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_concurrent_actions: 8,
            guard_mode: false,
            fuel_budget: FuelAmount(100_000),
            destructive_action_policy: ApprovalPolicy::AlwaysRequire,
            autonomy_ceiling: AutonomyLevel::ActAutonomously,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod type_tests {
    use super::*;

    #[test]
    fn autonomy_level_parse_roundtrip() {
        for level in [
            AutonomyLevel::Observe,
            AutonomyLevel::Suggest,
            AutonomyLevel::ActWithApproval,
            AutonomyLevel::ActAutonomously,
        ] {
            assert_eq!(AutonomyLevel::parse(level.as_str()), level);
        }
    }

    #[test]
    fn autonomy_level_unknown_falls_back_to_suggest() {
        assert_eq!(AutonomyLevel::parse("garbage"), AutonomyLevel::Suggest);
        assert_eq!(AutonomyLevel::parse(""), AutonomyLevel::Suggest);
    }

    #[test]
    fn autonomy_level_ordering() {
        assert!(AutonomyLevel::Observe < AutonomyLevel::Suggest);
        assert!(AutonomyLevel::Suggest < AutonomyLevel::ActWithApproval);
        assert!(AutonomyLevel::ActWithApproval < AutonomyLevel::ActAutonomously);
    }

    #[test]
    fn approval_policy_default_constraints_are_safe() {
        let c = FormationConstraints::default();
        assert_eq!(c.destructive_action_policy, ApprovalPolicy::AlwaysRequire);
        assert_eq!(c.autonomy_ceiling, AutonomyLevel::ActAutonomously);
        assert_eq!(c.fuel_budget, FuelAmount(100_000));
    }
}
