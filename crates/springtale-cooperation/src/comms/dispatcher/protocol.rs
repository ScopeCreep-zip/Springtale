//! Protocol-message dispatcher. Fans out [`ProtocolMsg`] from the MPSC
//! router to per-member inboxes based on [`MessageTarget`].
//!
//! Runs in a `tokio::spawn` task owned by the formation. Exits when the
//! router sender is dropped (formation dissolve).
//!
//! [`ProtocolMsg`]: crate::comms::bus::ProtocolMsg
//! [`MessageTarget`]: crate::comms::MessageTarget

use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;

use super::super::bus::{ProtocolDispatch, ProtocolMsg};
use super::super::MessageTarget;

// Compile-time witness that the dispatcher always operates on
// `ProtocolMsg`. The loop below binds `msg` via type inference from
// `ProtocolDispatch::rx`, so without this line the import would be
// flagged "unused" — but the type IS the module's entire subject.
// Keeping the name referenced here makes that contract visible to
// readers and satisfies the dead-import lint.
const _: fn(ProtocolMsg) = |_| ();

/// Spawn-friendly dispatcher loop.
///
/// `resolve_nearest` maps a `CapabilityDecl` to the AgentId that should
/// receive a `MessageTarget::NearestCapable` message. Callers inject the
/// formation's capability index here. `None` means the formation has no
/// agent with the requested capability; the message is dropped.
pub async fn run<F>(mut dispatch: ProtocolDispatch, resolve_nearest: F)
where
    F: Fn(&CapabilityDecl) -> Option<AgentId> + Send + 'static,
{
    while let Some(msg) = dispatch.rx.recv().await {
        // Clone the target enum up front so we can `continue` on no-op
        // cases, and still `move` the original message into the winning
        // inbox below without aliasing `msg.target`.
        let target = msg.target.clone();
        match target {
            MessageTarget::Formation => {
                // Fan out to every member except the sender.
                let source = msg.source;
                for entry in dispatch.inboxes.iter() {
                    let agent_id = *entry.key();
                    if agent_id == source {
                        continue;
                    }
                    if let Err(e) = entry.value().try_send(msg.clone()) {
                        tracing::warn!(
                            ?agent_id,
                            error = %e,
                            "protocol inbox full or closed; dropping"
                        );
                    }
                }
            }
            MessageTarget::Specific(target_id) => {
                if let Some(inbox) = dispatch.inboxes.get(&target_id) {
                    if let Err(e) = inbox.value().try_send(msg) {
                        tracing::warn!(
                            ?target_id,
                            error = %e,
                            "protocol inbox full or closed; dropping"
                        );
                    }
                } else {
                    tracing::debug!(?target_id, "protocol target not subscribed; dropping");
                }
            }
            MessageTarget::NearestCapable(cap) => {
                let Some(agent) = resolve_nearest(&cap) else {
                    tracing::debug!(
                        capability = %cap,
                        "no capable agent for nearest-capable target; dropping"
                    );
                    continue;
                };
                if let Some(inbox) = dispatch.inboxes.get(&agent)
                    && let Err(e) = inbox.value().try_send(msg)
                {
                    tracing::warn!(
                        ?agent,
                        error = %e,
                        "protocol inbox full or closed; dropping"
                    );
                }
            }
        }
    }
    tracing::debug!("protocol dispatcher exiting — router closed");
}
