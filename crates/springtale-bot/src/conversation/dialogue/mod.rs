//! Slot-filling dialogue — the frame state machine.

pub mod correction;
pub mod frame;
pub mod slots;
pub mod transition;

pub use frame::{FillSource, FilledSlot, Frame, FrameStep};
pub use transition::{TurnAction, advance, seed_and_advance, step};
