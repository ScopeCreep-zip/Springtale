use async_trait::async_trait;

use crate::mental_model::types::SharedMentalModel;

use super::error::StoreError;

/// Persistence interface for a `SharedMentalModel` scoped to one formation.
///
/// The `formation_id` key namespaces everything so one backend can host
/// many formations without collisions.
#[async_trait]
pub trait Store: Send + Sync {
    async fn save(&self, formation_id: &str, model: &SharedMentalModel) -> Result<(), StoreError>;
    async fn load(&self, formation_id: &str) -> Result<SharedMentalModel, StoreError>;
    async fn clear(&self, formation_id: &str) -> Result<(), StoreError>;
}
