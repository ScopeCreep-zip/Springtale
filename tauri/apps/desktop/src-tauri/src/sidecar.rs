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
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_specta::Event;

use crate::state::AppState;

/// Emitted when the `springtaled` sidecar stops without the shell
/// having asked it to — a crash, an OOM kill, an operator `kill(1)`.
///
/// A deliberate stop (`lock_vault`, auto-lock, quick-hide) does NOT emit
/// this: those take the [`crate::state::DaemonHandle`] out of state
/// before killing the child, and the supervisor treats an already-taken
/// handle as "expected". So receiving this event always means the window
/// is now holding a port and a token that lead nowhere.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct DaemonStopped {
    /// Process exit code, when the platform reported one.
    pub code: Option<i32>,
}

/// A running `springtaled` child process and the port it bound.
pub struct Daemon {
    /// Loopback port the management API is listening on.
    pub port: u16,
    /// Child handle — kept so locking the vault can terminate the daemon.
    pub child: CommandChild,
    /// The sidecar's remaining event stream, handed to [`supervise`].
    ///
    /// Nothing used to read this after `READY`: the receiver was dropped
    /// at the end of [`start`], so a daemon that died a second later did
    /// so unobserved and the shell kept talking to a closed port. It is
    /// carried out of `start` instead, and the caller starts supervision
    /// once the handle is in state (so a stop can never be seen before
    /// the thing it should clear exists).
    pub events: tauri::async_runtime::Receiver<CommandEvent>,
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
                    return Ok(Daemon {
                        port,
                        child,
                        events: rx,
                    });
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

/// Watch a started sidecar for the rest of its life.
///
/// [`start`] only reads the stream up to `READY`. Without this the shell
/// never learns that the daemon died: it keeps a stale `{ port, token }`,
/// the frontend's fetches and SSE reconnects chase a closed port, and the
/// window silently shows a colony that no longer exists.
///
/// On termination the stored [`crate::state::DaemonHandle`] is cleared —
/// so the next unlock spawns a fresh daemon instead of handing back a
/// dead port — and [`DaemonStopped`] is emitted so the UI can say so.
/// This is deliberately not a restart supervisor: `springtaled` holds the
/// unlocked vault, and re-deriving that needs the passphrase, which the
/// shell does not keep. Telling the user is the honest response.
pub fn supervise(app: tauri::AppHandle, mut events: tauri::async_runtime::Receiver<CommandEvent>) {
    tauri::async_runtime::spawn(async move {
        let mut code = None;
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stderr(line) => {
                    if let Ok(text) = std::str::from_utf8(&line) {
                        tracing::debug!(target: "springtaled", "{}", text.trim_end());
                    }
                }
                CommandEvent::Terminated(status) => {
                    code = status.code;
                    break;
                }
                CommandEvent::Error(e) => {
                    tracing::error!(error = %e, "springtaled sidecar stream error");
                    break;
                }
                // Stdout past READY carries nothing the shell acts on.
                _ => {}
            }
        }

        // Whether we saw `Terminated` or the stream simply ended, the
        // child is unreachable from here on.
        // Clone the Arc out first so the `State` borrow is not held
        // across the lock's await point.
        let slot = std::sync::Arc::clone(&app.state::<AppState>().daemon);
        let daemon = slot.lock().await.take();

        let Some(daemon) = daemon else {
            // `lock_vault` (or auto-lock, or quick-hide) already took the
            // handle and killed the child on purpose. Nothing to report.
            tracing::info!("springtaled sidecar stopped as requested");
            return;
        };

        tracing::error!(
            port = daemon.port,
            ?code,
            "springtaled sidecar stopped unexpectedly"
        );
        // Drops the dead child handle and the session token the daemon
        // issued — that token is worthless now, and holding it would only
        // invite the frontend to keep using it.
        drop(daemon);

        let stopped = DaemonStopped { code };
        if let Err(e) = stopped.emit(&app) {
            tracing::error!(error = %e, "failed to tell the window the daemon stopped");
        }
    });
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
