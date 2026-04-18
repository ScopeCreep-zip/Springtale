use crate::momentum::MomentumTier;

/// Static tier → cooperation-primitive names table (spec §7).
///
/// `read_env` is universal; everything else unlocks progressively. Returns
/// `&'static [&'static str]` so callers can iterate without allocating.
pub fn unlocked_for_tier(tier: MomentumTier) -> &'static [&'static str] {
    match tier {
        MomentumTier::Cold => COLD,
        MomentumTier::Warming => WARMING,
        MomentumTier::Hot => HOT,
        MomentumTier::Fever => FEVER,
    }
}

const COLD: &[&str] = &["read_env"];
const WARMING: &[&str] = &["read_env", "read_neighbors", "chain"];
const HOT: &[&str] = &[
    "read_env",
    "read_neighbors",
    "chain",
    "write_env",
    "synchronized_commit",
];
const FEVER: &[&str] = &[
    "read_env",
    "read_neighbors",
    "chain",
    "write_env",
    "synchronized_commit",
    "consensus",
    "ai_call",
    "recruit",
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn each_tier_is_a_superset_of_lower_tiers() {
        let cold = unlocked_for_tier(MomentumTier::Cold);
        let warming = unlocked_for_tier(MomentumTier::Warming);
        let hot = unlocked_for_tier(MomentumTier::Hot);
        let fever = unlocked_for_tier(MomentumTier::Fever);
        for c in cold {
            assert!(warming.contains(c), "Warming must contain all Cold items");
            assert!(hot.contains(c));
            assert!(fever.contains(c));
        }
        for w in warming {
            assert!(hot.contains(w));
            assert!(fever.contains(w));
        }
        for h in hot {
            assert!(fever.contains(h));
        }
    }

    #[test]
    fn fever_unlocks_consensus_and_ai() {
        let fever = unlocked_for_tier(MomentumTier::Fever);
        assert!(fever.contains(&"consensus"));
        assert!(fever.contains(&"ai_call"));
        assert!(fever.contains(&"recruit"));
    }
}
