//! Plan §1.15 F/G — utterance endpoints. `GET /cooperation/utterances`
//! is the def table (built-in, or the `[cooperation.utterances]` override);
//! `GET /cooperation/utterances/recent` is the ring, newest first, for
//! frontends that poll instead of holding `/cooperation/events`.

use axum::{Json, extract::State};

use springtale_cooperation::utterance::{Utterance, UtteranceDefs};

use crate::api::state::AppState;

/// `GET /cooperation/utterances` — the def table this daemon speaks with.
#[utoipa::path(
    get, operation_id = "utterances_utterance_defs",
    path = "/cooperation/utterances",
    tag = "utterances",
    responses((status = 200, description = "Simlish utterance definitions", body = Object))
)]
pub async fn utterance_defs(State(state): State<AppState>) -> Json<UtteranceDefs> {
    Json((*state.runtime.utterance_defs).clone())
}

/// `GET /cooperation/utterances/recent` — the ring, newest first (cap 1000).
#[utoipa::path(
    get, operation_id = "utterances_recent",
    path = "/cooperation/utterances/recent",
    tag = "utterances",
    responses((status = 200, description = "Recent utterances", body = Vec<Object>))
)]
pub async fn recent(State(state): State<AppState>) -> Json<Vec<Utterance>> {
    Json(springtale_runtime::utterance_ring::recent(&state.runtime.utterances).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No `[cooperation.utterances]` in the config → the endpoint serves
    /// the built-in table, key for key.
    #[test]
    fn test_utterance_defs_no_override_serves_builtin_table() {
        let cfg: springtale_runtime::config::CooperationConfig =
            toml::from_str("").expect("empty cooperation config parses");
        let builtin = UtteranceDefs::default();
        let mut got: Vec<&String> = cfg.utterances.0.keys().collect();
        got.sort();
        let mut want: Vec<&String> = builtin.0.keys().collect();
        want.sort();
        assert_eq!(got, want);
        assert!(!got.is_empty());
        for (name, def) in &builtin.0 {
            let served = cfg.utterances.get(name).expect("served def");
            assert_eq!(served.label_key, def.label_key, "{name}");
            assert_eq!(served.frames, def.frames, "{name}");
        }
    }
}
