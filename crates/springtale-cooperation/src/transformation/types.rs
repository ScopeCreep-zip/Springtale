//! Role transformation — agents change roles when capabilities are lost.
//!
//! Per COOPERATION.pdf §14:
//! Game sources: Siege dead→intel, Army of Two role oscillation,
//! It Takes Two chapter abilities.
//!
//! "When killed, players can switch to cameras and provide callouts.
//! Primary capability lost → role transforms to information agent."
//!
//! Dead agents aren't removed — they're transformed. Their continued
//! information contribution IS a form of the formation recovering
//! from their loss.

use crate::capability::CapabilityDecl;

/// How an agent's role transforms when its primary capability changes.
///
/// From COOPERATION.pdf §14:
/// ```text
/// pub enum RoleTransformation {
///     ToInformationAgent,
///     ToSupportAgent,
///     ReassignCapabilities(Vec<CapabilityDecl>),
/// }
/// ```
pub enum RoleTransformation {
    /// Primary capability lost. Becomes information-only.
    /// Siege: dead players switch to cameras and provide callouts.
    ToInformationAgent,

    /// Primary capability exhausted. Becomes support.
    /// Army of Two: low-aggro player shifts to overwatch.
    ToSupportAgent,

    /// Context changed. New tools assigned.
    /// It Takes Two: each chapter gives completely new abilities.
    ReassignCapabilities(Vec<CapabilityDecl>),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_transformation_variants() {
        let info = RoleTransformation::ToInformationAgent;
        assert!(matches!(info, RoleTransformation::ToInformationAgent));

        let support = RoleTransformation::ToSupportAgent;
        assert!(matches!(support, RoleTransformation::ToSupportAgent));

        let reassign =
            RoleTransformation::ReassignCapabilities(vec!["monitoring".into(), "logging".into()]);
        assert!(
            matches!(reassign, RoleTransformation::ReassignCapabilities(ref caps) if caps.len() == 2)
        );
    }
}
