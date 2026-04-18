//! Variant dispatcher — maps an `Intervention` to its per-variant executor.
//!
//! Kept tiny (just the match) so adding a variant = new action file + one
//! new arm.

use async_trait::async_trait;

use crate::cooperation::formation::Formation;

use super::super::trait_::InterventionAction;
use super::super::types::{Intervention, InterventionError};
use super::{change_intent, dissolve, escalate, inject_fuel};

pub struct DefaultInterventionAction;

#[async_trait]
impl InterventionAction for DefaultInterventionAction {
    async fn execute(
        &self,
        intervention: &Intervention,
        formation: &mut Formation,
    ) -> Result<(), InterventionError> {
        match intervention {
            Intervention::ChangeIntent(intent) => change_intent::apply(formation, intent.clone()),
            Intervention::InjectFuel(budget) => inject_fuel::apply(formation, budget),
            Intervention::ForcedDissolve { reason } => dissolve::apply(formation, reason.clone()),
            Intervention::EscalateToUser { summary } => {
                escalate::apply(formation, summary.clone())
            }
        }
    }
}
