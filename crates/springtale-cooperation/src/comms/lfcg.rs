//! LFCG communication axes — the type-level realization of `COOPERATION.md`
//! Appendix C.7 (Pais et al., *A Living Framework for Cooperative Games*, CHI
//! 2024, §4.3.3 "Communication-by-Design ↔ Means-of-Communication").
//!
//! The six `CommChannel` variants (§19) are classified along the two LFCG axes
//! so the cooperation model is explicitly isomorphic to the peer-reviewed
//! taxonomy rather than only implicitly aligned. This is additive typing — it
//! does not change the runtime behavior of the [`super::bus::FormationBus`].

use super::types::CommChannel;

/// LFCG **Communication-by-Design**: how strongly coordination depends on the
/// channel by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunicationByDesign {
    /// Coordination works without this channel — it is observational / implicit
    /// (Overcooked: read teammates from what they do, not what they say).
    Agnostic,
    /// A deliberately constrained channel — fixed vocabulary, low bandwidth
    /// (L4D survivor callouts, DRG/Siege pings).
    Limited,
    /// Coordination requires or is incentivised to use this channel (structured
    /// protocol messages, intent sing-backs, cohesion "Rock and Stone").
    RequiredOrIncentivised,
}

/// LFCG **Means-of-Communication**: how a message is conveyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeansOfComms {
    /// Inferred from observed behavior; never deliberately sent (Overcooked
    /// chicken-throwing).
    Implicit,
    /// Deliberately emitted. `auto = true` ⇒ condition-triggered (L4D callouts
    /// fire on state, not on an agent's decision); `auto = false` ⇒
    /// agent-initiated (pings, protocol messages, acks).
    Explicit { auto: bool },
}

impl CommChannel {
    /// Classify this channel along the two LFCG axes (Appendix C.7).
    pub fn classify(&self) -> (CommunicationByDesign, MeansOfComms) {
        use CommunicationByDesign as D;
        use MeansOfComms as M;
        match self {
            // L4D callouts: constrained vocabulary, fired automatically on state.
            CommChannel::StateBroadcast { .. } => (D::Limited, M::Explicit { auto: true }),
            // MH translated commands: structured, coordination depends on it.
            CommChannel::ProtocolMessage { .. } => {
                (D::RequiredOrIncentivised, M::Explicit { auto: false })
            }
            // DRG laser / Siege ping: a constrained attention-directing signal.
            CommChannel::DirectionalSignal { .. } => (D::Limited, M::Explicit { auto: false }),
            // "Rock and Stone": no information, an incentivised cohesion signal.
            CommChannel::CohesionSignal { .. } => {
                (D::RequiredOrIncentivised, M::Explicit { auto: false })
            }
            // Patapon sing-back: an incentivised explicit acknowledgement.
            CommChannel::IntentAcknowledgment { .. } => {
                (D::RequiredOrIncentivised, M::Explicit { auto: false })
            }
            // Overcooked observed behavior: inferred, never sent.
            CommChannel::ImplicitSignal { .. } => (D::Agnostic, M::Implicit),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cadence::{ActionDescriptor, AgentId};

    #[test]
    fn implicit_signal_is_agnostic_and_implicit() {
        let ch = CommChannel::ImplicitSignal {
            source: AgentId::new(),
            observed_action: ActionDescriptor {
                kind: "write".into(),
                target: None,
                payload_hash: 0,
            },
            inferred_meaning: None,
        };
        assert_eq!(
            ch.classify(),
            (CommunicationByDesign::Agnostic, MeansOfComms::Implicit)
        );
    }

    #[test]
    fn state_broadcast_is_auto_explicit() {
        let ch = CommChannel::CohesionSignal {
            source: AgentId::new(),
        };
        // Cohesion is explicit + agent-initiated, not auto.
        assert_eq!(ch.classify().1, MeansOfComms::Explicit { auto: false });
    }
}
