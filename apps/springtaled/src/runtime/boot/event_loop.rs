use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use springtale_core::rule::engine::RuleEngine;
use springtale_scheduler::queue::producer::JobProducer;

/// Main event loop: receives trigger events, matches rules, enqueues actions.
pub(super) async fn event_loop(
    mut trigger_rx: mpsc::Receiver<springtale_core::rule::engine::TriggerEvent>,
    engine: Arc<RwLock<RuleEngine>>,
    producer: Arc<JobProducer>,
) {
    while let Some(event) = trigger_rx.recv().await {
        let engine = engine.read().await;
        let matches = springtale_core::router::dispatch::dispatch_event(&engine, &event);

        for rule_match in &matches {
            tracing::info!(
                rule = %rule_match.rule_name,
                actions = rule_match.actions.len(),
                "rule matched trigger — enqueuing actions"
            );

            for action in rule_match.actions.iter() {
                match serde_json::to_value(action) {
                    Ok(payload) => {
                        if let Err(e) = producer.enqueue(payload, 3).await {
                            tracing::error!(
                                rule = %rule_match.rule_name,
                                error = %e,
                                "failed to enqueue action"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            rule = %rule_match.rule_name,
                            error = %e,
                            "failed to serialize action"
                        );
                    }
                }
            }
        }
    }
}
