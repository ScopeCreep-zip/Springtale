use tokio::sync::watch;

use crate::context::FormationContext;

use super::trait_::ContextPublisher;

/// `watch`-backed publisher. One writer, many readers — matches
/// COOPERATION.md §6's FormationContext dissemination primitive.
pub struct BusContextPublisher {
    tx: watch::Sender<FormationContext>,
}

impl BusContextPublisher {
    pub fn new(initial: FormationContext) -> (Self, watch::Receiver<FormationContext>) {
        let (tx, rx) = watch::channel(initial);
        (Self { tx }, rx)
    }
}

impl ContextPublisher for BusContextPublisher {
    fn publish(&self, ctx: FormationContext) {
        // `send` only errors when all receivers are dropped — that's a
        // teardown signal, not a failure we need to surface.
        let _ = self.tx.send(ctx);
    }
}
