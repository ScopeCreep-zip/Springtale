//! Backend-backed mental-model store — routes persistence through the
//! workspace `StorageBackend` trait so all cooperation SQL lives in the
//! `springtale-store` crate (per CLAUDE.md "No raw SQL outside store").
//!
//! Replaces the previous raw-rusqlite `SqliteStore`. Callers can still
//! inject alternative `Store` implementations (e.g., in-memory for tests)
//! via the trait.

use std::sync::Arc;

use async_trait::async_trait;
use springtale_store::StorageBackend;

use crate::mental_model::types::SharedMentalModel;

use super::error::StoreError;
use super::rows::{from_bundle, to_bundle};
use super::trait_::Store;

/// Mental-model store backed by a `StorageBackend`. One instance can
/// serve many formations — each call takes `formation_id` for row
/// namespacing inside the shared tables.
pub struct BackendStore {
    backend: Arc<dyn StorageBackend>,
}

impl BackendStore {
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Store for BackendStore {
    async fn save(&self, formation_id: &str, model: &SharedMentalModel) -> Result<(), StoreError> {
        let bundle = to_bundle(model)?;
        self.backend
            .mental_model_save(formation_id, &bundle)
            .await?;
        Ok(())
    }

    async fn load(&self, formation_id: &str) -> Result<SharedMentalModel, StoreError> {
        let bundle = self.backend.mental_model_load(formation_id).await?;
        from_bundle(bundle)
    }

    async fn clear(&self, formation_id: &str) -> Result<(), StoreError> {
        self.backend.mental_model_clear(formation_id).await?;
        Ok(())
    }
}
