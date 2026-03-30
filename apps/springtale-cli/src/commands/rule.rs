use anyhow::Result;
use tabled::{Table, Tabled};

use springtale_core::rule::engine::{RuleEngine, TriggerEvent};
use springtale_core::rule::trigger::Trigger;
use springtale_core::rule::types::{Rule, RuleId, RuleStatus};
use springtale_store::backend::sqlite::SqliteBackend;
use springtale_store::backend::trait_::StorageBackend;

use crate::cli::RuleAction;
use crate::output;

/// Row type for the rule list table.
#[derive(Tabled)]
struct RuleTableRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "TRIGGER")]
    trigger: String,
}

/// Handle rule subcommands.
pub async fn run(action: RuleAction, store: &SqliteBackend, json: bool) -> Result<()> {
    match action {
        RuleAction::List => {
            let rules = store.list_rules().await?;

            if json {
                output::print_json(&rules)?;
            } else if rules.is_empty() {
                println!("No rules defined.");
            } else {
                let rows: Vec<RuleTableRow> = rules
                    .iter()
                    .map(|r| RuleTableRow {
                        id: r.id.to_string(),
                        name: r.name.clone(),
                        status: format!("{:?}", r.status),
                        trigger: format!("{:?}", r.trigger),
                    })
                    .collect();
                let table = Table::new(rows).to_string();
                println!("{table}");
            }
        }
        RuleAction::Add { file } => {
            let contents = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("failed to read rule file at {}: {e}", file.display()))?;

            let rule: Rule = match file.extension().and_then(|ext| ext.to_str()) {
                Some("toml") => toml::from_str(&contents)
                    .map_err(|e| anyhow::anyhow!("failed to parse rule TOML: {e}"))?,
                Some("json") => serde_json::from_str(&contents)
                    .map_err(|e| anyhow::anyhow!("failed to parse rule JSON: {e}"))?,
                _ => {
                    // Try TOML first, then JSON
                    toml::from_str(&contents).or_else(|_| {
                        serde_json::from_str(&contents)
                            .map_err(|e| anyhow::anyhow!(
                                "failed to parse rule file (tried TOML and JSON): {e}"
                            ))
                    })?
                }
            };

            let rule_id = store.insert_rule(&rule).await?;
            println!("Added rule: {} (id: {rule_id})", rule.name);
        }
        RuleAction::Run { id } => {
            let uuid = uuid::Uuid::parse_str(&id)
                .map_err(|e| anyhow::anyhow!("invalid rule ID: {e}"))?;
            let rule_id = RuleId(uuid);

            // Load all rules and find the target
            let rules = store.list_rules().await?;
            let rule = rules
                .into_iter()
                .find(|r| r.id == rule_id)
                .ok_or_else(|| anyhow::anyhow!("rule not found: {id}"))?;

            // Build a synthetic trigger event from the rule's trigger
            let event = synthetic_trigger_event(&rule);

            // Load into engine and evaluate
            let mut engine = RuleEngine::new();
            engine.add_rule(rule);
            let matches = engine.evaluate(&event);

            if matches.is_empty() {
                println!("No actions matched (rule may be disabled or conditions failed).");
            } else {
                for m in &matches {
                    println!("Rule matched: {} ({})", m.rule_name, m.rule_id);
                    for (i, action) in m.actions.iter().enumerate() {
                        println!("  action[{i}]: {action:?}");
                    }
                }
            }
        }
        RuleAction::Toggle { id } => {
            let uuid = uuid::Uuid::parse_str(&id)
                .map_err(|e| anyhow::anyhow!("invalid rule ID: {e}"))?;
            let rule_id = RuleId(uuid);
            // Toggle: read current state and flip
            let rules = store.list_rules().await?;
            let rule = rules
                .iter()
                .find(|r| r.id == rule_id)
                .ok_or_else(|| anyhow::anyhow!("rule not found: {id}"))?;

            let new_enabled = !matches!(rule.status, RuleStatus::Enabled);
            store.toggle_rule(&rule_id, new_enabled).await?;
            println!(
                "Rule {id} is now {}",
                if new_enabled { "enabled" } else { "disabled" }
            );
        }
    }
    Ok(())
}

/// Build a synthetic `TriggerEvent` that matches a rule's trigger definition.
///
/// Used for dry-run evaluation: the event is constructed so it will match the
/// rule's trigger pattern, letting us see which actions would fire.
fn synthetic_trigger_event(rule: &Rule) -> TriggerEvent {
    match &rule.trigger {
        Trigger::Cron { expression } => TriggerEvent {
            trigger_type: "Cron".into(),
            connector: None,
            event: Some(expression.clone()),
            payload: serde_json::json!({"synthetic": true}),
        },
        Trigger::FileWatch { path, event } => TriggerEvent {
            trigger_type: "FileWatch".into(),
            connector: None,
            event: Some(format!("{path}:{event}")),
            payload: serde_json::json!({"synthetic": true, "path": path}),
        },
        Trigger::Webhook { path } => TriggerEvent {
            trigger_type: "Webhook".into(),
            connector: None,
            event: Some(path.clone()),
            payload: serde_json::json!({"synthetic": true}),
        },
        Trigger::ConnectorEvent { connector, event } => TriggerEvent {
            trigger_type: "ConnectorEvent".into(),
            connector: Some(connector.clone()),
            event: Some(event.clone()),
            payload: serde_json::json!({"synthetic": true}),
        },
        Trigger::SystemEvent { event } => TriggerEvent {
            trigger_type: "SystemEvent".into(),
            connector: None,
            event: Some(event.clone()),
            payload: serde_json::json!({"synthetic": true}),
        },
    }
}
