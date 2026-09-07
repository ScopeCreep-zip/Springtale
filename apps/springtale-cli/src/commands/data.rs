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
                let done = exported_body(&path);
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
            output::emit_status(json_out, &stats, import_line)?;
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

/// The `data export --output` body — the export itself went to the
/// file, so `--json` reports where it landed.
fn exported_body(path: &std::path::Path) -> Value {
    json!({ "exported_to": path.display().to_string() })
}

/// The `data import` notice, read off the daemon's insert counts.
fn import_line(v: &Value) -> String {
    format!(
        "Imported: {} rules, {} connectors, {} events",
        v["rules_inserted"], v["connectors_inserted"], v["events_inserted"]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_data_export_to_file_json_shape_names_the_destination() {
        let out = json_value(&exported_body(std::path::Path::new("/tmp/export.json")));
        assert_eq!(key_set(&out), ["exported_to"]);
        assert!(out["exported_to"].is_string());
        assert_eq!(out["exported_to"], "/tmp/export.json");
    }

    #[test]
    fn test_data_export_to_stdout_json_is_the_export_document_itself() {
        // No envelope: the export *is* the payload.
        let export = json!({
            "rules": [{ "id": "r-1" }],
            "connectors": [{ "name": "telegram" }],
            "events": [],
        });
        assert_eq!(json_value(&export), export);
    }

    #[test]
    fn test_data_import_json_shape_reports_three_insert_counts() {
        let stats = json!({
            "rules_inserted": 2,
            "connectors_inserted": 1,
            "events_inserted": 40,
        });
        let out = json_value(&stats);
        assert_eq!(
            key_set(&out),
            ["connectors_inserted", "events_inserted", "rules_inserted"]
        );
        assert!(out["rules_inserted"].is_number());
        assert!(out["connectors_inserted"].is_number());
        assert!(out["events_inserted"].is_number());
        assert_eq!(
            import_line(&stats),
            "Imported: 2 rules, 1 connectors, 40 events"
        );
    }
}
