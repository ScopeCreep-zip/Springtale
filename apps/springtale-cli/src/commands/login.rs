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
pub async fn login() -> Result<()> {
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
        .post(format!("{base}/auth/login"))
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
        .post(format!("{base}/auth/logout"))
        .bearer_auth(&session)
        .send()
        .await;

    println!("Logged in as {name}");
    println!("Token saved to {} (mode 0600)", path.display());
    Ok(())
}

/// `springtale logout` — revoke the saved token, then delete it.
pub async fn logout() -> Result<()> {
    let Some(saved) = client_config::read_token_file()? else {
        println!("Not logged in.");
        return Ok(());
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
    println!(
        "Logged out{}",
        if revoked {
            " (token revoked)"
        } else {
            " (local token deleted)"
        }
    );
    Ok(())
}
