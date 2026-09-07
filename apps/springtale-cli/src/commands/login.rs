//! `springtale login` / `springtale logout` (plan 6.6, finding 109).
//!
//! Login is the only place a passphrase is used against the API, and it
//! is used to *obtain* a token, never as one. The flow mirrors Home
//! Assistant's: authenticate once, exchange that for a long-lived named
//! token, record the token (here: `$XDG_CONFIG_HOME/springtale/token`,
//! mode 0600), and drop the short-lived session. Logout revokes the
//! long-lived token server-side and deletes the file.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use secrecy::{ExposeSecret, SecretString};

use springtale_runtime::client_config;

use crate::client::UNREACHABLE;
use crate::output;

/// The route a passphrase is exchanged at.
const LOGIN: &str = "/auth/login";
/// The route the long-lived token is revoked at.
const LOGOUT: &str = "/auth/logout";

/// Name the CLI gives the token it creates, so `springtale auth tokens`
/// (and the dashboard) show where it came from.
fn token_name() -> String {
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "local".to_owned());
    format!("springtale-cli@{host}")
}

fn base_url() -> Result<String> {
    client_config::load_base_url(Path::new("springtale.toml")).context("springtale.toml")
}

/// `springtale login` — prompt for the vault passphrase, exchange it for
/// a long-lived token, save it.
pub async fn login(json_out: bool) -> Result<()> {
    let base = base_url()?;
    let http = springtale_transport::safe_http::client().map_err(|e| anyhow!("safe_http: {e}"))?;

    let passphrase = SecretString::new(
        rpassword::read_password_from_tty(Some("Vault passphrase: "))
            .map_err(|e| anyhow!("failed to read passphrase: {e}"))?
            .into(),
    );
    if passphrase.expose_secret().is_empty() {
        bail!("no passphrase given");
    }

    // 1. Log in. The daemon mints a random session token; the passphrase
    //    never becomes a credential.
    let response = http
        .post(format!("{base}{LOGIN}"))
        // SECURITY: expose needed to put the passphrase in the login
        // body — the one request that is allowed to carry it.
        .json(&serde_json::json!({ "passphrase": passphrase.expose_secret() }))
        .send()
        .await
        .map_err(|_| anyhow!(UNREACHABLE))?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!("passphrase rejected");
    }
    if !response.status().is_success() {
        bail!("login failed: HTTP {}", response.status());
    }
    let body: serde_json::Value = response.json().await.context("login response")?;
    let session = body
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("login response had no token"))?
        .to_owned();

    // 2. Exchange it for a long-lived named token — the one that lands
    //    on disk, so a saved credential is revocable on its own.
    let name = token_name();
    let created: serde_json::Value = http
        .post(format!("{base}/auth/tokens"))
        .bearer_auth(&session)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .map_err(|_| anyhow!(UNREACHABLE))?
        .error_for_status()
        .context("could not create a long-lived token")?
        .json()
        .await
        .context("token response")?;
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("token response had no id"))?;
    let token = created
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("token response had no token"))?;

    let path = client_config::write_token_file(id, token)?;

    // 3. Drop the session; the saved token is what the CLI uses now.
    let _ = http
        .post(format!("{base}{LOGOUT}"))
        .bearer_auth(&session)
        .send()
        .await;

    // The token itself is never echoed — only where it landed.
    let body = logged_in_body(&name, id, &path);
    output::emit(json_out, &body, |v| {
        format!(
            "Logged in as {}\nToken saved to {} (mode 0600)",
            output::cell(v, "logged_in_as"),
            output::cell(v, "token_path")
        )
    })
}

/// `springtale logout` — revoke the saved token, then delete it.
pub async fn logout(json_out: bool) -> Result<()> {
    let Some(saved) = client_config::read_token_file()? else {
        let body = not_logged_in_body();
        return output::emit(json_out, &body, |_| "Not logged in.".to_owned());
    };
    let base = base_url()?;
    let http = springtale_transport::safe_http::client().map_err(|e| anyhow!("safe_http: {e}"))?;

    let revoked = match http
        .delete(format!("{base}/auth/tokens/{}", saved.id))
        .bearer_auth(&saved.token)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => true,
        Ok(r) => {
            eprintln!(
                "warning: daemon did not revoke the token (HTTP {})",
                r.status()
            );
            false
        }
        Err(_) => {
            eprintln!("warning: {UNREACHABLE} — deleting the local token anyway");
            false
        }
    };

    client_config::delete_token_file()?;
    let body = logged_out_body(revoked);
    output::emit(json_out, &body, |_| {
        format!(
            "Logged out{}",
            if revoked {
                " (token revoked)"
            } else {
                " (local token deleted)"
            }
        )
    })
}

/// The `login` body. The token itself is never echoed — only who the
/// CLI is now, which token id to revoke, and where the file landed.
fn logged_in_body(name: &str, id: &str, path: &Path) -> serde_json::Value {
    serde_json::json!({
        "logged_in_as": name,
        "token_id": id,
        "token_path": path.display().to_string(),
    })
}

/// The `logout` body when a saved token was found.
fn logged_out_body(revoked: bool) -> serde_json::Value {
    serde_json::json!({ "logged_out": true, "revoked": revoked })
}

/// The `logout` body when there was nothing to log out of.
fn not_logged_in_body() -> serde_json::Value {
    serde_json::json!({ "logged_out": false, "reason": "not logged in" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_login_json_shape_names_identity_token_id_and_path() {
        let out = json_value(&logged_in_body(
            "springtale-cli@laptop",
            "tok-1",
            Path::new("/home/u/.config/springtale/token"),
        ));
        assert_eq!(key_set(&out), ["logged_in_as", "token_id", "token_path"]);
        assert!(out["logged_in_as"].is_string());
        assert!(out["token_id"].is_string());
        assert!(out["token_path"].is_string());
        assert_eq!(out["token_path"], "/home/u/.config/springtale/token");
    }

    #[test]
    fn test_login_json_never_carries_the_token_material() {
        let out = json_value(&logged_in_body("cli@host", "tok-1", Path::new("/t")));
        for leaky in ["token", "passphrase", "secret"] {
            assert!(out.get(leaky).is_none(), "{leaky} must not be emitted");
        }
    }

    #[test]
    fn test_logout_json_shape_names_logged_out_and_revoked() {
        let out = json_value(&logged_out_body(true));
        assert_eq!(key_set(&out), ["logged_out", "revoked"]);
        assert_eq!(out["logged_out"], true);
        assert!(out["revoked"].is_boolean());
        assert_eq!(json_value(&logged_out_body(false))["revoked"], false);
    }

    #[test]
    fn test_logout_when_not_logged_in_json_shape_explains_itself() {
        let out = json_value(&not_logged_in_body());
        assert_eq!(key_set(&out), ["logged_out", "reason"]);
        assert_eq!(out["logged_out"], false);
        assert!(out["reason"].is_string());
        assert_eq!(out["reason"], "not logged in");
    }
}
