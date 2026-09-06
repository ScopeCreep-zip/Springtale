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
        RecipeAction::Pieces { id } => {
            let body: Value = client.get(&format!("/recipes/{id}/pieces")).await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        RecipeAction::Favorite { id } => {
            let body: Value = client
                .post(&format!("/recipes/{id}/favorite"), &json!({}))
                .await?;
            output::emit_status(json_out, &body, |v| {
                format!("favorite: {}", output::cell(v, "favorite"))
            })?;
        }
        RecipeAction::Recent { id } => {
            let body: Value = client
                .post(&format!("/recipes/{id}/recent"), &json!({}))
                .await?;
            output::emit_status(json_out, &body, |_| {
                format!("Recorded {id} as recently used.")
            })?;
        }
        RecipeAction::Fork { id, name } => {
            let body: Value = client
                .post(&format!("/recipes/{id}/fork"), &json!({ "new_name": name }))
                .await?;
            output::emit_status(json_out, &body, |v| {
                format!("Forked to {}", output::cell(v, "id"))
            })?;
        }
        RecipeAction::Preflight { id, inputs } => {
            let body: Value = client
                .post(&format!("/recipes/{id}/preflight"), &load_inputs(inputs)?)
                .await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        RecipeAction::TestStep {
            id,
            inputs,
            rule_index,
            step_index,
        } => {
            let body: Value = client
                .post(
                    &format!("/recipes/{id}/test-step"),
                    &json!({
                        "inputs": load_inputs(inputs)?,
                        "rule_index": rule_index,
                        "step_index": step_index,
                    }),
                )
                .await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        RecipeAction::Save { file } => {
            let recipe = crate::commands::json_input::load(&file)?;
            let body: Value = client.post("/recipes/user", &recipe).await?;
            output::emit_status(json_out, &body, |v| {
                format!("Saved recipe {}", output::cell(v, "id"))
            })?;
        }
        RecipeAction::Delete { id } => {
            let body: Value = client.delete(&format!("/recipes/user/{id}")).await?;
            output::emit_status(json_out, &body, |_| format!("Deleted recipe {id}."))?;
        }
        RecipeAction::Export { id } => {
            // The route answers TOML text, not JSON.
            let toml = text(
                &client,
                reqwest::Method::GET,
                &format!("/recipes/{id}/export"),
                None,
            )
            .await?;
            println!("{toml}");
        }
        RecipeAction::Render { id, inputs } => {
            let toml = text(
                &client,
                reqwest::Method::POST,
                &format!("/recipes/{id}/render"),
                Some(load_inputs(inputs)?),
            )
            .await?;
            println!("{toml}");
        }
        RecipeAction::Import { file } => {
            let toml_text = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", file.display()))?;
            // The route takes the TOML document itself as the body.
            let response = client
                .request(reqwest::Method::POST, "/recipes/import")
                .header("content-type", "text/plain")
                .body(toml_text)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("{}: {e}", crate::client::UNREACHABLE))?;
            let status = response.status();
            let raw = response.text().await.unwrap_or_default();
            if !status.is_success() {
                anyhow::bail!("{status}: {raw}");
            }
            let body: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
            output::emit_status(json_out, &body, |v| {
                format!("Imported recipe {}", output::cell(v, "id"))
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

/// Fetch a route that answers plain text (`export`, `render`) rather
/// than JSON, so the document lands on stdout unquoted.
async fn text(
    client: &Client,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<String> {
    let mut request = client.request(method, path);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("{}: {e}", crate::client::UNREACHABLE))?;
    let status = response.status();
    let raw = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("{status}: {raw}");
    }
    Ok(raw)
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
