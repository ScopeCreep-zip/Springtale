//! Step 4f — publish each agent's action to the bus as an `ImplicitSignal`.
//!
//! Per `COOPERATION.md §19` (Overcooked pattern): peers observe what others
//! are doing without explicit protocol messages. The bus's implicit channel
//! holds the latest action descriptor for each agent.

use crate::cooperation::formation::Formation;
use springtale_cooperation::tick_processor::FormationTickResult;

pub fn run(formation: &Formation, result: &FormationTickResult) {
    for report in &result.reports {
        if let Some(action) = &report.action_taken {
            formation
                .bus
                .update_implicit(report.agent_id, action.clone());
        }
    }
}
