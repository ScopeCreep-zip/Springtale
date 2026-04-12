//! Built-in command handlers.
//!
//! One module per command. Each submodule defines a `*Handler` struct and
//! its `Handler` impl. This module re-exports them and provides
//! [`register_builtins`] and [`BUILTIN_COMMANDS`] for the rest of the crate.

mod alias;
mod connectors;
mod delrule;
mod disable;
mod enable;
mod events;
mod help;
mod newrule;
mod pair;
mod prefs;
mod rules;
mod run;
mod send;
mod status;
mod toggle;

pub use alias::AliasHandler;
pub use connectors::ConnectorsHandler;
pub use delrule::DelRuleHandler;
pub use disable::DisableHandler;
pub use enable::EnableHandler;
pub use events::EventsHandler;
pub use help::HelpHandler;
pub use newrule::NewRuleHandler;
pub use pair::PairHandler;
pub use prefs::PrefsHandler;
pub use rules::RulesHandler;
pub use run::RunHandler;
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
    Ok(())
}
