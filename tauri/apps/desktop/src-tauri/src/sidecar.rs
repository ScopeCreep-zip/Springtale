//! `springtaled` sidecar supervision.
//!
//! The desktop shell owns no runtime of its own. It spawns one daemon as
//! a Tauri `externalBin` sidecar, hands it the vault passphrase over a
//! pipe, and waits for the daemon to report the loopback port it bound.
//! Every subsequent read and write goes over that HTTP API — the same API
//! the web dashboard uses, so there is exactly one state owner.
//!
//! Mobile: iOS forbids subprocesses and Tauri's Android sidecar support is
//! still open (tauri-apps/tauri#9774), so on those targets the daemon is
//! meant to run in-process via `springtaled::runtime::boot` with the same
//! `--bind 127.0.0.1:0` semantics. The frontend cannot tell the difference;
//! it is the same web provider hitting the same loopback API.

use secrecy::{ExposeSecret, SecretString};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};

/// A running `springtaled` child process and the port it bound.
pub struct Daemon {
    /// Loopback port the management API is listening on.
    pub port: u16,
    /// Child handle — kept so locking the vault can terminate the daemon.
    pub child: CommandChild,
}

/// Spawn `springtaled`, feed it the passphrase, and wait for `READY {port}`.
///
/// The passphrase travels on stdin only. `argv` is world-readable through
/// `ps` on every platform we ship, and the environment is readable by any
/// process running as the same user on Linux — neither is acceptable for a
/// survivor's vault passphrase.
pub async fn start(app: &tauri::AppHandle, passphrase: &SecretString) -> Result<Daemon, String> {
    let (mut rx, mut child) = app
        .shell()
        .sidecar("springtaled")
        .map_err(|e| format!("springtaled sidecar not found: {e}"))?
        .args(["--bind", "127.0.0.1:0", "--passphrase-stdin"])
        .spawn()
        .map_err(|e| format!("failed to spawn springtaled: {e}"))?;

    // SECURITY: expose needed to hand the passphrase to the daemon over
    // stdin, never argv or env.
    let line = format!("{}\n", passphrase.expose_secret());
    child
        .write(line.as_bytes())
        .map_err(|e| format!("failed to send passphrase to springtaled: {e}"))?;

    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(line) => {
                if let Some(port) = parse_ready(&line) {
                    tracing::info!(port, "springtaled sidecar ready");
                    return Ok(Daemon { port, child });
                }
            }
            CommandEvent::Stderr(line) => {
                // The daemon logs to stderr; surface it so a wrong
                // passphrase or a corrupt vault is debuggable.
                if let Ok(text) = std::str::from_utf8(&line) {
                    tracing::debug!(target: "springtaled", "{}", text.trim_end());
                }
            }
            CommandEvent::Terminated(status) => {
                return Err(format!(
                    "springtaled exited before READY (code {:?}) — wrong passphrase or corrupt vault",
                    status.code
                ));
            }
            CommandEvent::Error(e) => return Err(format!("springtaled sidecar error: {e}")),
            _ => {}
        }
    }

    Err("springtaled stream closed before READY".to_owned())
}

/// Parse a `READY {port}` line. Returns `None` for any other output.
fn parse_ready(line: &[u8]) -> Option<u16> {
    std::str::from_utf8(line)
        .ok()?
        .trim()
        .strip_prefix("READY ")?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::parse_ready;

    #[test]
    fn test_parse_ready_with_port_returns_port() {
        assert_eq!(parse_ready(b"READY 51234\n"), Some(51234));
    }

    #[test]
    fn test_parse_ready_bare_ready_returns_none() {
        assert_eq!(parse_ready(b"READY\n"), None);
    }

    #[test]
    fn test_parse_ready_unrelated_line_returns_none() {
        assert_eq!(parse_ready(b"INFO springtaled starting"), None);
    }
}

/// Log in to the freshly started daemon and return the bearer token it
/// issues (plan 6.6, finding 109).
///
/// The shell used to compute `HMAC(passphrase)` and use that as the
/// bearer: deterministic, unrotatable, and a passphrase equivalent. Now
/// the passphrase is presented exactly once, to `POST /auth/login`, and
/// the daemon mints a random 32-byte session token for it. The
/// passphrase never becomes a credential and the token can be dropped
/// (`POST /auth/logout`) without touching the vault.
pub async fn login(port: u16, passphrase: &secrecy::SecretString) -> Result<String, String> {
    use secrecy::ExposeSecret as _;

    let http = springtale_transport::safe_http::client()
        .map_err(|e| format!("could not build an HTTP client: {e}"))?;
    let response = http
        .post(format!("http://127.0.0.1:{port}/auth/login"))
        // SECURITY: expose needed for the one request that carries the
        // passphrase — the login itself. It is not stored anywhere.
        .json(&serde_json::json!({ "passphrase": passphrase.expose_secret() }))
        .send()
        .await
        .map_err(|e| format!("could not reach the daemon to log in: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("daemon rejected the login: {}", response.status()));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("unreadable login response: {e}"))?;
    body.get("token")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| "login response carried no token".to_owned())
}
