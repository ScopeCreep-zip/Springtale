//! Deterministic natural-language understanding (no ML).
//!
//! Normalization, fuzzy matching, a concept thesaurus, gazetteer
//! entity resolution, grammar entity extractors, and intent ranking —
//! the pieces that let the bot understand a free sentence without an
//! LLM.

pub mod entities;
pub mod fuzzy;
pub mod gazetteer;
pub mod intent;
pub mod normalize;
pub mod synonyms;

pub use gazetteer::{GazHit, Gazetteer};
pub use intent::{IntentCandidate, IntentDecision, decide, rank};
pub use normalize::{Token, tokenize};
