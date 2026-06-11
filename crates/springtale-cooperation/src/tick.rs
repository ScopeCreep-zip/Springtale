//! Canonical cadence tick identifier.
//!
//! `TickId` is the opaque monotonic counter from
//! `COOPERATION_IMPLEMENTATION_PLAN.md` §11 decision #2: ticks are the
//! load-bearing time axis for deterministic replay, so a tick must never
//! be confused with any other `u64` travelling alongside it (e.g.
//! `ActionDescriptor::payload_hash`). Newtype-per-domain is the
//! established deterministic-simulation pattern (see the `tick-id` crate
//! and Rust API guidelines C-NEWTYPE).
//!
//! Wall-clock time (`Instant`) is observability-only; cooperation
//! semantics are expressed in ticks.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Monotonic tick counter. One per cadence-bus emission.
///
/// Opaque on purpose — consumers should not assume a tick-to-seconds
/// ratio; the cadence bus owns the interval and pacing dividers change
/// the effective per-formation rate (§22).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[repr(transparent)]
pub struct TickId(pub u64);

impl TickId {
    /// Zero tick — the genesis of a formation's lifetime.
    pub const ZERO: Self = Self(0);

    /// The next tick in sequence.
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Difference in ticks (saturating — never panics).
    pub const fn delta(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

impl From<u64> for TickId {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl std::fmt::Display for TickId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn next_and_delta() {
        let t = TickId::ZERO;
        let n = t.next();
        assert_eq!(n, TickId(1));
        assert_eq!(n.delta(t), 1);
        assert_eq!(t.delta(n), 0, "delta saturates, never underflows");
    }

    #[test]
    fn ordering_is_sequence_order() {
        assert!(TickId(2) > TickId(1));
        assert!(TickId(1) >= TickId(1));
    }

    #[test]
    fn serde_roundtrip_is_transparent_u64() {
        let t = TickId(42);
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "42", "repr(transparent) newtype serializes as u64");
        let back: TickId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn display_is_hash_prefixed() {
        assert_eq!(TickId(7).to_string(), "#7");
    }
}
