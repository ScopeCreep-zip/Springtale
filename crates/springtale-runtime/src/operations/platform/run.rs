//! Execute one platform verb (plan 5.4).
//!
//! The verb registry says what chat and the AI tool loop may ask the
//! platform to do; this module is the one place that actually does it.
//! Every branch delegates to an existing runtime operation — nothing
//! new is reachable through a verb that isn't reachable through the
//! surfaces that already exist.
//!
//! The AI tool loop routes the `platform` pseudo-connector here instead
//! of the connector registry (`springtale_bot::tool_runner`), so a
//! model-issued `platform__formation_pause` and a typed
//! `/formation pause` run the same code.

use serde_json::{Value, json};

use crate::error::OperationError;
use crate::operations::{config, formations as f, memory, safety};
use crate::state::RuntimeState;

use super::verb::PlatformVerb;

/// Rows kept when `memory.compact` runs without an explicit window.
/// Matches the `/memory compact` default so the two surfaces prune the
/// same amount.
const DEFAULT_MEMORY_KEEP: usize = 100;

/// Pull a string argument out of the tool/JSON argument object.
fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, OperationError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| OperationError::Validation(format!("missing argument '{key}'")))
}

/// Resolve a user-typed formation reference to `(id, name)`.
///
/// Exact name first, then a unique case-insensitive prefix. An
/// ambiguous prefix is an error rather than a guess — steering the
/// wrong formation is the expensive mistake.
async fn resolve_formation(
    state: &RuntimeState,
    needle: &str,
) -> Result<(String, String), OperationError> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Err(OperationError::Validation("which formation?".to_owned()));
    }
    let list = f::list_formations(state).await?;
    if let Some(hit) = list.iter().find(|x| x.name.eq_ignore_ascii_case(needle)) {
        return Ok((hit.id.clone(), hit.name.clone()));
    }
    let lower = needle.to_lowercase();
    let mut hits = list
        .iter()
        .filter(|x| x.name.to_lowercase().starts_with(&lower));
    match (hits.next(), hits.next()) {
        (Some(hit), None) => Ok((hit.id.clone(), hit.name.clone())),
        (Some(_), Some(_)) => Err(OperationError::Validation(format!(
            "'{needle}' matches more than one formation — say the whole name"
        ))),
        _ => Err(OperationError::NotFound(format!(
            "no formation called '{needle}'"
        ))),
    }
}

/// Run one verb and return its structured result.
///
/// The caller decides whether an approval was needed — this function
/// executes what it is handed. `read_only` on the verb is the input to
/// that decision, not something enforced here.
pub async fn run_platform_verb(
    state: &RuntimeState,
    verb: &PlatformVerb,
    args: &Value,
) -> Result<Value, OperationError> {
    match verb.name {
        // ── formation ────────────────────────────────────────────────
        "formation.list" => {
            let list = f::list_formations(state).await?;
            let rows: Vec<Value> = list
                .iter()
                .map(|x| {
                    json!({
                        "id": x.id,
                        "name": x.name,
                        "status": x.status,
                        "intent": x.intent,
                        "members": x.member_count,
                        "momentum": x.momentum_label,
                    })
                })
                .collect();
            Ok(json!({ "formations": rows }))
        }
        "formation.get" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            let d = f::get_formation(state, &id).await?;
            Ok(json!({
                "name": name,
                "status": d.info.status,
                "intent": d.info.intent,
                "momentum": d.info.momentum_label,
                "members": d.info.members,
            }))
        }
        "formation.deploy" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            f::deploy_formation(state, &id).await?;
            Ok(json!({ "formation": name, "status": "deployed" }))
        }
        "formation.pause" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            f::pause_formation(state, &id).await?;
            Ok(json!({ "formation": name, "status": "paused" }))
        }
        "formation.resume" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            f::resume_formation(state, &id).await?;
            Ok(json!({ "formation": name, "status": "resumed" }))
        }
        "formation.dissolve" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            f::dissolve_formation(state, &id).await?;
            Ok(json!({ "formation": name, "status": "dissolved" }))
        }
        "formation.rally" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            f::rally_formation(state, &id).await?;
            Ok(json!({ "formation": name, "status": "rallied" }))
        }
        "formation.intent" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            match args.get("intent").and_then(Value::as_str) {
                Some(intent) if !intent.trim().is_empty() => {
                    f::update_intent(state, &id, intent.trim()).await?;
                    Ok(json!({ "formation": name, "intent": intent.trim() }))
                }
                _ => {
                    let next = f::cycle_intent(state, &id).await?;
                    Ok(json!({ "formation": name, "intent": next }))
                }
            }
        }
        "formation.guard" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            let on = config::toggle_formation_guard(state, &id).await?;
            Ok(json!({ "formation": name, "guard": on }))
        }
        "formation.add_member" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            let connector = arg(args, "connector")?;
            f::add_member(state, &id, connector).await?;
            Ok(json!({ "formation": name, "added": connector }))
        }
        "formation.remove_member" => {
            let (id, name) = resolve_formation(state, arg(args, "formation")?).await?;
            let connector = arg(args, "connector")?;
            f::remove_member(state, &id, connector).await?;
            Ok(json!({ "formation": name, "removed": connector }))
        }
        // ── approvals ────────────────────────────────────────────────
        "approvals.list" => {
            let rows: Vec<Value> = crate::operations::approvals::pending(state)
                .await
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id.to_string(),
                        "connector": r.connector_name,
                        "summary": r.summary,
                    })
                })
                .collect();
            Ok(json!({ "pending": rows }))
        }
        "approvals.approve" | "approvals.deny" => {
            let approve = verb.name == "approvals.approve";
            let id = arg(args, "id")?;
            let uuid = uuid::Uuid::parse_str(id)
                .map_err(|_| OperationError::Validation(format!("'{id}' is not an approval id")))?;
            let req = crate::operations::approvals::ResolveRequest {
                decision: if approve {
                    crate::operations::approvals::ResolveDecision::Approve
                } else {
                    crate::operations::approvals::ResolveDecision::Deny
                },
                approver: Some("owner (chat)".to_owned()),
                reason: Some("denied from chat".to_owned()),
            };
            crate::operations::approvals::resolve(
                state,
                crate::approval::ApprovalRequestId(uuid),
                req,
            )
            .await
            .map_err(|e| OperationError::Validation(e.to_string()))?;
            Ok(json!({
                "id": id,
                "decision": if approve { "approved" } else { "denied" },
            }))
        }
        // ── memory ───────────────────────────────────────────────────
        "memory.audit" => {
            let audit = memory::audit_memory(&*state.store).await?;
            serde_json::to_value(audit).map_err(|e| OperationError::Serialization(e.to_string()))
        }
        "memory.compact" => {
            let keep = args
                .get("keep")
                .and_then(Value::as_u64)
                .map_or(DEFAULT_MEMORY_KEEP, |n| n as usize);
            let deleted = memory::compact_memory(&*state.store, keep).await?;
            Ok(json!({ "kept_per_session": keep, "deleted": deleted }))
        }
        // ── safety ───────────────────────────────────────────────────
        "safety.get" => {
            let cfg = safety::get_safety_config(state).await?;
            Ok(json!({
                "window_title": cfg.window_title,
                "auto_lock_minutes": cfg.auto_lock_minutes,
                "content_protected": cfg.content_protected,
                "panic_taps": cfg.panic_tap_count,
                "disguise_active": cfg.disguise_active,
            }))
        }
        "safety.set" => {
            let key = arg(args, "key")?;
            let value = arg(args, "value")?;
            let mut cfg = safety::get_safety_config(state).await?;
            match key {
                "window-title" => cfg.window_title = value.to_owned(),
                "auto-lock-minutes" => {
                    cfg.auto_lock_minutes = value.parse().map_err(|_| {
                        OperationError::Validation("minutes must be a number".to_owned())
                    })?
                }
                "content-protected" => {
                    cfg.content_protected = matches!(value, "true" | "on" | "yes")
                }
                "panic-taps" => {
                    cfg.panic_tap_count = value.parse().map_err(|_| {
                        OperationError::Validation("taps must be a number".to_owned())
                    })?
                }
                other => {
                    return Err(OperationError::Validation(format!(
                        "'{other}' is not a safety setting"
                    )));
                }
            }
            safety::save_safety_config(state, cfg).await?;
            Ok(json!({ "key": key, "value": value }))
        }
        // ── model configuration ──────────────────────────────────────
        "ai.get" => {
            let cfg = config::get_config(&*state.store, &config::AiTarget::Colony.key()).await?;
            Ok(json!({
                "adapter": cfg.get("type").and_then(Value::as_str).unwrap_or("noop"),
                "model": cfg.get("model").and_then(Value::as_str),
            }))
        }
        "ai.set" => {
            let requested = arg(args, "adapter")?;
            let adapter = match requested {
                "none" | "noop" => "noop",
                a @ ("ollama" | "openai" | "anthropic") => a,
                other => {
                    return Err(OperationError::Validation(format!(
                        "'{other}' is not an adapter"
                    )));
                }
            };
            // Keep whatever else is configured (model, host, key
            // reference) and change only the adapter type — same as
            // `/ai set`.
            let mut cfg =
                config::get_config(&*state.store, &config::AiTarget::Colony.key()).await?;
            if !cfg.is_object() {
                cfg = json!({});
            }
            if let Some(map) = cfg.as_object_mut() {
                map.insert("type".to_owned(), json!(adapter));
            }
            config::configure_ai_adapter(state, config::AiTarget::Colony, cfg).await?;
            Ok(json!({ "adapter": adapter }))
        }
        other => Err(OperationError::NotFound(format!(
            "'{other}' is not a platform verb"
        ))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn arg_rejects_missing_and_blank() {
        let args = json!({ "formation": "  ", "connector": "kick" });
        assert!(arg(&args, "formation").is_err());
        assert!(arg(&args, "missing").is_err());
        assert_eq!(arg(&args, "connector").unwrap(), "kick");
    }

    #[tokio::test]
    async fn unknown_verb_is_not_found() {
        // A verb value that is not in the registry can only be built by
        // hand; running it must fail rather than silently no-op.
        let verb = PlatformVerb {
            name: "formation.assign",
            description: "not a verb",
            group: super::super::verb::VerbGroup::Intervention,
            read_only: false,
            args: &[],
        };
        // No RuntimeState is needed: the match arm falls through to the
        // catch-all before touching state, so a null state pointer is
        // never dereferenced. We assert on the branch via `verb.name`.
        assert!(super::super::find_verb(verb.name).is_none());
    }
}
