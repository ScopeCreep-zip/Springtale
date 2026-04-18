use crate::context::FormationContext;

use super::state_msg::StateMessage;

/// Writes FormationContext to the watch channel. One writer per formation
/// (the Formation struct itself); many readers (every agent + observers).
pub trait ContextPublisher: Send + Sync {
    fn publish(&self, ctx: FormationContext);
}

/// Agent-side subscription to peer state broadcasts.
pub trait StateSubscriber: Send + Sync {
    fn try_recv(&mut self) -> Option<StateMessage>;
}
