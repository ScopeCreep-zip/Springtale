use async_trait::async_trait;

use crate::cooperation::formation::Formation;

use super::types::{Intervention, InterventionError, InterventionSignals};

/// Pure policy — maps live cooperation signals to an `Intervention` verb.
///
/// Evaluators are free to be stateless (rule tables) or stateful (hysteresis
/// counters); the trait only requires the map function.
pub trait InterventionEvaluator: Send + Sync {
    fn evaluate(&self, signals: &InterventionSignals) -> Option<Intervention>;
}

/// Side-effect executor — mutates a Formation to realise the chosen verb.
///
/// Kept async because future variants (e.g. escalating to the user via IPC)
/// will need to await.
#[async_trait]
pub trait InterventionAction: Send + Sync {
    async fn execute(
        &self,
        intervention: &Intervention,
        formation: &mut Formation,
    ) -> Result<(), InterventionError>;
}
