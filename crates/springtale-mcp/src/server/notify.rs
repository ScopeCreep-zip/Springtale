//! `notifications/tools/list_changed` — keeping a client's cached tool
//! list honest.
//!
//! [`SpringtaleMcp::get_info`](super::registry::SpringtaleMcp) advertises
//! the `tools.listChanged` capability. The MCP spec's promise for that
//! capability is that the server "SHOULD send a notification when the
//! tool list changes", and a client is entitled to call `tools/list`
//! once at initialization and cache the result. Until this module
//! existed the daemon advertised the capability and never sent the
//! notification, so installing or removing a connector left every
//! connected client calling tools that no longer exist and blind to
//! ones that now do.
//!
//! Shape: the runtime publishes protocol-free
//! [`ToolCatalogEvent`]s on a broadcast channel (it cannot depend on
//! `rmcp` — `springtale-mcp` depends on `springtale-runtime`, not the
//! other way round). Each connected client gets one forwarder task,
//! started from `on_initialized`, holding that client's
//! [`Peer`](rmcp::service::Peer) and one subscription. The task
//! translates each in-scope event into one notification frame on that
//! client's stream and exits — pruning itself — as soon as the peer's
//! transport is gone or the runtime drops the channel.

use std::future::Future;

use rmcp::RoleServer;
use rmcp::service::Peer;
use springtale_runtime::tool_catalog::ToolCatalogEvent;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

/// The one thing a forwarder needs from a connected client.
///
/// A trait rather than a bare `Peer<RoleServer>` so the forward loop —
/// including its scope filter and its prune-on-dead-peer exit — is
/// testable without standing up a transport.
pub trait ToolListSink: Send + Sync + 'static {
    /// Send one `notifications/tools/list_changed`. Returns `false` if
    /// the frame did not reach the client.
    fn send_tool_list_changed(&self) -> impl Future<Output = bool> + Send;

    /// Whether the client's transport is gone for good. Distinguishes a
    /// disconnected client (stop forwarding) from a send that merely
    /// failed this once (keep forwarding).
    fn is_closed(&self) -> bool;
}

impl ToolListSink for Peer<RoleServer> {
    async fn send_tool_list_changed(&self) -> bool {
        match Peer::notify_tool_list_changed(self).await {
            Ok(()) => true,
            // A disconnected client is the ordinary end of a session,
            // not an error worth surfacing: log at debug and let the
            // caller drop this forwarder.
            Err(e) => {
                tracing::debug!(error = %e, "tools/list_changed frame not delivered");
                false
            }
        }
    }

    fn is_closed(&self) -> bool {
        Peer::is_transport_closed(self)
    }
}

/// Whether a catalog event concerns a server with this scope.
///
/// A server scoped to one connector (`SpringtaleMcp::for_connector`)
/// only lists that connector's actions, so a change to a different
/// connector cannot have changed its list.
pub fn in_scope(scope: Option<&str>, event: &ToolCatalogEvent) -> bool {
    scope.is_none_or(|s| s == event.connector)
}

/// What happened to one delivery attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delivery {
    /// The client got the frame.
    Sent,
    /// The send failed but the session is still live — a client that
    /// has not opened its SSE stream yet is the ordinary case. Keeping
    /// the forwarder alive means one early miss does not silently
    /// disable list-changed notifications for the rest of the session.
    Missed,
    /// The transport is gone. Stop forwarding and let the task end,
    /// which is how dead peers get pruned.
    Disconnected,
}

/// Send one frame and classify the outcome.
async fn deliver<S: ToolListSink>(sink: &S) -> Delivery {
    if sink.is_closed() {
        return Delivery::Disconnected;
    }
    if sink.send_tool_list_changed().await {
        return Delivery::Sent;
    }
    if sink.is_closed() {
        Delivery::Disconnected
    } else {
        Delivery::Missed
    }
}

/// Forward catalog changes to one client until it or the runtime goes
/// away.
///
/// Returns the number of notifications actually delivered, which is what
/// the tests assert on.
pub async fn forward_tool_list_changes<S: ToolListSink>(
    mut events: Receiver<ToolCatalogEvent>,
    sink: S,
    scope: Option<String>,
) -> usize {
    let mut sent = 0usize;
    loop {
        let outcome = match events.recv().await {
            Ok(event) => {
                if !in_scope(scope.as_deref(), &event) {
                    continue;
                }
                deliver(&sink).await
            }
            // Overflow means we missed events but still know the list
            // moved. The notification carries no payload, so one frame
            // covers every dropped event; a scoped server notifies too
            // rather than guess whether the lost events were in scope.
            Err(RecvError::Lagged(skipped)) => {
                tracing::debug!(skipped, "tool catalog subscriber lagged; notifying anyway");
                deliver(&sink).await
            }
            // The runtime dropped the channel — the process is shutting
            // down. Nothing left to forward.
            Err(RecvError::Closed) => break,
        };
        match outcome {
            Delivery::Sent => sent += 1,
            Delivery::Missed => {}
            Delivery::Disconnected => break,
        }
    }
    sent
}

/// Start a forwarder for one connected client.
///
/// Detached on purpose: it owns only a `Peer` clone and a broadcast
/// receiver, and it ends on its own when either side disappears, so
/// there is no handle worth keeping.
pub fn spawn_tool_list_forwarder<S: ToolListSink>(
    events: Receiver<ToolCatalogEvent>,
    sink: S,
    scope: Option<String>,
) {
    tokio::spawn(async move {
        let sent = forward_tool_list_changes(events, sink, scope).await;
        tracing::debug!(sent, "MCP tools/list_changed forwarder finished");
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use springtale_runtime::tool_catalog::{ToolCatalogChange, ToolCatalogNotifier};

    use super::*;

    /// Counts frames. `closed` models a client that went away;
    /// `fail_once` models a single failed send on a still-live session
    /// (a client that has not opened its SSE stream yet), clearing
    /// itself so the next send succeeds.
    #[derive(Clone, Default)]
    struct CountingSink {
        sent: Arc<AtomicUsize>,
        closed: Arc<AtomicBool>,
        fail_once: Arc<AtomicBool>,
    }

    impl ToolListSink for CountingSink {
        async fn send_tool_list_changed(&self) -> bool {
            if self.closed.load(Ordering::SeqCst) {
                return false;
            }
            if self.fail_once.swap(false, Ordering::SeqCst) {
                return false;
            }
            self.sent.fetch_add(1, Ordering::SeqCst);
            true
        }

        fn is_closed(&self) -> bool {
            self.closed.load(Ordering::SeqCst)
        }
    }

    fn event(connector: &str) -> ToolCatalogEvent {
        ToolCatalogEvent {
            connector: connector.to_owned(),
            change: ToolCatalogChange::Installed,
        }
    }

    #[tokio::test]
    async fn test_forward_unscoped_notifies_every_change() {
        let notifier = ToolCatalogNotifier::new();
        let rx = notifier.subscribe();
        let sink = CountingSink::default();

        notifier.notify("github", ToolCatalogChange::Installed);
        notifier.notify("telegram", ToolCatalogChange::Removed);
        notifier.notify("github", ToolCatalogChange::Disabled);
        drop(notifier);

        let sent = forward_tool_list_changes(rx, sink.clone(), None).await;
        assert_eq!(sent, 3);
        assert_eq!(sink.sent.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_forward_scoped_ignores_other_connectors() {
        let notifier = ToolCatalogNotifier::new();
        let rx = notifier.subscribe();
        let sink = CountingSink::default();

        notifier.notify("github", ToolCatalogChange::Installed);
        notifier.notify("telegram", ToolCatalogChange::Removed);
        drop(notifier);

        let sent = forward_tool_list_changes(rx, sink, Some("github".to_owned())).await;
        assert_eq!(sent, 1);
    }

    #[tokio::test]
    async fn test_forward_stops_when_client_disconnects() {
        let notifier = ToolCatalogNotifier::new();
        let rx = notifier.subscribe();
        let sink = CountingSink::default();
        sink.closed.store(true, Ordering::SeqCst);

        notifier.notify("github", ToolCatalogChange::Installed);
        notifier.notify("github", ToolCatalogChange::Removed);

        // The loop must exit on the first dead-peer send rather than
        // spinning on a channel the runtime still holds open.
        let sent = forward_tool_list_changes(rx, sink.clone(), None).await;
        assert_eq!(sent, 0);
        assert_eq!(sink.sent.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_forward_survives_a_transient_send_failure() {
        let notifier = ToolCatalogNotifier::new();
        let rx = notifier.subscribe();
        let sink = CountingSink::default();
        sink.fail_once.store(true, Ordering::SeqCst);

        notifier.notify("github", ToolCatalogChange::Installed);
        notifier.notify("github", ToolCatalogChange::Removed);
        drop(notifier);

        // The first frame is lost, but the forwarder must still be
        // alive to deliver the second.
        let sent = forward_tool_list_changes(rx, sink.clone(), None).await;
        assert_eq!(sent, 1);
        assert_eq!(sink.sent.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_forward_exits_when_runtime_drops_the_channel() {
        let notifier = ToolCatalogNotifier::new();
        let rx = notifier.subscribe();
        drop(notifier);

        assert_eq!(
            forward_tool_list_changes(rx, CountingSink::default(), None).await,
            0
        );
    }

    #[tokio::test]
    async fn test_notify_does_not_fail_when_forwarder_is_gone() {
        let notifier = ToolCatalogNotifier::new();
        let rx = notifier.subscribe();
        drop(rx);
        // Dropping the only subscriber must leave the publisher usable —
        // a connector install cannot fail because a client hung up.
        notifier.notify("github", ToolCatalogChange::Installed);
        assert_eq!(notifier.subscriber_count(), 0);
    }

    #[test]
    fn test_in_scope_filters_by_connector() {
        assert!(in_scope(None, &event("github")));
        assert!(in_scope(Some("github"), &event("github")));
        assert!(!in_scope(Some("github"), &event("telegram")));
    }
}
