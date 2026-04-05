use anyhow::Result;
use tabled::{Table, Tabled};

use springtale_core::rule::types::{Rule, RuleId, RuleStatus};
use springtale_store::backend::sqlite::SqliteBackend;

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
            let rules = springtale_runtime::operations::rules::list_rules_from_store(store)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

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
            let contents = std::fs::read_to_string(&file).map_err(|e| {
                anyhow::anyhow!("failed to read rule file at {}: {e}", file.display())
            })?;

            let rule: Rule = match file.extension().and_then(|ext| ext.to_str()) {
                Some("toml") => toml::from_str(&contents)
                    .map_err(|e| anyhow::anyhow!("failed to parse rule TOML: {e}"))?,
                Some("json") => serde_json::from_str(&contents)
                    .map_err(|e| anyhow::anyhow!("failed to parse rule JSON: {e}"))?,
                _ => {
                    // Try TOML first, then JSON
                    toml::from_str(&contents).or_else(|_| {
                        serde_json::from_str(&contents).map_err(|e| {
                            anyhow::anyhow!("failed to parse rule file (tried TOML and JSON): {e}")
                        })
                    })?
                }
            };

            let rule_id = springtale_runtime::operations::rules::add_rule_to_store(store, &rule)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Added rule: {} (id: {rule_id})", rule.name);
        }
        RuleAction::Run { id } => {
            let uuid =
                uuid::Uuid::parse_str(&id).map_err(|e| anyhow::anyhow!("invalid rule ID: {e}"))?;
            let rule_id = RuleId(uuid);

            // Load all rules and find the target
            let rules = springtale_runtime::operations::rules::list_rules_from_store(store)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let rule = rules
                .into_iter()
                .find(|r| r.id == rule_id)
                .ok_or_else(|| anyhow::anyhow!("rule not found: {id}"))?;

            let result = springtale_runtime::operations::rules::run_rule_standalone(&rule);

            if !result.matched {
                println!("No actions matched (rule may be disabled or conditions failed).");
            } else {
                println!(
                    "Rule matched: {} ({}) — {} action(s) would fire",
                    rule.name, rule.id, result.actions_count
                );
            }
        }
        RuleAction::Delete { id } => {
            let uuid =
                uuid::Uuid::parse_str(&id).map_err(|e| anyhow::anyhow!("invalid rule ID: {e}"))?;
            let rule_id = RuleId(uuid);
            springtale_runtime::operations::rules::delete_rule_from_store(store, &rule_id)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Deleted rule: {id}");
        }
        RuleAction::Update { id, file } => {
            let uuid =
                uuid::Uuid::parse_str(&id).map_err(|e| anyhow::anyhow!("invalid rule ID: {e}"))?;
            let rule_id = RuleId(uuid);

            let contents = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("failed to read file: {e}"))?;
            let mut rule: Rule = match file.extension().and_then(|ext| ext.to_str()) {
                Some("toml") => toml::from_str(&contents)?,
                Some("json") => serde_json::from_str(&contents)?,
                _ => toml::from_str(&contents).or_else(|_| {
                    serde_json::from_str(&contents)
                        .map_err(|e| anyhow::anyhow!("failed to parse rule file: {e}"))
                })?,
            };
            rule.id = rule_id;
            springtale_runtime::operations::rules::add_rule_to_store(store, &rule)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Updated rule: {id}");
        }
        RuleAction::Toggle { id } => {
            let uuid =
                uuid::Uuid::parse_str(&id).map_err(|e| anyhow::anyhow!("invalid rule ID: {e}"))?;
            let rule_id = RuleId(uuid);

            // Read current state to determine toggle direction
            let rules = springtale_runtime::operations::rules::list_rules_from_store(store)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let rule = rules
                .iter()
                .find(|r| r.id == rule_id)
                .ok_or_else(|| anyhow::anyhow!("rule not found: {id}"))?;

            let new_enabled = !matches!(rule.status, RuleStatus::Enabled);
            springtale_runtime::operations::rules::toggle_rule_in_store(
                store,
                &rule_id,
                new_enabled,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "Rule {id} is now {}",
                if new_enabled { "enabled" } else { "disabled" }
            );
        }
    }
    Ok(())
}
