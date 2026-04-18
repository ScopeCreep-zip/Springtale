//! Role transformation — agents change roles when capabilities are lost.
//!
//! Per COOPERATION.pdf §14: Dead agents aren't removed — they're transformed.

pub mod trigger;
mod types;

pub use types::RoleTransformation;
