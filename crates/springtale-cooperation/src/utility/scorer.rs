//! Composite scorers — combine multiple scored considerations into
//! a single utility value.
//!
//! Per big-brain's choices.rs and Dave Mark's IAUS:
//! - AllOrNothing: all must pass threshold, else 0
//! - SumOfScorers: additive (weak signals combine)
//! - ProductOfScorers: multiplicative (one zero kills — "need all conditions")
//! - WinningScorer: max only (strongest signal wins)
//! - MeasuredScorer: pluggable Measure with weights

use super::measure::Measure;

/// All child scores must meet the threshold, else total is 0.
/// "Attack requires BOTH ammo AND visible target."
#[derive(Debug)]
pub struct AllOrNothing {
    pub threshold: f32,
}

impl AllOrNothing {
    pub fn evaluate(&self, scores: &[f32]) -> f32 {
        if scores.iter().all(|s| *s >= self.threshold) {
            scores.iter().sum()
        } else {
            0.0
        }
    }
}

/// Sum of scores, gated by threshold.
/// "Investigate = heard_noise(0.3) + saw_shadow(0.4) = 0.7"
#[derive(Debug)]
pub struct SumOfScorers {
    pub threshold: f32,
}

impl SumOfScorers {
    pub fn evaluate(&self, scores: &[f32]) -> f32 {
        let sum: f32 = scores.iter().sum();
        if sum >= self.threshold { sum } else { 0.0 }
    }
}

/// Product of scores — one zero kills the whole.
/// Per GDC talk: includes optional compensation for N-input deflation.
#[derive(Debug)]
pub struct ProductOfScorers {
    pub compensated: bool,
}

impl ProductOfScorers {
    pub fn evaluate(&self, scores: &[f32]) -> f32 {
        if scores.is_empty() {
            return 0.0;
        }
        let product: f32 = scores.iter().fold(1.0, |acc, s| acc * s.max(0.0));
        if self.compensated && scores.len() > 1 {
            let mod_factor = 1.0 - (1.0 / scores.len() as f32);
            let makeup = (1.0 - product) * mod_factor;
            (product + makeup * product).clamp(0.0, 1.0)
        } else {
            product.clamp(0.0, 1.0)
        }
    }
}

/// Maximum score only — strongest signal wins.
/// "Flee from the MOST dangerous threat."
#[derive(Debug)]
pub struct WinningScorer {
    pub threshold: f32,
}

impl WinningScorer {
    pub fn evaluate(&self, scores: &[f32]) -> f32 {
        let max = scores.iter().copied().fold(0.0f32, f32::max);
        if max >= self.threshold { max } else { 0.0 }
    }
}

/// Pluggable measure with per-score weights.
/// Uses any Measure implementation (WeightedSum, WeightedProduct, etc.)
#[derive(Debug)]
pub struct MeasuredScorer<M: Measure> {
    pub measure: M,
}

impl<M: Measure> MeasuredScorer<M> {
    pub fn evaluate(&self, weighted_scores: &[(f32, f32)]) -> f32 {
        self.measure.calculate(weighted_scores)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use super::super::measure::WeightedSum;

    #[test]
    fn test_all_or_nothing_pass() {
        let scorer = AllOrNothing { threshold: 0.3 };
        let result = scorer.evaluate(&[0.5, 0.6, 0.7]);
        assert!((result - 1.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_all_or_nothing_fail() {
        let scorer = AllOrNothing { threshold: 0.5 };
        let result = scorer.evaluate(&[0.8, 0.3, 0.9]); // 0.3 < 0.5
        assert!((result - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sum_of_scorers_combines() {
        let scorer = SumOfScorers { threshold: 0.5 };
        let result = scorer.evaluate(&[0.3, 0.4]); // 0.7 >= 0.5
        assert!((result - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sum_of_scorers_below_threshold() {
        let scorer = SumOfScorers { threshold: 0.8 };
        let result = scorer.evaluate(&[0.3, 0.4]); // 0.7 < 0.8
        assert!((result - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_product_one_zero() {
        let scorer = ProductOfScorers { compensated: false };
        let result = scorer.evaluate(&[0.8, 0.0, 0.9]);
        assert!((result - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_winning_scorer() {
        let scorer = WinningScorer { threshold: 0.5 };
        let result = scorer.evaluate(&[0.3, 0.9, 0.5]);
        assert!((result - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_winning_scorer_below_threshold() {
        let scorer = WinningScorer { threshold: 0.8 };
        let result = scorer.evaluate(&[0.3, 0.5, 0.7]);
        assert!((result - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_measured_scorer() {
        let scorer = MeasuredScorer { measure: WeightedSum };
        let result = scorer.evaluate(&[(0.8, 0.5), (0.6, 0.5)]);
        assert!((result - 0.7).abs() < 0.01);
    }
}
