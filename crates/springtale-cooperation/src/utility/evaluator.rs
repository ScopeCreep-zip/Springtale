//! Response curves — shape raw input values into 0.0-1.0 scores.
//!
//! Per Dave Mark's GameAIPro3 Ch13: "the response curve choice IS
//! the game design." Different curves create different agent behaviors:
//!
//! - Linear: every unit of change matters equally (distance scoring)
//! - Power(0.5): diminishing returns (start caring early — survival needs)
//! - Power(2.0): accelerating urgency (ignore small, react to big — damage)
//! - Sigmoid: dead zone at extremes, decision zone in middle (prevents oscillation)

/// Maps a raw input value to a 0.0-1.0 score.
pub trait ResponseCurve: std::fmt::Debug + Send + Sync {
    /// Evaluate the curve at the given input (clamped to bookends).
    fn evaluate(&self, input: f32) -> f32;
}

/// Linear interpolation between min and max.
/// Input at min → 0.0, input at max → 1.0.
#[derive(Debug)]
pub struct Linear {
    pub min: f32,
    pub max: f32,
}

impl ResponseCurve for Linear {
    fn evaluate(&self, input: f32) -> f32 {
        if (self.max - self.min).abs() < f32::EPSILON {
            return 0.0;
        }
        ((input - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }
}

/// Power curve — input^exponent after normalization.
///
/// - exponent < 1 (e.g., 0.5 = sqrt): diminishing returns.
///   Thirst 20% → score 0.45 (already somewhat urgent).
///   Agent starts seeking water early. Good for survival needs.
///
/// - exponent > 1 (e.g., 2.0 = quadratic): accelerating urgency.
///   Health loss 20% → score 0.04 (barely care).
///   Agent ignores minor damage, reacts to serious injury.
#[derive(Debug)]
pub struct Power {
    pub min: f32,
    pub max: f32,
    pub exponent: f32,
}

impl ResponseCurve for Power {
    fn evaluate(&self, input: f32) -> f32 {
        if (self.max - self.min).abs() < f32::EPSILON {
            return 0.0;
        }
        let normalized = ((input - self.min) / (self.max - self.min)).clamp(0.0, 1.0);
        normalized.powf(self.exponent)
    }
}

/// Sigmoid curve — S-shaped, creates dead zones at extremes.
///
/// Prevents oscillation: enemy strength at 30% → ignore, at 50% →
/// ramps sharply, at 70% → definitely respond. The steep middle
/// zone means agents commit to decisions rather than flip-flopping.
///
/// - steepness: how sharp the transition (higher = sharper)
/// - midpoint: input value where output = 0.5
#[derive(Debug)]
pub struct Sigmoid {
    pub midpoint: f32,
    pub steepness: f32,
}

impl ResponseCurve for Sigmoid {
    fn evaluate(&self, input: f32) -> f32 {
        1.0 / (1.0 + (-self.steepness * (input - self.midpoint)).exp())
    }
}

/// Inverted curve — flips any other curve (high input = low score).
#[derive(Debug)]
pub struct Inverted<C: ResponseCurve> {
    pub inner: C,
}

impl<C: ResponseCurve> ResponseCurve for Inverted<C> {
    fn evaluate(&self, input: f32) -> f32 {
        1.0 - self.inner.evaluate(input)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_boundaries() {
        let curve = Linear {
            min: 0.0,
            max: 100.0,
        };
        assert!((curve.evaluate(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((curve.evaluate(50.0) - 0.5).abs() < f32::EPSILON);
        assert!((curve.evaluate(100.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_linear_clamps() {
        let curve = Linear {
            min: 0.0,
            max: 100.0,
        };
        assert!((curve.evaluate(-10.0) - 0.0).abs() < f32::EPSILON);
        assert!((curve.evaluate(200.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_power_diminishing_returns() {
        let curve = Power {
            min: 0.0,
            max: 1.0,
            exponent: 0.5,
        };
        // sqrt(0.5) ≈ 0.707 — already high at midpoint
        let mid = curve.evaluate(0.5);
        assert!(
            mid > 0.6,
            "diminishing returns: mid ({mid}) should be > 0.6"
        );
    }

    #[test]
    fn test_power_accelerating() {
        let curve = Power {
            min: 0.0,
            max: 1.0,
            exponent: 2.0,
        };
        // 0.5^2 = 0.25 — still low at midpoint
        let mid = curve.evaluate(0.5);
        assert!(mid < 0.3, "accelerating: mid ({mid}) should be < 0.3");
    }

    #[test]
    fn test_sigmoid_midpoint() {
        let curve = Sigmoid {
            midpoint: 0.5,
            steepness: 10.0,
        };
        let mid = curve.evaluate(0.5);
        assert!(
            (mid - 0.5).abs() < 0.01,
            "sigmoid midpoint should be ~0.5, got {mid}"
        );
    }

    #[test]
    fn test_sigmoid_dead_zones() {
        let curve = Sigmoid {
            midpoint: 0.5,
            steepness: 10.0,
        };
        let low = curve.evaluate(0.1);
        let high = curve.evaluate(0.9);
        assert!(low < 0.05, "sigmoid low end should be near 0, got {low}");
        assert!(high > 0.95, "sigmoid high end should be near 1, got {high}");
    }

    #[test]
    fn test_inverted() {
        let curve = Inverted {
            inner: Linear { min: 0.0, max: 1.0 },
        };
        assert!((curve.evaluate(0.0) - 1.0).abs() < f32::EPSILON);
        assert!((curve.evaluate(1.0) - 0.0).abs() < f32::EPSILON);
    }
}
