//! Intent — publish intent patterns to the cadence bus.
//!
//! Per COOPERATION.pdf §3.2:
//! Game source: Patapon drum patterns, Total War attack/defend orders,
//! Siege IGL strat calls.
//!
//! "Intent describes WHAT, never HOW. 'Attack' tells the formation to
//! engage. It does not tell individual agents which target to pick,
//! what timing to use, or what sequence to follow."
//!
//! IntentPattern is defined in cooperation::cadence (§5) — the
//! orchestrator publishes it, the cadence bus broadcasts it,
//! agents interpret it individually.

use crate::cooperation::cadence::{CadenceBus, IntentPattern};

/// Publish a new intent to the cadence bus.
///
/// Three sources for intent transitions (§5.2):
/// 1. Orchestrator command (this function)
/// 2. Formation self-governance (via consensus at Fever tier)
/// 3. Momentum-gated access to new intent options
pub async fn publish_intent(cadence: &CadenceBus, intent: IntentPattern) {
    *cadence.current_intent.write().await = intent;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_publish_intent() {
        let (bus, mut reports_rx) = CadenceBus::new(Duration::from_millis(100), 16);

        publish_intent(&bus, IntentPattern::Execute { plan_id: None }).await;

        let intent = bus.current_intent.read().await;
        assert!(matches!(&*intent, IntentPattern::Execute { .. }));

        // Verify reports channel is functional (not just created)
        let sender = bus.reports_sender();
        sender.send(springtale_cooperation::cadence::TickReport {
            agent_id: springtale_cooperation::cadence::AgentId::new(),
            tick_sequence: 0,
            action_taken: None,
            latency: Duration::from_millis(0),
            intent_alignment: 1.0,
            interference_with: vec![],
        }).await.expect("report send");
        let report = reports_rx.recv().await.expect("report recv");
        assert!((report.intent_alignment - 1.0).abs() < f32::EPSILON);
    }
}
