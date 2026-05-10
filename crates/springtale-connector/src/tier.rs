//! Capability tier — shared between the capability checker and the
//! WASM sandbox's per-tier `InstancePre` cache.
//!
//! Mirrors `springtale_cooperation::momentum::MomentumTier`. The enum
//! lives here (rather than inside `wasm/`) because the capability
//! checker — always compiled regardless of the `wasm-sandbox` feature
//! — needs to know which tier a pending invocation is bound to, even
//! when no WASM sandbox is present. The actual per-tier Linker
//! building (`register_tier_primitives`, `WasmTierCache`) stays behind
//! the feature gate.
//!
//! Conversion from `MomentumTier` happens in `springtale-runtime`'s
//! `CapabilityBridge` (connector crate cannot depend on cooperation).

/// Momentum tier used for capability gating.
///
/// | Tier    | HTTP | Notes                                      |
/// |---------|:----:|--------------------------------------------|
/// | Cold    |  —   | Assembly-only. No network, no env writes.  |
/// | Warming |  ✓   | Neighbor reads + chaining + HTTP.          |
/// | Hot     |  ✓   | Environment writes + synchronized commits. |
/// | Fever   |  ✓   | Consensus + AI + recruit.                  |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WasmTier {
    Cold,
    Warming,
    Hot,
    Fever,
}

impl WasmTier {
    /// All four tiers in index order — used by `WasmTierCache` to iterate.
    pub const ALL: [WasmTier; 4] = [
        WasmTier::Cold,
        WasmTier::Warming,
        WasmTier::Hot,
        WasmTier::Fever,
    ];

    /// Stable index into any length-4 tier-keyed array.
    pub fn index(self) -> usize {
        match self {
            WasmTier::Cold => 0,
            WasmTier::Warming => 1,
            WasmTier::Hot => 2,
            WasmTier::Fever => 3,
        }
    }

    /// Inverse of [`index`]. Returns `None` if `idx` is out of range.
    pub fn from_index(idx: usize) -> Option<WasmTier> {
        match idx {
            0 => Some(WasmTier::Cold),
            1 => Some(WasmTier::Warming),
            2 => Some(WasmTier::Hot),
            3 => Some(WasmTier::Fever),
            _ => None,
        }
    }
}

impl Default for WasmTier {
    /// Cold — the conservative default. Matches the starting point of
    /// `MomentumState::default()` in springtale-cooperation.
    fn default() -> Self {
        WasmTier::Cold
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn all_covers_every_variant_once() {
        assert_eq!(WasmTier::ALL.len(), 4);
        let mut idxs: Vec<usize> = WasmTier::ALL.iter().map(|t| t.index()).collect();
        idxs.sort();
        assert_eq!(idxs, vec![0, 1, 2, 3]);
    }

    #[test]
    fn from_index_roundtrip() {
        for tier in WasmTier::ALL {
            assert_eq!(WasmTier::from_index(tier.index()), Some(tier));
        }
        assert_eq!(WasmTier::from_index(4), None);
    }

    #[test]
    fn default_is_cold() {
        assert_eq!(WasmTier::default(), WasmTier::Cold);
    }

    #[test]
    fn tier_ordering_is_ascending() {
        assert!(WasmTier::Cold < WasmTier::Warming);
        assert!(WasmTier::Warming < WasmTier::Hot);
        assert!(WasmTier::Hot < WasmTier::Fever);
    }
}
