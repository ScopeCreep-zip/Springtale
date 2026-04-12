//! Canvas operations — get and update A2UI state.

use serde::Serialize;
use springtale_core::canvas::{CanvasState, CanvasUpdate};

use crate::error::OperationError;
use crate::state::RuntimeState;

/// Get the current canvas state.
pub async fn get_canvas(state: &RuntimeState) -> CanvasState {
    state.canvas.read().await.clone()
}

/// Apply a canvas update — mutates state and broadcasts to subscribers.
pub async fn update_canvas(state: &RuntimeState, update: CanvasUpdate) -> CanvasState {
    let mut canvas = state.canvas.write().await;
    canvas.apply(&update);
    let snapshot = canvas.clone();

    // Broadcast to SSE/Tauri event subscribers
    let _ = state.canvas_tx.send(update);

    snapshot
}

// ── Connection graph ────────────────────────────────────────────────────────

/// A pipe (data flow) within a connection.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionPipe {
    pub id: String,
    pub dir: i8,
    pub status: String,
}

/// A connection between two connectors with one or more pipes.
#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    pub a: String,
    pub b: String,
    pub pipes: Vec<ConnectionPipe>,
}

/// Compute the connection graph from rules and connector schemas.
///
/// Moved from frontend `mappers.ts:mapConnections` — this is business
/// logic about how rules create data flows between connectors.
pub async fn compute_connections(state: &RuntimeState) -> Result<Vec<Connection>, OperationError> {
    let rules = super::rules::list_rules(state).await;
    let schemas = super::connectors::get_connector_schemas(state).await;
    let connectors = super::connectors::list_connectors(state).await;

    let connector_names: Vec<&str> = connectors.iter().map(|c| c.name.as_str()).collect();

    let mut conns: Vec<Connection> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for rule in &rules {
        let trig_conn = rule.connector_name.as_deref().unwrap_or(&rule.trigger_type);
        if !connector_names.contains(&trig_conn) {
            continue;
        }

        for dest_name in &connector_names {
            if *dest_name == trig_conn {
                continue;
            }

            let mut key_parts = [trig_conn, dest_name];
            key_parts.sort();
            let key = format!("{}:{}", key_parts[0], key_parts[1]);

            if seen.contains(&key) {
                // Add pipe to existing connection
                if let Some(existing) = conns.iter_mut().find(|c| {
                    let mut ck = [c.a.as_str(), c.b.as_str()];
                    ck.sort();
                    format!("{}:{}", ck[0], ck[1]) == key
                }) {
                    existing.pipes.push(ConnectionPipe {
                        id: rule.id.clone(),
                        dir: if trig_conn == existing.a { 1 } else { -1 },
                        status: if rule.status == "enabled" {
                            "active".into()
                        } else {
                            "idle".into()
                        },
                    });
                }
                break;
            }

            // Check schema compatibility
            let source_schema = schemas.iter().find(|s| s.name == trig_conn);
            let dest_schema = schemas.iter().find(|s| s.name == *dest_name);
            match (source_schema, dest_schema) {
                (Some(src), Some(dst)) if !src.triggers.is_empty() || !dst.actions.is_empty() => {
                    seen.insert(key);
                    conns.push(Connection {
                        a: trig_conn.to_owned(),
                        b: dest_name.to_string(),
                        pipes: vec![ConnectionPipe {
                            id: rule.id.clone(),
                            dir: 1,
                            status: if rule.status == "enabled" {
                                "active".into()
                            } else {
                                "idle".into()
                            },
                        }],
                    });
                    break;
                }
                _ => continue,
            }
        }
    }

    Ok(conns)
}
