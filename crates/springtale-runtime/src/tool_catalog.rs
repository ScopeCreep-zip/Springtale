//! Tool-catalog change fan-out — "the tool list you cached is stale".
//!
//! The daemon's MCP server advertises the `tools.listChanged` capability,
//! and the MCP spec is explicit about what that promises: a server that
//! declares it "SHOULD send a notification when the tool list changes"
//! (`notifications/tools/list_changed`). Without a signal a client that
//! called `tools/list` once at initialization keeps calling connectors
//! that were removed, and never sees connectors installed since.
//!
//! The connector registry lives in [`RuntimeState`](crate::state::RuntimeState)
//! and the MCP crate sits *above* the runtime in the dependency order, so
//! the runtime cannot call into `rmcp` to send the notification itself.
//! Instead it publishes a protocol-free [`ToolCatalogEvent`] here and
//! `springtale-mcp` subscribes per connected client, translating each
//! event into one `notifications/tools/list_changed` frame on that
//! client's stream.
//!
//! Mirror of the `canvas_tx` / `notification_tx` broadcast pattern
//! already on `RuntimeState`. Publishing never fails and never blocks:
//! with no MCP client attached there are no receivers, and
//! [`ToolCatalogNotifier::notify`] drops the event.

use tokio::sync::broadcast;

/// How many events the channel buffers per subscriber before a slow
/// client starts losing them. Connector installs are human-paced, so
/// this is generous; a lagged subscriber is handled by notifying
/// unconditionally rather than by replaying, so overflow costs a
/// redundant `tools/list` at worst.
const CHANNEL_CAPACITY: usize = 64;

/// What happened to a connector, for logs and for scope filtering.
///
/// The MCP notification itself carries no payload — it only says
/// "re-read the list" — so this exists to let a scoped server ignore
/// changes to connectors it does not serve, and to make the event
/// legible in tracing output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCatalogChange {
    /// A connector was loaded into the live registry (configure & load,
    /// or a WASM install).
    Installed,
    /// A connector was dropped from the live registry.
    Removed,
    /// A disabled connector became callable again.
    Enabled,
    /// A connector stopped being callable. Disabled connectors are
    /// omitted from `tools/list`, so this changes the list.
    Disabled,
    /// A connector's host was rebuilt in place — its action set may
    /// differ from the one the client cached.
    Reloaded,
}

impl ToolCatalogChange {
    /// Lower-case label used in tracing fields.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Removed => "removed",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Reloaded => "reloaded",
        }
    }
}

/// One change to the set of connector actions the runtime can dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCatalogEvent {
    /// The connector whose entry changed.
    pub connector: String,
    /// What happened to it.
    pub change: ToolCatalogChange,
}

/// Publish/subscribe handle for [`ToolCatalogEvent`]s.
///
/// Cheap to clone (a `broadcast::Sender` is an `Arc` inside), which is
/// what lets it sit on the cloneable `RuntimeState`.
#[derive(Clone, Debug)]
pub struct ToolCatalogNotifier {
    tx: broadcast::Sender<ToolCatalogEvent>,
}

impl ToolCatalogNotifier {
    /// A notifier with no subscribers yet.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self { tx }
    }

    /// Subscribe to future changes. Events published before this call
    /// are not replayed — a client that subscribes at initialization has
    /// just fetched the current list anyway.
    pub fn subscribe(&self) -> broadcast::Receiver<ToolCatalogEvent> {
        self.tx.subscribe()
    }

    /// Publish a change.
    ///
    /// Infallible by construction: `broadcast::Sender::send` errors only
    /// when there are no receivers, which is the ordinary case (no MCP
    /// client attached). Connector installs must not fail because
    /// nobody was listening, so the error is dropped.
    pub fn notify(&self, connector: impl Into<String>, change: ToolCatalogChange) {
        let event = ToolCatalogEvent {
            connector: connector.into(),
            change,
        };
        tracing::debug!(
            connector = %event.connector,
            change = change.as_str(),
            subscribers = self.tx.receiver_count(),
            "tool catalog changed"
        );
        let _ = self.tx.send(event);
    }

    /// How many live subscribers there are. Diagnostic only.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for ToolCatalogNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_notify_subscriber_receives_event() {
        let notifier = ToolCatalogNotifier::new();
        let mut rx = notifier.subscribe();

        notifier.notify("github", ToolCatalogChange::Installed);

        let event = rx.recv().await.expect("subscriber receives the event");
        assert_eq!(
            event,
            ToolCatalogEvent {
                connector: "github".to_owned(),
                change: ToolCatalogChange::Installed,
            }
        );
    }

    #[tokio::test]
    async fn test_notify_with_no_subscribers_is_not_an_error() {
        let notifier = ToolCatalogNotifier::new();
        assert_eq!(notifier.subscriber_count(), 0);
        // Must not panic: a connector install with no MCP client
        // attached is the common case.
        notifier.notify("telegram", ToolCatalogChange::Removed);
    }

    #[tokio::test]
    async fn test_notify_after_subscriber_dropped_is_not_an_error() {
        let notifier = ToolCatalogNotifier::new();
        let rx = notifier.subscribe();
        drop(rx);
        assert_eq!(notifier.subscriber_count(), 0);
        // A disconnected MCP client must not break the install path.
        notifier.notify("slack", ToolCatalogChange::Disabled);
    }

    #[tokio::test]
    async fn test_multiple_subscribers_each_receive_the_event() {
        let notifier = ToolCatalogNotifier::new();
        let mut a = notifier.subscribe();
        let mut b = notifier.subscribe();
        assert_eq!(notifier.subscriber_count(), 2);

        notifier.notify("kick", ToolCatalogChange::Enabled);

        assert_eq!(
            a.recv().await.expect("first client").change,
            ToolCatalogChange::Enabled
        );
        assert_eq!(
            b.recv().await.expect("second client").change,
            ToolCatalogChange::Enabled
        );
    }

    #[tokio::test]
    async fn test_clone_shares_the_channel() {
        let notifier = ToolCatalogNotifier::new();
        let mut rx = notifier.subscribe();
        let cloned = notifier.clone();

        cloned.notify("nostr", ToolCatalogChange::Reloaded);

        assert_eq!(
            rx.recv()
                .await
                .expect("clone publishes to the same channel"),
            ToolCatalogEvent {
                connector: "nostr".to_owned(),
                change: ToolCatalogChange::Reloaded,
            }
        );
    }
}
