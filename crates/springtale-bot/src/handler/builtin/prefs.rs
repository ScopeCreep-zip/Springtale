//! `/prefs` — get or set user preferences.

use async_trait::async_trait;

use crate::error::BotError;
use crate::handler::registry::{Handler, HandlerContext, HandlerResult};

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
                let mut current = prefs.unwrap_or_else(|| default_prefs(&ctx.user_id));

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
                let p = prefs.unwrap_or_else(|| default_prefs(&ctx.user_id));
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

fn default_prefs(user_id: &str) -> springtale_store::UserPrefsRow {
    springtale_store::UserPrefsRow {
        user_id: user_id.to_owned(),
        timezone: "UTC".into(),
        language: "en".into(),
        notifications_enabled: false,
        updated_at: chrono::Utc::now(),
    }
}
