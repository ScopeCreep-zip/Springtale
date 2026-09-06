//! `springtale workspace` — the external workspaces (servers, repos,
//! channels) a formation's connectors can reach.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::WorkspaceAction;
use crate::client::Client;
use crate::commands::json_input;
use crate::output;

/// The workspace directory. Query filters are appended to it.
const WORKSPACES: &str = "/workspaces";

/// Handle workspace subcommands.
pub async fn run(action: WorkspaceAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        WorkspaceAction::List {
            formation,
            connector,
        } => {
            let mut query = format!("?formation_id={formation}");
            if let Some(connector) = &connector {
                query.push_str(&format!("&connector={connector}"));
            }
            let body: Value = client.get(&format!("{WORKSPACES}{query}")).await?;
            output::emit(json_out, &body, workspace_table)?;
        }
        WorkspaceAction::Scan {
            formation,
            connector,
        } => {
            let body: Value = client
                .post(
                    "/workspaces/scan",
                    &json!({ "formation_id": formation, "connector_name": connector }),
                )
                .await?;
            output::emit(json_out, &body, workspace_table)?;
        }
        WorkspaceAction::Add {
            formation,
            key,
            name,
            connector,
            kind,
        } => {
            let body: Value = client
                .post(
                    WORKSPACES,
                    &json!({
                        "formation_id": formation,
                        "workspace_key": key,
                        "display_name": name,
                        "connector_name": connector,
                        "kind": kind,
                    }),
                )
                .await?;
            output::emit_status(json_out, &body, |_| format!("Added workspace {key}."))?;
        }
        WorkspaceAction::Remove { formation, key } => {
            let body: Value = client
                .delete(&format!(
                    "{WORKSPACES}?formation_id={formation}&workspace_key={key}"
                ))
                .await?;
            output::emit_status(json_out, &body, |_| format!("Removed workspace {key}."))?;
        }
        WorkspaceAction::OnboardUrl {
            connector,
            config,
            payload,
        } => {
            let body: Value = client
                .post(
                    "/workspaces/onboard-url",
                    &json!({
                        "connector_name": connector,
                        "config": json_input::load(&config)?,
                        "payload": json_input::load_or_empty(payload)?,
                    }),
                )
                .await?;
            output::emit(json_out, &body, |v| output::cell(v, "url"))?;
        }
        WorkspaceAction::Onboard {
            session,
            connector,
            config,
            payload,
        } => {
            // The route answers SSE; `stream` hands back the raw body so
            // the progress frames land on stdout as they arrive.
            let response = client
                .post_stream(
                    "/workspaces/onboard",
                    &json!({
                        "session_id": session,
                        "connector_name": connector,
                        "config": json_input::load(&config)?,
                        "payload": json_input::load_or_empty(payload)?,
                    }),
                )
                .await?;
            print_frames(response).await?;
        }
    }
    Ok(())
}

/// Print each SSE `data:` frame the onboard stream sends, one per line.
async fn print_frames(response: reqwest::Response) -> Result<()> {
    use futures_util::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| anyhow::anyhow!("onboard stream read error: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(pos) = buffer.find("\n\n") {
            let frame: String = buffer.drain(..pos + 2).collect();
            for line in frame.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    println!("{data}");
                }
            }
        }
    }
    Ok(())
}

/// The shared workspace table: `list` and `scan` return the same rows.
fn workspace_table(v: &Value) -> String {
    let empty = Vec::new();
    let rows = v
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|w| {
            vec![
                output::cell(w, "workspace_key"),
                output::cell(w, "display_name"),
                output::cell(w, "connector_name"),
                output::cell(w, "kind"),
            ]
        })
        .collect();
    output::rows_table(&["KEY", "NAME", "CONNECTOR", "KIND"], rows)
}
