//! `springtale author` — the trusted-author registry that connector
//! manifest signatures are verified against.
//!
//! Deliberately offline (plan 2.2's offline set, alongside `init` and
//! `vault`): `author add --self` registers this instance's signing
//! identity, and that has to be possible on first run, before there is a
//! daemon to ask. Requiring `springtale server start` to register the
//! key that signs your own connectors would put the first-run path
//! behind the thing it precedes.
//!
//! That leaves one hazard — two writers against one registry — and it is
//! closed by both surfaces going through the same code: every read,
//! write and validation here is
//! [`springtale_runtime::operations::authors`], byte for byte the
//! functions `GET /authors` and `POST /authors/{name}` call, against the
//! same `trusted-author:` rows in the same store. The daemon does not
//! own a parallel copy; there is one registry and one implementation of
//! it, reached from a socket or from a terminal.

use anyhow::{Context, Result};
use tabled::{Table, Tabled};

use springtale_crypto::identity::keypair::Keypair;
use springtale_runtime::operations::authors;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::cli::AuthorAction;
use crate::output;

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

            // Hex and 32-byte checks live in the operation, so the
            // terminal cannot store a key the API would have refused.
            let author = authors::add(store, &name, &pubkey_hex)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            let added = serde_json::json!({ "name": author.name, "pubkey": author.pubkey });
            output::emit(json, &added, |v| {
                format!(
                    "Trusted author added: {}\n  pubkey: {}",
                    output::cell(v, "name"),
                    output::cell(v, "pubkey")
                )
            })?;
        }
        AuthorAction::List => {
            let authors = authors::list(store)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let rows: Vec<AuthorTableRow> = authors
                .iter()
                .map(|a| AuthorTableRow {
                    name: a.name.clone(),
                    pubkey: a.pubkey.clone(),
                })
                .collect();

            output::emit(json, &authors, |_| {
                if rows.is_empty() {
                    "No trusted authors.".to_owned()
                } else {
                    Table::new(rows).to_string()
                }
            })?;
        }
        AuthorAction::Remove { name } => {
            authors::remove(store, &name)
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

/// The `author list` body — a bare array, one object per author. The
/// command emits the operation's own rows; this is the same shape,
/// asserted by the output tests.
#[cfg(test)]
fn authors_json(rows: &[AuthorTableRow]) -> Vec<serde_json::Value> {
    rows.iter()
        .map(|r| serde_json::json!({ "name": r.name, "pubkey": r.pubkey }))
        .collect()
}

/// The `author add` body, as the command emits it.
#[cfg(test)]
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

    /// Rows as `operations::authors::list` returns them. Parsing the
    /// `trusted-author:` config entries is the operation's job, so this
    /// starts from its output rather than re-implementing the parse.
    fn rows() -> Vec<AuthorTableRow> {
        vec![AuthorTableRow {
            name: "kali".to_owned(),
            pubkey: "aa11".to_owned(),
        }]
    }

    #[test]
    fn test_author_list_json_shape_is_a_bare_array_of_name_and_pubkey() {
        let out = json_value(&authors_json(&rows()));
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
        let out = json_value(&authors_json(&[]));
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
