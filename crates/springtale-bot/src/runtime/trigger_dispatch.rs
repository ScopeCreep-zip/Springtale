//! Rule-engine trigger dispatch — what fires when a `TriggerEvent` arrives
//! on `bot.rule_rx`. Lives in its own module so `event_loop.rs` stays
//! composition-only (the cooperation tick pipeline is in `tick_steps/`).

use crate::runtime::lifecycle::Bot;

pub async fn handle_trigger_event(
    bot: &mut Bot,
    event: &springtale_core::rule::engine::TriggerEvent,
) -> Result<(), crate::error::BotError> {
    // D1 — universal mention harvester. Runs BEFORE rule dispatch
    // so destinations get registered regardless of whether any
    // rule matches this event. The harvester is a no-op for
    // connectors without a MentionExtractor (cron / filesystem /
    // http / browser / shell).
    if let Some(connector_name) = event.connector.as_deref() {
        if let Err(e) = springtale_runtime::operations::workspaces::harvest_event(
            &bot.store,
            &bot.registry,
            connector_name,
            event.event.as_deref().unwrap_or(""),
            &event.payload,
            None,
        )
        .await
        {
            tracing::warn!(
                error = %e,
                connector = %connector_name,
                "destination harvester failed (non-fatal)"
            );
        }
    }

    let engine = bot.engine.read().await;
    let matches = springtale_core::router::dispatch::dispatch_event(&engine, event);
    drop(engine);

    let mode = match event.trigger_type.as_str() {
        "Cron" => springtale_cooperation::execution::ExecutionMode::Cron,
        "Webhook" => springtale_cooperation::execution::ExecutionMode::Webhook,
        "ConnectorEvent" => springtale_cooperation::execution::ExecutionMode::ConnectorEvent,
        "FileWatch" => springtale_cooperation::execution::ExecutionMode::FileWatch,
        _ => springtale_cooperation::execution::ExecutionMode::Manual,
    };

    for rule_match in &matches {
        tracing::info!(
            rule = %rule_match.rule_name,
            actions = rule_match.actions.len(),
            "bot: rule matched trigger — dispatching actions"
        );

        // Build the cooperation envelope per rule fire. Solo bots
        // pass `for_global`; once formation context is plumbed into
        // the bot (Phase 0.4), this becomes `for_agent` /
        // `for_formation` with the firing agent's id and tier.
        let execution = springtale_cooperation::execution::ExecutionContext::for_global(
            rule_match.rule_id,
            mode,
        );
        let trigger_payload = (*rule_match.payload).clone();

        match springtale_runtime::dispatch::dispatch_actions(
            &rule_match.actions,
            &bot.capability_bridge,
            &bot.sentinel,
            execution,
            trigger_payload,
        )
        .await
        {
            Ok(chain) => tracing::info!(
                rule = %rule_match.rule_name,
                summary = %chain.brief(),
                "bot: rule actions dispatched"
            ),
            Err(e) => tracing::error!(
                rule = %rule_match.rule_name,
                error = %e,
                "bot: rule actions dispatch failed"
            ),
        }
    }
    Ok(())
}
