//! Intent-ack consumer. Drains the fan-in mpsc from every member into the
//! formation's cadence evaluator.
//!
//! Runs in a `tokio::spawn` task owned by the formation. Exits when all
//! senders are dropped (formation dissolve).

use super::super::bus::{AckDispatch, IntentAckMsg};

/// Spawn-friendly ack consumer loop. `handler` is invoked for each ack.
pub async fn run<H>(mut dispatch: AckDispatch, mut handler: H)
where
    H: FnMut(IntentAckMsg) + Send + 'static,
{
    while let Some(ack) = dispatch.rx.recv().await {
        handler(ack);
    }
    tracing::debug!("ack consumer exiting — all senders dropped");
}
