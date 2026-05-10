//! Rule-engine trigger dispatch — what fires when a `TriggerEvent` arrives
//! on `bot.rule_rx`. Lives in its own module so `event_loop.rs` stays
//! composition-only (the cooperation tick pipeline is in `tick_steps/`).

use crate::runtime::lifecycle::Bot;

pub async fn handle_trigger_event(
    bot: &mut Bot,
    event: &springtale_core::rule::engine::TriggerEvent,
) -> Result<(), crate::error::BotError> {
    let engine = bot.engine.read().await;
    let matches = springtale_core::router::dispatch::dispatch_event(&engine, event);
    drop(engine);

    for rule_match in &matches {
        tracing::info!(
            rule = %rule_match.rule_name,
            actions = rule_match.actions.len(),
            "bot: rule matched trigger — dispatching actions"
        );
        for action in rule_match.actions.iter() {
            match springtale_runtime::dispatch::dispatch_action(
                action,
                &bot.capability_bridge,
                &bot.sentinel,
            )
            .await
            {
                Ok(msg) => tracing::info!(
                    rule = %rule_match.rule_name,
                    result = %msg,
                    "bot: action dispatched"
                ),
                Err(e) => tracing::error!(
                    rule = %rule_match.rule_name,
                    error = %e,
                    "bot: action dispatch failed"
                ),
            }
        }
    }
    Ok(())
}
