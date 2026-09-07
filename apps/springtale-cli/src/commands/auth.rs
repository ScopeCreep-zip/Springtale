//! `springtale auth` — the API tokens the daemon has issued.
//!
//! `springtale login` mints one and writes it to the token file; this
//! family is how you see the rest and revoke one you no longer trust.

use anyhow::Result;
use serde_json::Value;

use crate::cli::AuthAction;
use crate::client::Client;
use crate::output;

/// Handle auth subcommands.
pub async fn run(action: AuthAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        AuthAction::Tokens => {
            let body: Value = client.get("/auth/tokens").await?;
            output::emit(json_out, &body, tokens_table)?;
        }
        AuthAction::Revoke { id } => {
            let body: Value = client.delete(&format!("/auth/tokens/{id}")).await?;
            output::emit_status(json_out, &body, |_| format!("Revoked token {id}."))?;
        }
    }
    Ok(())
}

/// The `auth tokens` table — one row per issued API token.
fn tokens_table(v: &Value) -> String {
    let rows = output::array(v, "tokens")
        .iter()
        .map(|t| {
            vec![
                output::cell(t, "id"),
                output::cell(t, "name"),
                output::cell(t, "created_at"),
                output::cell(t, "last_used_at"),
            ]
        })
        .collect();
    output::rows_table(&["ID", "NAME", "CREATED", "LAST USED"], rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};
    use serde_json::json;

    fn tokens() -> Value {
        json!({
            "tokens": [{
                "id": "tok-1",
                "name": "springtale-cli@laptop",
                "created_at": "2026-09-01T09:00:00Z",
                "last_used_at": "2026-09-04T08:30:00Z",
            }]
        })
    }

    #[test]
    fn test_auth_tokens_json_shape_is_a_tokens_envelope_without_secrets() {
        let out = json_value(&tokens());
        assert_eq!(key_set(&out), ["tokens"]);
        assert!(out["tokens"].is_array());
        let token = &out["tokens"][0];
        assert!(token["id"].is_string());
        assert!(token["name"].is_string());
        assert!(token["created_at"].is_string());
        assert!(token["last_used_at"].is_string());
        // The token material itself is never listed.
        assert!(token.get("token").is_none());
    }

    #[test]
    fn test_tokens_table_reads_every_field_the_json_shape_promises() {
        let table = tokens_table(&tokens());
        for want in [
            "ID",
            "tok-1",
            "springtale-cli@laptop",
            "2026-09-04T08:30:00Z",
        ] {
            assert!(table.contains(want), "table lost {want}:\n{table}");
        }
    }

    #[test]
    fn test_tokens_table_is_empty_when_no_token_exists() {
        assert_eq!(tokens_table(&json!({ "tokens": [] })), "");
    }
}
