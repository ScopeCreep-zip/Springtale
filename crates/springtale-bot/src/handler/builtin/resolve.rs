//! Formation name resolution for chat commands.
//!
//! Chat names a formation the way a person does — "research", "Research
//! Squad" — while the runtime keys on ids. Resolution is exact first,
//! then case-insensitive prefix, and an ambiguous prefix is an error
//! rather than a guess: pausing the wrong formation is not recoverable
//! by re-reading a chat line.

use springtale_runtime::operations::formations::{FormationInfo, list_formations};
use springtale_runtime::state::RuntimeState;

use crate::error::BotError;

/// Resolve a user-typed formation name to `(id, name)`.
pub async fn resolve_formation(
    state: &RuntimeState,
    query: &str,
) -> Result<(String, String), BotError> {
    let formations = list_formations(state)
        .await
        .map_err(|e| BotError::Handler(e.to_string()))?;
    match pick(&formations, query) {
        Ok(f) => Ok((f.id.clone(), f.name.clone())),
        Err(e) => Err(e),
    }
}

/// Exact match, then case-insensitive prefix; ambiguity is an error.
pub fn pick<'a>(
    formations: &'a [FormationInfo],
    query: &str,
) -> Result<&'a FormationInfo, BotError> {
    let q = query.trim();
    if q.is_empty() {
        return Err(BotError::Handler("name a formation".to_owned()));
    }
    if let Some(exact) = formations.iter().find(|f| f.name == q) {
        return Ok(exact);
    }
    let lower = q.to_lowercase();
    let hits: Vec<&FormationInfo> = formations
        .iter()
        .filter(|f| f.name.to_lowercase().starts_with(&lower) || f.id == q)
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(BotError::Handler(format!("no formation matches '{q}'"))),
        many => {
            let names: Vec<&str> = many.iter().map(|f| f.name.as_str()).collect();
            Err(BotError::Handler(format!(
                "'{q}' matches {} formations ({}) — say the whole name",
                many.len(),
                names.join(", ")
            )))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn info(id: &str, name: &str) -> FormationInfo {
        FormationInfo {
            id: id.to_owned(),
            name: name.to_owned(),
            intent: "reconnoiter".to_owned(),
            status: "active".to_owned(),
            member_count: 0,
            operational_count: 0,
            members: vec![],
            momentum_tier: "Cold".to_owned(),
            momentum_label: "Cold".to_owned(),
            momentum_consecutive_successes: 0,
            momentum_interference_count: 0,
            momentum_successes_to_next_tier: None,
            capabilities: vec![],
            guard_status: "--".to_owned(),
            guard_engaged: false,
            rally_tokens: 0,
            rally_max: 0,
        }
    }

    #[test]
    fn test_pick_exact_name_wins() {
        let f = vec![info("1", "Research"), info("2", "Research Squad")];
        assert_eq!(pick(&f, "Research").expect("exact").id, "1");
    }

    #[test]
    fn test_pick_case_insensitive_prefix_resolves() {
        let f = vec![info("1", "Research Squad"), info("2", "Watchtower")];
        assert_eq!(pick(&f, "research").expect("prefix").id, "1");
    }

    #[test]
    fn test_pick_ambiguous_prefix_errors() {
        let f = vec![info("1", "Research Squad"), info("2", "Research Team")];
        let err = pick(&f, "res").expect_err("ambiguous");
        assert!(err.to_string().contains("matches 2 formations"));
    }

    #[test]
    fn test_pick_unknown_name_errors() {
        let f = vec![info("1", "Research Squad")];
        assert!(pick(&f, "nope").is_err());
    }
}
