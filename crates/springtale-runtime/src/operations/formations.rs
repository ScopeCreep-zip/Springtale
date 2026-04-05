//! Formation operations — create, deploy, pause, resume, dissolve, list.
//!
//! Uses the cooperation module directly. Formations are the user-facing
//! abstraction over the cooperative agent architecture (COOPERATION.pdf).

use serde::Serialize;

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Formation info for listing.
#[derive(Debug, Serialize)]
pub struct FormationInfo {
    pub id: String,
    pub name: String,
    pub intent: String,
    pub status: String,
    pub member_count: usize,
}

/// Create a new formation — stores config, creates member entries.
pub async fn create_formation(
    state: &RuntimeState,
    name: String,
    intent: String,
    connectors: Vec<String>,
) -> Result<String, OperationError> {
    let formation_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let row = springtale_store::FormationRow {
        id: formation_id.clone(),
        name,
        intent,
        status: "draft".to_owned(),
        created_at: now,
        updated_at: now,
    };

    state.store.insert_formation(&row).await?;

    for connector in &connectors {
        let member = springtale_store::FormationMemberRow {
            id: uuid::Uuid::new_v4().to_string(),
            formation_id: formation_id.clone(),
            connector_name: connector.clone(),
            role_hint: None,
        };
        state.store.insert_formation_member(&member).await?;
    }

    Ok(formation_id)
}

/// Deploy a formation — changes status to active.
pub async fn deploy_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "active").await?;
    Ok(())
}

/// Pause a formation.
pub async fn pause_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "paused").await?;
    Ok(())
}

/// Resume a paused formation.
pub async fn resume_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "active").await?;
    Ok(())
}

/// Dissolve a formation — graceful shutdown.
pub async fn dissolve_formation(state: &RuntimeState, id: &str) -> Result<(), OperationError> {
    state.store.update_formation_status(id, "dissolved").await?;
    Ok(())
}

/// List all formations with member counts.
pub async fn list_formations(state: &RuntimeState) -> Result<Vec<FormationInfo>, OperationError> {
    let formations = state.store.list_formations().await?;
    let mut infos = Vec::new();

    for f in formations {
        let members = state.store.list_formation_members(&f.id).await?;
        infos.push(FormationInfo {
            id: f.id,
            name: f.name,
            intent: f.intent,
            status: f.status,
            member_count: members.len(),
        });
    }

    Ok(infos)
}
