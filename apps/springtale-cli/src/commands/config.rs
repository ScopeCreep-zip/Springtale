use std::io::Read;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use springtale_runtime::operations::config::{self as config_ops, AI_COLONY_KEY, AiTarget};
use springtale_store::backend::sqlite::SqliteBackend;

use crate::cli::{AiConfigAction, ConfigAction};

/// Handle `config` subcommands. Store-direct: writes the same JSON shape
/// under the same key the daemon's `configure_ai_adapter` uses, so a
/// headless setup is picked up at the next boot / dispatch.
pub async fn run(action: ConfigAction, store: &SqliteBackend) -> Result<()> {
    match action {
        ConfigAction::Ai { action } => run_ai(action, store).await,
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

async fn run_ai(action: AiConfigAction, store: &SqliteBackend) -> Result<()> {
    match action {
        AiConfigAction::Get { scope, id } => {
            let target = parse_target(&scope, id)?;
            let resolved = match &target {
                AiTarget::Agent { rule_id } => {
                    config_ops::resolve_ai_config(store, rule_id, None).await
                }
                AiTarget::Formation { .. } => {
                    match config_ops::get_config(store, &target.key()).await {
                        Ok(Value::Null) => config_ops::get_config(store, AI_COLONY_KEY).await,
                        other => other,
                    }
                }
                AiTarget::Colony => config_ops::get_config(store, AI_COLONY_KEY).await,
            }
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("{}", serde_json::to_string_pretty(&redact(resolved))?);
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
            let config = Value::Object(cfg);
            // Validate exactly as the runtime does before persisting.
            config_ops::build_adapter(&config)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            config_ops::set_config(store, &target.key(), config)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "AI config stored under '{}' (applies at next daemon start or dispatch)",
                target.key()
            );
        }
    }
    Ok(())
}

/// Never echo a stored API key back to the terminal.
fn redact(mut value: Value) -> Value {
    if let Some(obj) = value.as_object_mut()
        && obj.contains_key("api_key")
    {
        obj.insert("api_key".into(), Value::String("<redacted>".into()));
    }
    value
}
