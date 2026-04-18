//! Action executors — one file per `Intervention` variant plus a central
//! dispatcher that picks the right executor.

pub mod change_intent;
pub mod dispatcher;
pub mod dissolve;
pub mod escalate;
pub mod inject_fuel;

pub use dispatcher::DefaultInterventionAction;
