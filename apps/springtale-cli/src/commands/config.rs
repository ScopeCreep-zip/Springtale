//! `springtale config` — runtime configuration, over the daemon.
//!
//! Writes go through `POST /config/ai/configure`, which persists *and*
//! hot-swaps the adapter, so a change takes effect without a restart.

use std::io::Read;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use springtale_runtime::operations::config::{AI_COLONY_KEY, AiTarget};

use crate::cli::{AiConfigAction, ConfigAction};
use crate::client::Client;
use crate::commands::json_input;
use crate::output;

/// Handle `config` subcommands.
pub async fn run(action: ConfigAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        ConfigAction::List => {
            let body: Value = client.get("/config").await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })
        }
        ConfigAction::Connector { name, file } => {
            let body: Value = client
                .post(&format!("/config/connector/{name}"), &json_input::load(&file)?)
                .await?;
            output::emit_status(json_out, &body, |_| {
                format!("Connector config saved for '{name}'.")
            })
        }
        ConfigAction::Heartbeat { file } => {
            let body: Value = match file {
                Some(file) => client.put("/config/heartbeat", &json_input::load(&file)?).await?,
                None => client.get("/config/heartbeat").await?,
            };
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })
        }
        ConfigAction::Ai { action } => run_ai(action, &client, json_out).await,
    }
}

fn parse_target(scope: &str, id: Option<String>) -> Result<AiTarget> {
    match scope {
        "colony" => Ok(AiTarget::Colony),
        "formation" => Ok(AiTarget::Formation {
            id: id.context("--scope formation needs a formation id")?,
        }),
        "agent" => {
            let raw = id.context("--scope agent needs a rule id")?;
            let rule_id = uuid::Uuid::parse_str(&raw).context("rule id must be a UUID")?;
            Ok(AiTarget::Agent { rule_id })
        }
        other => bail!("unknown scope: {other}"),
    }
}

async fn run_ai(action: AiConfigAction, client: &Client, json_out: bool) -> Result<()> {
    match action {
        AiConfigAction::Get { scope, id } => {
            let target = parse_target(&scope, id)?;
            let mut body: Value = client.get(&format!("/config/{}", target.key())).await?;
            // Levels inherit: an unset formation/agent falls back to the
            // colony socket, which is what the runtime resolves to.
            if body.get("value").is_none_or(Value::is_null) && !matches!(target, AiTarget::Colony) {
                body = client.get(&format!("/config/{AI_COLONY_KEY}")).await?;
            }
            let redacted = redact(body);
            output::emit(json_out, &redacted, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })
        }
        AiConfigAction::Put { file } => {
            // The whole adapter document, as-is. `set` is the flag-built
            // sibling; this one is for a config you already have on disk.
            let body: Value = client.post("/config/ai", &json_input::load(&file)?).await?;
            output::emit_status(json_out, &body, |_| "AI config applied.".to_owned())
        }
        AiConfigAction::Set {
            scope,
            id,
            adapter_type,
            model,
            base_url,
            api_key_stdin,
        } => {
            let target = parse_target(&scope, id)?;
            let mut cfg = Map::new();
            cfg.insert("type".into(), Value::String(adapter_type));
            if let Some(model) = model {
                cfg.insert("model".into(), Value::String(model));
            }
            if let Some(url) = base_url {
                cfg.insert("base_url".into(), Value::String(url));
            }
            if api_key_stdin {
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .context("reading API key from stdin")?;
                let key = buf.lines().next().unwrap_or("").trim().to_owned();
                if key.is_empty() {
                    bail!("--api-key-stdin given but stdin was empty");
                }
                cfg.insert("api_key".into(), Value::String(key));
            }
            let key = target.key();
            let body: Value = client
                .post(
                    "/config/ai/configure",
                    &json!({ "target": target, "config": Value::Object(cfg) }),
                )
                .await?;
            output::emit(json_out, &body, |_| {
                format!("AI config applied at '{key}' (live, no restart)")
            })
        }
    }
}

/// Never echo a stored API key back to the terminal.
fn redact(mut value: Value) -> Value {
    if let Some(obj) = value.get_mut("value").and_then(Value::as_object_mut)
        && obj.contains_key("api_key")
    {
        obj.insert("api_key".into(), Value::String("<redacted>".into()));
    }
    value
}
