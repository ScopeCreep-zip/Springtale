use async_trait::async_trait;

use super::registry::{Handler, HandlerContext, HandlerResult};
use crate::error::BotError;

/// Command names reserved for builtins. Cannot be overridden by connectors.
pub const BUILTIN_COMMANDS: &[&str] = &["help", "status", "rules", "connectors", "prefs", "alias"];

/// `/help` — Lists all available commands.
pub struct HelpHandler;

#[async_trait]
impl Handler for HelpHandler {
    async fn handle(&self, _args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let _ = ctx; // Will use registry reference in full impl
        Ok(HandlerResult {
            response: "Available commands: /help, /status, /rules, /connectors, /prefs, /alias"
                .into(),
        })
    }

    fn description(&self) -> &str {
        "List available commands"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}

/// `/status` — Shows bot health and uptime.
pub struct StatusHandler;

#[async_trait]
impl Handler for StatusHandler {
    async fn handle(&self, _args: &str, _ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        Ok(HandlerResult {
            response: "Bot is running.".into(),
        })
    }

    fn description(&self) -> &str {
        "Show bot status"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}

/// `/rules` — Lists active rules from the engine.
pub struct RulesHandler;

#[async_trait]
impl Handler for RulesHandler {
    async fn handle(&self, _args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let engine = ctx.engine.read().await;
        let rules = engine.list_rules();
        if rules.is_empty() {
            return Ok(HandlerResult {
                response: "No rules configured.".into(),
            });
        }
        let mut lines = Vec::with_capacity(rules.len() + 1);
        lines.push(format!("{} active rules:", rules.len()));
        for rule in rules {
            let status = if rule.status == springtale_core::rule::types::RuleStatus::Enabled {
                "enabled"
            } else {
                "disabled"
            };
            lines.push(format!("  {} [{}]", rule.name, status));
        }
        Ok(HandlerResult {
            response: lines.join("\n"),
        })
    }

    fn description(&self) -> &str {
        "List active rules"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}

/// `/connectors` — Lists installed connectors.
pub struct ConnectorsHandler;

#[async_trait]
impl Handler for ConnectorsHandler {
    async fn handle(&self, _args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let registry = ctx.registry.read().await;
        let list = registry.list();
        if list.is_empty() {
            return Ok(HandlerResult {
                response: "No connectors installed.".into(),
            });
        }
        let mut lines = Vec::with_capacity(list.len() + 1);
        lines.push(format!("{} connectors:", list.len()));
        for (name, enabled) in &list {
            let status = if *enabled { "enabled" } else { "disabled" };
            lines.push(format!("  {} [{}]", name, status));
        }
        Ok(HandlerResult {
            response: lines.join("\n"),
        })
    }

    fn description(&self) -> &str {
        "List installed connectors"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}

/// `/prefs` — Get or set user preferences.
pub struct PrefsHandler;

#[async_trait]
impl Handler for PrefsHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let prefs = ctx.store.get_user_prefs(&ctx.user_id).await?;
        let parts: Vec<&str> = args.trim().splitn(3, ' ').collect();

        match parts.first().copied() {
            Some("set") if parts.len() == 3 => {
                let key = parts[1];
                let value = parts[2];
                let mut current = prefs.unwrap_or_else(|| springtale_store::UserPrefsRow {
                    user_id: ctx.user_id.clone(),
                    timezone: "UTC".into(),
                    language: "en".into(),
                    notifications_enabled: false,
                    updated_at: chrono::Utc::now(),
                });

                match key {
                    "timezone" => current.timezone = value.into(),
                    "language" => current.language = value.into(),
                    "notifications" => {
                        current.notifications_enabled = value == "on" || value == "true";
                    }
                    _ => {
                        return Ok(HandlerResult {
                            response: format!(
                                "Unknown preference: {key}. Options: timezone, language, notifications"
                            ),
                        });
                    }
                }
                current.updated_at = chrono::Utc::now();
                ctx.store.upsert_user_prefs(&current).await?;
                Ok(HandlerResult {
                    response: format!("Set {key} = {value}"),
                })
            }
            _ => {
                let p = prefs.unwrap_or_else(|| springtale_store::UserPrefsRow {
                    user_id: ctx.user_id.clone(),
                    timezone: "UTC".into(),
                    language: "en".into(),
                    notifications_enabled: false,
                    updated_at: chrono::Utc::now(),
                });
                Ok(HandlerResult {
                    response: format!(
                        "Preferences:\n  timezone: {}\n  language: {}\n  notifications: {}",
                        p.timezone,
                        p.language,
                        if p.notifications_enabled { "on" } else { "off" },
                    ),
                })
            }
        }
    }

    fn description(&self) -> &str {
        "Get or set user preferences"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}

/// `/alias` — Manage command aliases.
pub struct AliasHandler;

#[async_trait]
impl Handler for AliasHandler {
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError> {
        let parts: Vec<&str> = args.trim().splitn(3, ' ').collect();
        match parts.first().copied() {
            Some("set") if parts.len() == 3 => {
                let alias = parts[1].trim();
                let target = parts[2].trim();
                if alias.is_empty() || target.is_empty() {
                    return Ok(HandlerResult {
                        response: "Usage: /alias set <alias> <command>".into(),
                    });
                }
                ctx.store.upsert_alias(alias, target, &ctx.user_id).await?;
                Ok(HandlerResult {
                    response: format!("Alias set: /{alias} → /{target}"),
                })
            }
            Some("remove") if parts.len() >= 2 => {
                let alias = parts[1];
                ctx.store.delete_alias(alias).await?;
                Ok(HandlerResult {
                    response: format!("Alias removed: {alias}"),
                })
            }
            _ => {
                let aliases = ctx.store.list_aliases().await?;
                if aliases.is_empty() {
                    return Ok(HandlerResult {
                        response: "No aliases defined. Use: /alias set <alias> <command>".into(),
                    });
                }
                let mut lines = vec!["Aliases:".to_owned()];
                for (alias, target) in &aliases {
                    lines.push(format!("  /{alias} → /{target}"));
                }
                Ok(HandlerResult {
                    response: lines.join("\n"),
                })
            }
        }
    }

    fn description(&self) -> &str {
        "Manage command aliases"
    }

    fn is_builtin(&self) -> bool {
        true
    }
}

/// Register all builtin handlers into a registry.
pub fn register_builtins(registry: &mut super::registry::HandlerRegistry) -> Result<(), BotError> {
    registry.register("help".into(), Box::new(HelpHandler))?;
    registry.register("status".into(), Box::new(StatusHandler))?;
    registry.register("rules".into(), Box::new(RulesHandler))?;
    registry.register("connectors".into(), Box::new(ConnectorsHandler))?;
    registry.register("prefs".into(), Box::new(PrefsHandler))?;
    registry.register("alias".into(), Box::new(AliasHandler))?;
    Ok(())
}
