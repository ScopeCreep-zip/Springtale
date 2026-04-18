//! L2 state dissemination — `watch` for FormationContext + `broadcast` for
//! peer state messages.
//!
//! Family 4 (AutoGen topic/subscription, Akka cluster sharding). This layer
//! carries coordination *state*, not tasks — agents reading FormationContext
//! see the current intent/momentum/phase; peer broadcasts carry liveness and
//! status changes.

pub mod publisher;
pub mod state_msg;
pub mod trait_;

pub use publisher::BusContextPublisher;
pub use state_msg::StateMessage;
pub use trait_::{ContextPublisher, StateSubscriber};
