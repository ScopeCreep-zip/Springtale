//! `springtale author` — the trusted-author registry that connector
//! manifest signatures are verified against.
//!
//! Entries are stored as `trusted-author:{name}` → `{"pubkey":"<hex>"}`,
//! byte for byte what `POST /authors/{name}` in springtaled writes, so
//! the CLI and the API share one registry.

use anyhow::{Context, Result};
use tabled::{Table, Tabled};

use springtale_crypto::identity::keypair::Keypair;
use springtale_store::StorageBackend;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::cli::AuthorAction;
use crate::output;

/// Config-store key prefix shared with `springtaled`'s `/authors` API.
const TRUSTED_AUTHOR_PREFIX: &str = "trusted-author:";

/// Row type for the author list table.
#[derive(Tabled)]
struct AuthorTableRow {
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "PUBKEY")]
    pubkey: String,
}

/// Handle author subcommands.
pub async fn run(action: AuthorAction, store: &SqliteBackend, json: bool) -> Result<()> {
    match action {
        AuthorAction::Add {
            name,
            pubkey,
            use_self,
        } => {
            let (name, pubkey_hex) = if use_self {
                let keypair = load_local_identity()?;
                (
                    name.unwrap_or_else(|| "local".to_owned()),
                    hex::encode(keypair.verifying_key().to_bytes()),
                )
            } else {
                let name = name.context("author name is required (or pass --self)")?;
                let pubkey = pubkey.context("pubkey hex is required (or pass --self)")?;
                (name, pubkey)
            };

            // Same validation as the API: hex-encoded 32-byte Ed25519 key.
            let pubkey_bytes = hex::decode(&pubkey_hex).context("pubkey is not valid hex")?;
            if pubkey_bytes.len() != 32 {
                anyhow::bail!("pubkey must be a 32-byte Ed25519 public key");
            }

            let key = format!("{TRUSTED_AUTHOR_PREFIX}{name}");
            let value = serde_json::json!({ "pubkey": pubkey_hex }).to_string();
            store
                .set_config(&key, &value)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            let added = author_body(&name, &pubkey_hex);
            output::emit(json, &added, |v| {
                format!(
                    "Trusted author added: {}\n  pubkey: {}",
                    output::cell(v, "name"),
                    output::cell(v, "pubkey")
                )
            })?;
        }
        AuthorAction::List => {
            let configs = store
                .list_config()
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let rows = author_rows(configs);
            let authors = authors_json(&rows);
            output::emit(json, &authors, |_| {
                if rows.is_empty() {
                    "No trusted authors.".to_owned()
                } else {
                    Table::new(rows).to_string()
                }
            })?;
        }
        AuthorAction::Remove { name } => {
            let key = format!("{TRUSTED_AUTHOR_PREFIX}{name}");
            store
                .delete_config(&key)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let removed = removed_body(&name);
            output::emit(json, &removed, |v| {
                format!("Removed trusted author: {}", output::cell(v, "name"))
            })?;
        }
    }
    Ok(())
}

/// Load this instance's Ed25519 identity from the vault (created by
/// `springtale init`). Prompts for the vault passphrase on the TTY.
pub fn load_local_identity() -> Result<Keypair> {
    let vault_path = springtale_store::paths::default_vault_path();
    if !vault_path.exists() {
        anyhow::bail!(
            "no vault at {} — run `springtale init` first",
            vault_path.display()
        );
    }

    let passphrase = rpassword::read_password_from_tty(Some("Vault passphrase: "))
        .context("failed to read passphrase")?;
    let vault = springtale_crypto::vault::store::Vault::open(&vault_path, passphrase.as_bytes())
        .context("failed to open vault (wrong passphrase?)")?;

    let secret = vault
        .get("identity")
        .context("failed to read identity from vault")?
        .context("vault has no identity — run `springtale init`")?;
    let bytes: [u8; 32] = secret
        .as_slice()
        .try_into()
        .context("identity in vault is not 32 bytes")?;

    Keypair::from_secret_bytes(bytes).context("identity in vault is not a valid Ed25519 key")
}

/// Turn the config rows into author rows, dropping everything that is
/// not a `trusted-author:` entry.
fn author_rows(configs: Vec<(String, String)>) -> Vec<AuthorTableRow> {
    configs
        .into_iter()
        .filter_map(|(key, value)| {
            let name = key.strip_prefix(TRUSTED_AUTHOR_PREFIX)?;
            let data: serde_json::Value = serde_json::from_str(&value).ok()?;
            Some(AuthorTableRow {
                name: name.to_owned(),
                pubkey: data
                    .get("pubkey")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned(),
            })
        })
        .collect()
}

/// The `author list` body — a bare array, one object per author.
fn authors_json(rows: &[AuthorTableRow]) -> Vec<serde_json::Value> {
    rows.iter()
        .map(|r| serde_json::json!({ "name": r.name, "pubkey": r.pubkey }))
        .collect()
}

/// The `author add` body.
fn author_body(name: &str, pubkey_hex: &str) -> serde_json::Value {
    serde_json::json!({ "name": name, "pubkey": pubkey_hex })
}

/// The `author remove` body.
fn removed_body(name: &str) -> serde_json::Value {
    serde_json::json!({ "name": name, "removed": true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    fn configs() -> Vec<(String, String)> {
        vec![
            (
                "trusted-author:kali".to_owned(),
                r#"{"pubkey":"aa11"}"#.to_owned(),
            ),
            // Not an author entry — must not reach the output.
            ("heartbeat".to_owned(), r#"{"interval":30}"#.to_owned()),
        ]
    }

    #[test]
    fn test_author_list_json_shape_is_a_bare_array_of_name_and_pubkey() {
        let out = json_value(&authors_json(&author_rows(configs())));
        assert!(out.is_array(), "authors are not wrapped in an envelope");
        assert_eq!(out.as_array().expect("array").len(), 1);
        let author = &out[0];
        assert_eq!(key_set(author), ["name", "pubkey"]);
        assert!(author["name"].is_string());
        assert!(author["pubkey"].is_string());
        assert_eq!(author["name"], "kali");
        assert_eq!(author["pubkey"], "aa11");
    }

    #[test]
    fn test_author_list_json_is_empty_when_no_author_is_trusted() {
        let out = json_value(&authors_json(&author_rows(Vec::new())));
        assert_eq!(out, serde_json::json!([]));
    }

    #[test]
    fn test_author_add_json_shape_names_the_author_and_its_key() {
        let out = json_value(&author_body("kali", "aa11"));
        assert_eq!(key_set(&out), ["name", "pubkey"]);
        assert!(out["name"].is_string());
        assert!(out["pubkey"].is_string());
    }

    #[test]
    fn test_author_remove_json_shape_names_the_author_and_the_flag() {
        let out = json_value(&removed_body("kali"));
        assert_eq!(key_set(&out), ["name", "removed"]);
        assert_eq!(out["name"], "kali");
        assert!(out["removed"].is_boolean());
        assert_eq!(out["removed"], true);
    }
}
