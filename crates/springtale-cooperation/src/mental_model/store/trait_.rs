use crate::mental_model::types::SharedMentalModel;

use super::error::StoreError;

/// Persistence interface for a `SharedMentalModel` scoped to one formation.
///
/// The `formation_id` key namespaces everything so one SQLite file can host
/// many formations without collisions.
pub trait Store: Send + Sync {
    fn save(&self, formation_id: &str, model: &SharedMentalModel) -> Result<(), StoreError>;
    fn load(&self, formation_id: &str) -> Result<SharedMentalModel, StoreError>;
    fn clear(&self, formation_id: &str) -> Result<(), StoreError>;
}
