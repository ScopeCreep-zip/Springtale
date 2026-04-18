//! Measure trait — combines multiple weighted scores into a single value.
//!
//! Per big-brain's measures.rs and Dave Mark's IAUS:
//! - WeightedSum: Σ(score × weight) — additive, good for combining independent factors
//! - WeightedProduct: Π(score × weight) — multiplicative, one zero kills whole
//! - ChebyshevDistance: max(score × weight) — only strongest signal matters
//! - Compensated: √(Σ(w_normalized × score²)) — RMS-like, penalizes extremes

/// Combines multiple (score, weight) pairs into a single utility value.
pub trait Measure: std::fmt::Debug + Send + Sync {
    fn calculate(&self, scores: &[(f32, f32)]) -> f32;
}

/// Sum of (score × weight). Good for combining independent factors.
/// "I'm a little thirsty AND a little hungry = moderately needy."
#[derive(Debug)]
pub struct WeightedSum;

impl Measure for WeightedSum {
    fn calculate(&self, scores: &[(f32, f32)]) -> f32 {
        scores.iter().map(|(s, w)| s * w).sum()
    }
}

/// Product of (score × weight). One zero kills the whole score.
/// "I need ammo AND a visible target — either missing = can't attack."
///
/// Per GDC "Building a Better Centaur": includes optional compensation
/// for the N-input deflation problem (0.8^5 = 0.33 but 0.8^2 = 0.64).
/// Compensation formula: final = product + (1 - product) * mod_factor * product
/// where mod_factor = 1 - (1 / num_scorers).
#[derive(Debug)]
pub struct WeightedProduct {
    /// Enable compensation for N-input deflation.
    pub compensated: bool,
}

impl Measure for WeightedProduct {
    fn calculate(&self, scores: &[(f32, f32)]) -> f32 {
        if scores.is_empty() {
            return 0.0;
        }

        let product: f32 = scores.iter().fold(1.0, |acc, (s, w)| acc * (s * w).max(0.0));

        if self.compensated && scores.len() > 1 {
            let mod_factor = 1.0 - (1.0 / scores.len() as f32);
            let makeup = (1.0 - product) * mod_factor;
            (product + makeup * product).clamp(0.0, 1.0)
        } else {
            product.clamp(0.0, 1.0)
        }
    }
}

/// Maximum of (score × weight). Only the strongest signal matters.
/// "Flee from the MOST dangerous threat, ignore minor ones."
#[derive(Debug)]
pub struct ChebyshevDistance;

impl Measure for ChebyshevDistance {
    fn calculate(&self, scores: &[(f32, f32)]) -> f32 {
        scores
            .iter()
            .fold(0.0f32, |best, (s, w)| (s * w).max(best))
    }
}

/// Compensated weighted measure — RMS-like scoring.
/// Penalizes extreme values more than WeightedSum.
/// √(Σ(w_normalized × score²))
#[derive(Debug)]
pub struct Compensated;

impl Measure for Compensated {
    fn calculate(&self, scores: &[(f32, f32)]) -> f32 {
        let weight_sum: f32 = scores.iter().map(|(_, w)| w).sum();
        if weight_sum == 0.0 {
            return 0.0;
        }
        scores
            .iter()
            .map(|(s, w)| (w / weight_sum) * s.powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_sum() {
        let m = WeightedSum;
        let score = m.calculate(&[(0.8, 0.5), (0.6, 0.5)]);
        assert!((score - 0.7).abs() < 0.01); // 0.8*0.5 + 0.6*0.5 = 0.7
    }

    #[test]
    fn test_weighted_sum_empty() {
        let m = WeightedSum;
        assert!((m.calculate(&[]) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_weighted_product_one_zero() {
        let m = WeightedProduct { compensated: false };
        let score = m.calculate(&[(0.8, 1.0), (0.0, 1.0)]);
        assert!((score - 0.0).abs() < f32::EPSILON); // one zero kills all
    }

    #[test]
    fn test_weighted_product_compensation() {
        let m_plain = WeightedProduct { compensated: false };
        let m_comp = WeightedProduct { compensated: true };

        let scores = &[(0.8, 1.0), (0.8, 1.0), (0.8, 1.0), (0.8, 1.0), (0.8, 1.0)];

        let plain = m_plain.calculate(scores);   // 0.8^5 = 0.328
        let comp = m_comp.calculate(scores);      // compensated, should be higher

        assert!(comp > plain, "compensated ({comp}) should be > plain ({plain})");
    }

    #[test]
    fn test_chebyshev_max() {
        let m = ChebyshevDistance;
        let score = m.calculate(&[(0.3, 1.0), (0.9, 1.0), (0.5, 1.0)]);
        assert!((score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compensated_rms() {
        let m = Compensated;
        let score = m.calculate(&[(0.8, 0.5), (0.6, 0.5)]);
        // √(0.5 * 0.64 + 0.5 * 0.36) = √(0.32 + 0.18) = √0.5 ≈ 0.707
        assert!((score - 0.707).abs() < 0.01);
    }
}
