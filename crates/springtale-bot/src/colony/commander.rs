//! The colony commander — strategic cross-formation orchestration.

use springtale_ai::adapter::{AiOptions, AiRequest, ChatMessage};
use springtale_cooperation::cadence::IntentPattern;
use springtale_cooperation::command::parse_intent;
use springtale_cooperation::momentum::MomentumTier;

use crate::cooperation::formation::Formation;
use crate::error::BotError;
use crate::runtime::lifecycle::Bot;

/// One review of the whole colony. Lock-light: snapshot under a brief read,
/// decide (deterministic, or via the colony AI WITHOUT holding the lock), then
/// apply under a brief write. Guarded formations are never auto-touched.
pub async fn run(bot: &mut Bot) {
    // The colony's OWN adapter (strategic layer). Absent ⇒ deterministic policy.
    let cfg = match springtale_runtime::operations::config::get_config(
        bot.store.as_ref(),
        "ai:colony",
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "colony: ai:colony config read failed; using deterministic policy");
            serde_json::Value::Null
        }
    };

    // Snapshot under a short read lock — never held across the AI await.
    let snapshot: Vec<ColonyMember> = {
        let formations = bot.formations.read().await;
        formations
            .iter()
            .filter(|f| !f.paused)
            .map(ColonyMember::from_formation)
            .collect()
    };
    if snapshot.is_empty() {
        return;
    }

    // Decide per-formation intent moves: AI when configured, else deterministic.
    let decisions: Vec<(uuid::Uuid, IntentPattern)> = if cfg.is_null() {
        deterministic_policy(&snapshot)
    } else {
        match ai_policy(&cfg, &snapshot).await {
            Ok(moves) => moves,
            Err(e) => {
                tracing::warn!(error = %e, "colony: AI policy failed; deterministic fallback");
                deterministic_policy(&snapshot)
            }
        }
    };
    if decisions.is_empty() {
        return;
    }

    // Apply under a brief write lock. Guard mode vetoes auto-changes.
    let mut formations = bot.formations.write().await;
    for (fid, intent) in decisions {
        if let Some(f) = formations.iter_mut().find(|f| f.id.0 == fid) {
            if f.constraints.guard_mode {
                continue;
            }
            tracing::info!(formation = %fid, intent = ?intent, "colony: cross-formation move applied");
            crate::orchestrator::intent::apply_intent(f, intent);
            f.cascade_hit_streak = 0;
        }
    }
}

/// Minimal colony-visible summary of one formation. Owned so it survives the
/// lock release before the AI call.
struct ColonyMember {
    id: uuid::Uuid,
    intent_label: &'static str,
    tier: MomentumTier,
    cascade_streak: u32,
    operational: usize,
    guarded: bool,
    connectors: Vec<String>,
}

impl ColonyMember {
    fn from_formation(f: &Formation) -> Self {
        Self {
            id: f.id.0,
            intent_label: intent_label(&f.intent),
            tier: f.momentum.tier,
            cascade_streak: f.cascade_hit_streak,
            operational: f.operational_count(),
            guarded: f.constraints.guard_mode,
            connectors: f
                .members
                .iter()
                .flat_map(|m| m.capabilities.iter().map(|c| c.name.clone()))
                .collect(),
        }
    }
}

fn intent_label(intent: &IntentPattern) -> &'static str {
    match intent {
        IntentPattern::Reconnoiter { .. } => "Reconnoiter",
        IntentPattern::Execute { .. } => "Execute",
        IntentPattern::Stabilize { .. } => "Stabilize",
        IntentPattern::Surge { .. } => "Surge",
        IntentPattern::Dissolve { .. } => "Dissolve",
    }
}

/// Deterministic cross-formation policy (the Noop default): de-escalate
/// formations that are panicking (Cold tier + sustained cascade hits) to
/// Stabilize. Conservative — it only pulls back; it never escalates or
/// dissolves without an operator/AI in the loop.
fn deterministic_policy(snapshot: &[ColonyMember]) -> Vec<(uuid::Uuid, IntentPattern)> {
    snapshot
        .iter()
        .filter(|m| {
            !m.guarded
                && m.tier == MomentumTier::Cold
                && m.cascade_streak >= super::DEESCALATE_STREAK
                && m.intent_label != "Stabilize"
        })
        .map(|m| {
            (
                m.id,
                IntentPattern::Stabilize {
                    reason: "colony de-escalation".into(),
                },
            )
        })
        .collect()
}

/// AI cross-formation policy: ask the colony adapter for per-formation intent
/// recommendations, validated against the live snapshot before applying.
async fn ai_policy(
    cfg: &serde_json::Value,
    snapshot: &[ColonyMember],
) -> Result<Vec<(uuid::Uuid, IntentPattern)>, BotError> {
    let adapter = springtale_runtime::operations::config::build_adapter(cfg)
        .await
        .map_err(|e| BotError::Handler(format!("colony adapter: {e}")))?;

    let roster = snapshot
        .iter()
        .map(|m| {
            format!(
                "- {} intent={} tier={:?} cascade={} operational={} connectors=[{}]",
                m.id,
                m.intent_label,
                m.tier,
                m.cascade_streak,
                m.operational,
                m.connectors.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let system = "You are the colony commander for Springtale, orchestrating ACROSS \
        formations (teams of bots working real infrastructure). For each formation, \
        decide whether to change its intent. Valid intents: Reconnoiter (monitor, \
        read-only), Execute (take action), Stabilize (hold / cool down), Surge \
        (maximum effort). Only recommend a change when warranted by the roster. \
        Respond ONLY with a JSON array of moves, e.g. \
        [{\"formation_id\":\"<uuid>\",\"intent\":\"Execute\"}]. Return [] for no changes.";
    let request = AiRequest::Chat {
        messages: vec![
            ChatMessage::text("system", system.to_owned()),
            ChatMessage::text("user", format!("Colony roster:\n{roster}")),
        ],
    };
    let response = adapter
        .complete(request, AiOptions::default())
        .await
        .map_err(|e| BotError::Handler(format!("colony AI call: {e}")))?;

    parse_colony_moves(&response.content, snapshot)
}

/// Parse the colony AI's JSON moves, keeping only well-formed entries that
/// target a live formation in the snapshot.
fn parse_colony_moves(
    content: &str,
    snapshot: &[ColonyMember],
) -> Result<Vec<(uuid::Uuid, IntentPattern)>, BotError> {
    let trimmed = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(trimmed)
        .map_err(|e| BotError::Handler(format!("colony move parse: {e}")))?;

    let live: std::collections::HashSet<uuid::Uuid> = snapshot.iter().map(|m| m.id).collect();
    let mut out = Vec::new();
    for v in parsed {
        let Some(fid) = v
            .get("formation_id")
            .and_then(|s| s.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
        else {
            continue;
        };
        if !live.contains(&fid) {
            tracing::warn!(formation = %fid, "colony: AI proposed move for unknown formation — skipping");
            continue;
        }
        let Some(intent_str) = v.get("intent").and_then(|s| s.as_str()) else {
            continue;
        };
        out.push((fid, parse_intent(intent_str)));
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn member(
        tier: MomentumTier,
        cascade: u32,
        intent: &'static str,
        guarded: bool,
    ) -> ColonyMember {
        ColonyMember {
            id: uuid::Uuid::new_v4(),
            intent_label: intent,
            tier,
            cascade_streak: cascade,
            operational: 2,
            guarded,
            connectors: vec!["connector-test".into()],
        }
    }

    #[test]
    fn deterministic_deescalates_panicking_cold_formation() {
        let m = member(MomentumTier::Cold, 3, "Execute", false);
        let id = m.id;
        let moves = deterministic_policy(&[m]);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].0, id);
        assert!(matches!(moves[0].1, IntentPattern::Stabilize { .. }));
    }

    #[test]
    fn deterministic_leaves_healthy_and_guarded_formations() {
        // Healthy (Hot, no cascade) and a guarded panicking one — neither moves.
        let healthy = member(MomentumTier::Hot, 0, "Execute", false);
        let guarded = member(MomentumTier::Cold, 5, "Execute", true);
        assert!(deterministic_policy(&[healthy, guarded]).is_empty());
    }

    #[test]
    fn parse_colony_moves_keeps_only_live_formations() {
        let m = member(MomentumTier::Cold, 1, "Reconnoiter", false);
        let live_id = m.id;
        let snapshot = vec![m];
        let json = format!(
            "[{{\"formation_id\":\"{live_id}\",\"intent\":\"Execute\"}},\
             {{\"formation_id\":\"{}\",\"intent\":\"Surge\"}}]",
            uuid::Uuid::new_v4()
        );
        let moves = parse_colony_moves(&json, &snapshot).unwrap();
        assert_eq!(moves.len(), 1, "unknown formation dropped");
        assert_eq!(moves[0].0, live_id);
        assert!(matches!(moves[0].1, IntentPattern::Execute { .. }));
    }
}
