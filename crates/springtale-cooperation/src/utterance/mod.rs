//! Utterances — a cooperation primitive (plan §1.15, findings 44/54/56).
//!
//! A member's utterance is how the formation and the human both learn its
//! state. Cohn (2013): a speech balloon is perceived by others in the
//! scene, a thought bubble only by the character and the reader. So
//! `Speech`/`Burst` ride the formation bus and every carrier reaches the
//! observer stream. Defs are data (RimWorld `MoteDef`), raised at the
//! event site (Stardew `doEmote()`).

pub mod defs;
pub mod emit;
pub mod types;

pub use defs::{UtteranceDef, UtteranceDefs};
pub use emit::{UtterCtx, emit_solo, utter};
pub use types::{Carrier, Shape, Tone, Utterance, UtteranceKind};
