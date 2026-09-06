//! `springtale recipe` — the shipped automation starters, over the daemon.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::RecipeAction;
use crate::client::Client;
use crate::output;

/// Handle recipe subcommands.
pub async fn run(action: RecipeAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        RecipeAction::List { category } => {
            let path = match category {
                Some(c) => format!("/recipes?category={c}"),
                None => "/recipes".to_owned(),
            };
            let body: Value = client.get(&path).await?;
            output::emit(json_out, &body, |v| {
                let empty = Vec::new();
                let rows = v
                    .as_array()
                    .unwrap_or(&empty)
                    .iter()
                    .map(|r| {
                        vec![
                            output::cell(r, "id"),
                            output::cell(r, "name"),
                            output::cell(r, "category"),
                        ]
                    })
                    .collect();
                output::rows_table(&["ID", "NAME", "CATEGORY"], rows)
            })?;
        }
        RecipeAction::Categories => {
            let body: Value = client.get("/recipes/categories").await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        RecipeAction::Get { id } => {
            let body: Value = client.get(&format!("/recipes/{id}")).await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        RecipeAction::Preview { id, inputs } => {
            let body: Value = client
                .post(&format!("/recipes/{id}/preview"), &load_inputs(inputs)?)
                .await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        RecipeAction::Apply { id, inputs } => {
            let body: Value = client
                .post(&format!("/recipes/{id}/apply"), &load_inputs(inputs)?)
                .await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
    }
    Ok(())
}

/// Read a `{ "values": { ... } }` inputs file, or send an empty set.
fn load_inputs(path: Option<std::path::PathBuf>) -> Result<Value> {
    let Some(path) = path else {
        return Ok(json!({ "values": {} }));
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("inputs must be JSON: {e}"))
}
