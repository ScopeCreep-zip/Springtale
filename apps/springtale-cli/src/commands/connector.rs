//! `springtale connector` — connector management, over the daemon.
//!
//! `sign` is the one local verb: it signs a manifest file with the
//! local identity from the vault and never touches the daemon.

use anyhow::Result;
use serde_json::{Value, json};

use crate::cli::ConnectorAction;
use crate::client::Client;
use crate::commands::json_input;
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
        ConnectorAction::Available => {
            let body: Value = client.get("/connectors/available").await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "available")
                    .iter()
                    .map(|c| {
                        vec![
                            output::cell(c, "name"),
                            output::cell(c, "label"),
                            output::cell(c, "installed"),
                        ]
                    })
                    .collect();
                output::rows_table(&["NAME", "LABEL", "INSTALLED"], rows)
            })?;
        }
        ConnectorAction::Schemas => {
            let body: Value = client.get("/connectors/schemas").await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        ConnectorAction::Setup { name, config } => {
            let body: Value = client
                .post(
                    "/connectors/setup",
                    &json!({ "name": name, "config": json_input::load(&config)? }),
                )
                .await?;
            output::emit_status(json_out, &body, |v| {
                format!("Set up connector: {}", output::cell(v, "name"))
            })?;
        }
        ConnectorAction::InstallWasm { manifest, wasm } => {
            install_wasm(&client, &manifest, &wasm, json_out).await?;
        }
        ConnectorAction::Cascade { name } => {
            let body: Value = client.delete(&format!("/connectors/{name}/cascade")).await?;
            output::emit_status(json_out, &body, |v| {
                format!(
                    "Removed {name} and {} rule(s).",
                    output::array(v, "rules_deleted").len()
                )
            })?;
        }
        ConnectorAction::Config { name } => {
            let body: Value = client.get(&format!("/connectors/{name}/config")).await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        ConnectorAction::UpsertConfig { name, file } => {
            let body: Value = client
                .post(
                    &format!("/connectors/{name}/upsert-config"),
                    &json_input::load(&file)?,
                )
                .await?;
            output::emit_status(json_out, &body, |v| {
                let verb = if output::cell(v, "is_new") == "true" {
                    "Created"
                } else {
                    "Updated"
                };
                format!("{verb} config for '{name}'.")
            })?;
        }
        ConnectorAction::Outputs { name, limit } => {
            let body: Value = client
                .get(&format!("/connectors/{name}/outputs?limit={limit}"))
                .await?;
            output::emit(json_out, &body, |v| {
                let rows = output::array(v, "outputs")
                    .iter()
                    .map(|o| {
                        vec![
                            output::cell(o, "created_at"),
                            output::cell(o, "action"),
                            output::cell(o, "summary"),
                        ]
                    })
                    .collect();
                output::rows_table(&["WHEN", "ACTION", "SUMMARY"], rows)
            })?;
        }
        ConnectorAction::Reload { name } => {
            let body: Value = client
                .post(&format!("/connectors/{name}/reload"), &json!({}))
                .await?;
            output::emit_status(json_out, &body, |_| format!("Reloaded connector: {name}"))?;
        }
        ConnectorAction::Test { name } => {
            let body: Value = client
                .post(&format!("/connectors/{name}/test"), &json!({}))
                .await?;
            output::emit(json_out, &body, |v| {
                serde_json::to_string_pretty(v).unwrap_or_default()
            })?;
        }
        ConnectorAction::Sign { .. } => unreachable!("handled above"),
    }
    Ok(())
}

/// The multipart boundary the WASM install body is framed with. The
/// route wants a `manifest` part (JSON) and a `wasm` part (binary), and
/// building those two parts by hand keeps the CLI's HTTP client free of
/// reqwest's `multipart` feature.
const WASM_BOUNDARY: &str = "springtale-install-wasm-boundary-9f2c41";

/// POST a manifest + module pair to `/connectors/install-wasm`.
async fn install_wasm(
    client: &Client,
    manifest_path: &std::path::Path,
    wasm_path: &std::path::Path,
    json_out: bool,
) -> Result<()> {
    let manifest_text = std::fs::read_to_string(manifest_path).map_err(|e| {
        anyhow::anyhow!("failed to read manifest at {}: {e}", manifest_path.display())
    })?;
    // The route parses the `manifest` part as JSON; a TOML manifest is
    // converted here so both forms work from the command line.
    let manifest_json = match serde_json::from_str::<Value>(&manifest_text) {
        Ok(value) => value,
        Err(_) => {
            let parsed: springtale_connector::ConnectorManifest = toml::from_str(&manifest_text)
                .map_err(|e| anyhow::anyhow!("manifest is neither JSON nor TOML: {e}"))?;
            serde_json::to_value(parsed)?
        }
    };
    let manifest_json = serde_json::to_string(&manifest_json)?;
    let wasm = std::fs::read(wasm_path)
        .map_err(|e| anyhow::anyhow!("failed to read module at {}: {e}", wasm_path.display()))?;
    if wasm
        .windows(WASM_BOUNDARY.len())
        .any(|w| w == WASM_BOUNDARY.as_bytes())
    {
        anyhow::bail!("module contains the multipart boundary; refusing to send a corrupt body");
    }

    let mut body: Vec<u8> = Vec::with_capacity(wasm.len() + manifest_json.len() + 512);
    body.extend_from_slice(
        format!(
            "--{WASM_BOUNDARY}\r\nContent-Disposition: form-data; name=\"manifest\"\r\nContent-Type: application/json\r\n\r\n{manifest_json}\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{WASM_BOUNDARY}\r\nContent-Disposition: form-data; name=\"wasm\"; filename=\"module.wasm\"\r\nContent-Type: application/wasm\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&wasm);
    body.extend_from_slice(format!("\r\n--{WASM_BOUNDARY}--\r\n").as_bytes());

    let response = client
        .request(reqwest::Method::POST, "/connectors/install-wasm")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={WASM_BOUNDARY}"),
        )
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("{}: {e}", crate::client::UNREACHABLE))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("{status}: {text}");
    }
    let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    output::emit_status(json_out, &parsed, |v| {
        format!("Installed WASM connector: {}", output::cell(v, "installed"))
    })
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
