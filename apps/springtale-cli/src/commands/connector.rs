//! `springtale connector` — connector management, over the daemon.
//!
//! `sign` is the one local verb: it signs a manifest file with the
//! local identity from the vault and never touches the daemon.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::ConnectorAction;
use crate::client::Client;
use crate::output;

/// Handle connector subcommands.
pub async fn run(action: ConnectorAction, json_out: bool) -> Result<()> {
    // `sign` is a local file + vault operation with no daemon route, so
    // it must not require a reachable daemon or an API token.
    if let ConnectorAction::Sign { path } = &action {
        return sign(path, json_out);
    }

    let client = Client::from_config()?;
    match action {
        ConnectorAction::List => {
            let body: Value = client.get("/connectors").await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "connectors")
                    .iter()
                    .map(|c| {
                        vec![
                            output::cell(c, "name"),
                            output::cell(c, "version"),
                            output::cell(c, "enabled"),
                        ]
                    })
                    .collect();
                output::rows_table(&["NAME", "VERSION", "ENABLED"], rows)
            })?;
        }
        ConnectorAction::Enable { name } => {
            let body: Value = client
                .post(&format!("/connectors/{name}/enable"), &json!({}))
                .await?;
            output::emit(json_out, &body, |_| format!("Enabled connector: {name}"))?;
        }
        ConnectorAction::Disable { name } => {
            let body: Value = client
                .post(&format!("/connectors/{name}/disable"), &json!({}))
                .await?;
            output::emit(json_out, &body, |_| format!("Disabled connector: {name}"))?;
        }
        ConnectorAction::Remove { name } => {
            let body: Value = client.delete(&format!("/connectors/{name}")).await?;
            output::emit(json_out, &body, |_| format!("Removed connector: {name}"))?;
        }
        ConnectorAction::Install { path } => {
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                anyhow::anyhow!("failed to read manifest at {}: {e}", path.display())
            })?;
            let manifest: springtale_connector::ConnectorManifest = toml::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("failed to parse manifest TOML: {e}"))?;
            let body: Value = client.post("/connectors/install", &manifest).await?;
            output::emit(json_out, &body, |v| {
                format!("Installed connector: {}", output::cell(v, "installed"))
            })?;
        }
        ConnectorAction::Sign { .. } => unreachable!("handled above"),
    }
    Ok(())
}

/// Sign a connector manifest with the local identity, in place.
fn sign(path: &std::path::Path, json_out: bool) -> Result<()> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read manifest at {}: {e}", path.display()))?;
    let mut manifest: springtale_connector::ConnectorManifest = toml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("failed to parse manifest TOML: {e}"))?;
    springtale_connector::manifest::verify::verify_manifest(&manifest)
        .map_err(|e| anyhow::anyhow!("manifest invalid: {e}"))?;

    let keypair = crate::commands::author::load_local_identity()?;
    let signature = springtale_connector::manifest::sign_manifest(&mut manifest, &keypair)
        .map_err(|e| anyhow::anyhow!("failed to sign manifest: {e}"))?;

    let signed = toml::to_string_pretty(&manifest)
        .map_err(|e| anyhow::anyhow!("failed to serialize signed manifest: {e}"))?;
    std::fs::write(path, signed)
        .map_err(|e| anyhow::anyhow!("failed to write manifest at {}: {e}", path.display()))?;

    let pubkey_hex = hex::encode(keypair.verifying_key().to_bytes());
    let body = json!({
        "path": path.display().to_string(),
        "author": manifest.author,
        "pubkey": pubkey_hex,
        "signature": signature,
    });
    output::emit(json_out, &body, |v| {
        let author = output::cell(v, "author");
        format!(
            "Signed {}\n  author:    {author}\n  pubkey:    {}\n  signature: {}\n  Install verifies against `trusted-author:{author}` — register it with `springtale author add {author} --self`.",
            output::cell(v, "path"),
            output::cell(v, "pubkey"),
            output::cell(v, "signature"),
        )
    })
}
