//! Built-in command handlers.
//!
//! One module per command. Each submodule defines a `*Handler` struct and
//! its `Handler` impl. This module re-exports them and provides
//! [`register_builtins`] and [`BUILTIN_COMMANDS`] for the rest of the crate.

mod ai;
mod alias;
mod approvals;
mod connectors;
mod delrule;
mod disable;
mod enable;
mod events;
mod formation;
mod help;
mod memory;
mod newrule;
mod pair;
mod prefs;
mod resolve;
mod rules;
mod run;
mod safety;
mod send;
mod status;
mod toggle;

pub use ai::AiHandler;
pub use alias::AliasHandler;
pub use approvals::ApprovalsHandler;
pub use connectors::ConnectorsHandler;
pub use delrule::DelRuleHandler;
pub use disable::DisableHandler;
pub use enable::EnableHandler;
pub use events::EventsHandler;
pub use formation::FormationHandler;
pub use help::HelpHandler;
pub use memory::MemoryHandler;
pub use newrule::NewRuleHandler;
pub use pair::PairHandler;
pub use prefs::PrefsHandler;
pub use rules::RulesHandler;
pub use run::RunHandler;
pub use safety::SafetyHandler;
pub use send::SendHandler;
pub use status::StatusHandler;
pub use toggle::ToggleHandler;

use crate::error::BotError;
use crate::handler::registry::HandlerRegistry;

/// Command names reserved for builtins. Cannot be overridden by connectors.
pub const BUILTIN_COMMANDS: &[&str] = &[
    "help",
    "status",
    "rules",
    "connectors",
    "prefs",
    "alias",
    "pair",
    "toggle",
    "enable",
    "disable",
    "events",
    "send",
    "newrule",
    "delrule",
    "run",
    // Plan 5.4 — the platform verbs, reachable from chat.
    "formation",
    "approvals",
    "memory",
    "safety",
    "ai",
];

/// Register every builtin handler into a fresh registry.
pub fn register_builtins(registry: &mut HandlerRegistry) -> Result<(), BotError> {
    registry.register("help".into(), Box::new(HelpHandler))?;
    registry.register("status".into(), Box::new(StatusHandler))?;
    registry.register("rules".into(), Box::new(RulesHandler))?;
    registry.register("connectors".into(), Box::new(ConnectorsHandler))?;
    registry.register("prefs".into(), Box::new(PrefsHandler))?;
    registry.register("alias".into(), Box::new(AliasHandler))?;
    registry.register("pair".into(), Box::new(PairHandler))?;
    registry.register("toggle".into(), Box::new(ToggleHandler))?;
    registry.register("enable".into(), Box::new(EnableHandler))?;
    registry.register("disable".into(), Box::new(DisableHandler))?;
    registry.register("events".into(), Box::new(EventsHandler))?;
    registry.register("send".into(), Box::new(SendHandler))?;
    registry.register("newrule".into(), Box::new(NewRuleHandler))?;
    registry.register("delrule".into(), Box::new(DelRuleHandler))?;
    registry.register("run".into(), Box::new(RunHandler))?;
    // Plan 5.4 — chat runs the platform, under the drum rule.
    registry.register("formation".into(), Box::new(FormationHandler))?;
    registry.register("approvals".into(), Box::new(ApprovalsHandler))?;
    registry.register("memory".into(), Box::new(MemoryHandler))?;
    registry.register("safety".into(), Box::new(SafetyHandler))?;
    registry.register("ai".into(), Box::new(AiHandler))?;
    Ok(())
}
