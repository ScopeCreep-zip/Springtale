//! Natural-language generation — templated, varied, deterministic.

pub mod phrasebook;
pub mod reflect;
pub mod render;

pub use render::{Move, SlotPrompt, SummaryLine, render};
