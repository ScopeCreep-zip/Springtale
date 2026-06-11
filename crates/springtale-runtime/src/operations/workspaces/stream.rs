//! Short-lived onboarding stream — the runtime half of the
//! one-click Onboard UX (Track D).
//!
//! Flow:
//!
//! 1. User clicks 🎯 Onboard in the deploy form.
//! 2. Frontend kicks off a 60s stream against the connector's
//!    `discover_destinations` action using the form's just-typed
//!    credentials.
//! 3. User taps the deep link, taps START in Telegram. The bot
//!    receives `/start <payload>`.
//! 4. The next poll iteration picks up that message, the runtime
//!    fires the per-discovery callback, the frontend auto-selects
//!    the chat in the picker dropdown.
//!
//! The runtime stays Tauri-agnostic — the Tauri command supplies a
//! callback that translates discoveries into `tauri_specta::Event`
//! emissions. Cancellation is a `tokio::sync::watch` channel; the
//! Tauri command holds the sender, the runtime holds the receiver.
//!
//! Sources:
//! - [Telegram getUpdates](https://core.telegram.org/bots/api#getupdates) — offset / timeout / allowed_updates contract.
//! - [Telegram deep linking](https://core.telegram.org/bots/features#deep-linking) — `/start <payload>` echo format.
//! - [Telegraf polling loop](https://github.com/telegraf/telegraf/blob/develop/src/core/network/polling.ts) — offset = last.update_id + 1 idiom.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::watch;

use springtale_connector::factory::FactoryEntry;
use springtale_cooperation::mental_model::WorkspaceProvenance;

use crate::error::OperationError;
use crate::operations::workspaces::query::WorkspaceInfo;

/// Callback invoked once per discovered destination.
///
/// `matched=true` means the chat passed the `/start <payload>` filter
/// — i.e. it is the user's own onboarding tap. Frontend uses this to
/// auto-select the chat in the picker dropdown.
pub type OnDiscoveryCallback = Arc<dyn Fn(WorkspaceInfo, bool) + Send + Sync + 'static>;

/// Maximum onboarding window. After this much wall-clock time the
/// stream terminates on its own even if no chat was discovered.
const STREAM_DURATION: Duration = Duration::from_secs(60);

/// Cadence between `discover_destinations` polls. 2 seconds gives the
/// user time to tap START in Telegram without burning rate-limit
/// budget on `getUpdates`.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Start a short-lived onboarding stream.
///
/// Spawns a tokio task that:
///   1. Instantiates a one-shot connector via the factory (no
///      registration — same pattern as `preview_onboard_url`).
///   2. Loops `connector.execute("discover_destinations", { since_update_id, payload_filter })`
///      every `POLL_INTERVAL` for up to `STREAM_DURATION`.
///   3. Calls `on_discover(workspace, matched=true)` for each new
///      destination. The action itself filters by payload, so any row
///      it returns is by definition a match.
///   4. Terminates early on the first match — the user only needs one
///      chat onboarded per Onboard click.
///   5. Honours the cancel signal at every poll boundary and during
///      every sleep, so dropping the picker mid-stream stops the task
///      within ≤ `POLL_INTERVAL`.
///
/// Returns the `watch::Sender<bool>` — store it on the Tauri side and
/// `send(true)` to cancel.
pub fn start_onboard_stream(
    connector_name: String,
    config: serde_json::Value,
    payload: String,
    on_discover: OnDiscoveryCallback,
) -> Result<watch::Sender<bool>, OperationError> {
    // Resolve the factory eagerly so a bad connector name surfaces
    // synchronously to the caller — the spawned task only sees a
    // factory it can definitely call.
    let factory = inventory::iter::<FactoryEntry>
        .into_iter()
        .find(|e| e.factory.name() == connector_name)
        .ok_or_else(|| {
            OperationError::Connector(format!("no factory registered for {connector_name}"))
        })?
        .factory;

    let (cancel_tx, mut cancel_rx) = watch::channel(false);

    tokio::spawn(async move {
        let connector = match factory.create(config).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    connector = %connector_name,
                    "onboard stream: factory.create failed",
                );
                return;
            }
        };

        let start = std::time::Instant::now();
        let mut since_update_id: Option<i64> = None;

        loop {
            // Cancellation + duration gates checked at every iteration
            // boundary so the task never overruns by more than one
            // poll round.
            if *cancel_rx.borrow() {
                tracing::debug!("onboard stream: cancel signal received");
                break;
            }
            if start.elapsed() >= STREAM_DURATION {
                tracing::debug!("onboard stream: duration limit reached");
                break;
            }

            let mut input = serde_json::json!({ "payload_filter": payload });
            if let Some(id) = since_update_id {
                input["since_update_id"] = serde_json::Value::from(id);
            }

            match connector.execute("discover_destinations", input).await {
                Ok(result) => {
                    if let Some(next) = result.output.get("next_update_id").and_then(|v| v.as_i64())
                    {
                        since_update_id = Some(next);
                    }
                    let mut found_match = false;
                    if let Some(rows) = result.output.get("workspaces").and_then(|v| v.as_array()) {
                        for row in rows {
                            if let Some(info) = row_to_workspace_info(row, &connector_name) {
                                (on_discover)(info, true);
                                found_match = true;
                            }
                        }
                    }
                    if found_match {
                        tracing::info!(
                            connector = %connector_name,
                            "onboard stream: match found, terminating",
                        );
                        break;
                    }
                }
                Err(e) => {
                    // Most likely: connector doesn't implement
                    // `discover_destinations`, or Telegram returned
                    // 409 Conflict (another poller has the lock).
                    // Either is terminal — there is no recovery path
                    // for the current stream.
                    tracing::warn!(
                        error = %e,
                        connector = %connector_name,
                        "onboard stream: discover_destinations failed; stopping",
                    );
                    break;
                }
            }

            // Sleep that honours the cancel signal. tokio::select! lets
            // us bail out within ms of receiving the cancel rather than
            // waiting the full POLL_INTERVAL.
            tokio::select! {
                _ = tokio::time::sleep(POLL_INTERVAL) => {}
                _ = cancel_rx.changed() => {
                    break;
                }
            }
        }
    });

    Ok(cancel_tx)
}

/// Convert one `workspaces[]` action-result row into a
/// `WorkspaceInfo` the frontend can render in the picker dropdown.
/// Provenance is `ActiveDiscovery` — the row was just observed live.
fn row_to_workspace_info(row: &serde_json::Value, connector_name: &str) -> Option<WorkspaceInfo> {
    let workspace_key = row
        .get("workspace_key")
        .and_then(|v| v.as_str())?
        .to_owned();
    let display_name = row
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&workspace_key)
        .to_owned();
    let kind = row
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();
    let metadata = row
        .get("metadata")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let metadata_json =
        if metadata.is_null() || metadata.as_object().map(|o| o.is_empty()).unwrap_or(false) {
            None
        } else {
            serde_json::to_string(&metadata).ok()
        };
    let now_ms = Utc::now().timestamp_millis();
    let provenance = WorkspaceProvenance::ActiveDiscovery {
        scanned_at: Utc::now(),
    };
    let provenance_json = serde_json::to_string(&provenance).ok()?;
    Some(WorkspaceInfo {
        workspace_key,
        connector_name: connector_name.to_owned(),
        display_name,
        kind,
        metadata_json,
        first_seen_at_unix_ms: now_ms,
        last_seen_at_unix_ms: now_ms,
        provenance_json,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn row_to_workspace_info_round_trips_basic_fields() {
        let row = serde_json::json!({
            "workspace_key": "telegram://chat/42",
            "display_name": "Alice",
            "kind": "user",
            "metadata": { "telegram_type": "private" }
        });
        let info = row_to_workspace_info(&row, "connector-telegram").unwrap();
        assert_eq!(info.workspace_key, "telegram://chat/42");
        assert_eq!(info.display_name, "Alice");
        assert_eq!(info.kind, "user");
        assert_eq!(info.connector_name, "connector-telegram");
        assert!(info.metadata_json.is_some());
        assert!(info.provenance_json.contains("active_discovery"));
    }

    #[test]
    fn row_to_workspace_info_handles_missing_optional_fields() {
        let row = serde_json::json!({
            "workspace_key": "telegram://chat/7",
        });
        let info = row_to_workspace_info(&row, "connector-telegram").unwrap();
        assert_eq!(info.display_name, "telegram://chat/7");
        assert_eq!(info.kind, "unknown");
        assert!(info.metadata_json.is_none());
    }

    #[test]
    fn row_to_workspace_info_drops_rows_without_workspace_key() {
        let row = serde_json::json!({ "display_name": "Bob" });
        assert!(row_to_workspace_info(&row, "connector-telegram").is_none());
    }

    #[tokio::test]
    async fn start_onboard_stream_rejects_unknown_connector() {
        let cb: OnDiscoveryCallback = Arc::new(|_, _| {});
        let result = start_onboard_stream(
            "connector-nope".to_owned(),
            serde_json::json!({}),
            "test".to_owned(),
            cb,
        );
        assert!(matches!(result, Err(OperationError::Connector(_))));
    }
}
