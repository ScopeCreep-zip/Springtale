//! `springtale data` — data export, over the daemon.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::DataAction;
use crate::client::Client;
use crate::output;

/// Handle data subcommands.
pub async fn run(action: DataAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    match action {
        DataAction::Export { output, encrypt } => {
            if encrypt {
                anyhow::bail!("encrypted export requires travel mode (springtale travel prepare)");
            }
            let data: Value = client.post("/data/export", &json!({})).await?;
            if let Some(path) = output {
                // Write with 0o600 permissions (architecture doc §8.2)
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&path)?;
                let mut writer = std::io::BufWriter::new(file);
                writer.write_all(serde_json::to_string_pretty(&data)?.as_bytes())?;
                let done = json!({ "exported_to": path.display().to_string() });
                output::emit_status(json_out, &done, |v| {
                    format!("Exported to: {}", output::cell(v, "exported_to"))
                })?;
            } else {
                // The export *is* the payload, so both forms print it —
                // the flag still routes through the one helper.
                output::emit(json_out, &data, |v| {
                    serde_json::to_string_pretty(v).unwrap_or_default()
                })?;
            }
        }
        DataAction::Import { input } => {
            let text = std::fs::read_to_string(&input)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", input.display()))?;
            let export: Value = serde_json::from_str(&text)
                .map_err(|e| anyhow::anyhow!("invalid export file: {e}"))?;
            let stats: Value = client.post("/data/import", &export).await?;
            output::emit_status(json_out, &stats, |v| {
                format!(
                    "Imported: {} rules, {} connectors, {} events",
                    v["rules_inserted"], v["connectors_inserted"], v["events_inserted"]
                )
            })?;
        }
        DataAction::Purge { yes } => {
            // Irreversible. The flag is required here and the route
            // demands an explicit `confirm`, so neither a slip of the
            // shell nor a stray POST can wipe a store.
            if !yes {
                anyhow::bail!(
                    "refusing to purge without --yes (this deletes every rule, event, and session)"
                );
            }
            let body: Value = client
                .post("/data/purge", &json!({ "confirm": true }))
                .await?;
            output::emit_status(json_out, &body, |_| {
                "All user data purged. Vault intact.".to_owned()
            })?;
        }
    }
    Ok(())
}
