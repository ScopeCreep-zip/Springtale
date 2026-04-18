//! Picker — selects the winning action from scored candidates.
//!
//! Per big-brain's pickers.rs:
//! - FirstToScore: priority-based, first above threshold wins (order matters)
//! - Highest: pure utility maximizer, highest score wins
//! - HighestToScore: highest above a minimum floor

/// Selects from a list of (index, score) pairs.
pub trait Picker: std::fmt::Debug + Send + Sync {
    /// Pick a winner from the scored candidates.
    /// Returns the index of the winner, or None if no candidate qualifies.
    fn pick(&self, scores: &[(usize, f32)]) -> Option<usize>;
}

/// First candidate that scores above the threshold.
/// Order matters — earlier candidates have priority.
///
/// "Check for emergency first (flee), then combat, then patrol."
#[derive(Debug)]
pub struct FirstToScore {
    pub threshold: f32,
}

impl Picker for FirstToScore {
    fn pick(&self, scores: &[(usize, f32)]) -> Option<usize> {
        scores
            .iter()
            .find(|(_, score)| *score >= self.threshold)
            .map(|(idx, _)| *idx)
    }
}

/// Highest-scoring candidate wins. Pure utility maximizer.
///
/// "Do whatever has the highest expected value right now."
#[derive(Debug)]
pub struct Highest;

impl Picker for Highest {
    fn pick(&self, scores: &[(usize, f32)]) -> Option<usize> {
        scores
            .iter()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .filter(|(_, score)| *score > 0.0)
            .map(|(idx, _)| *idx)
    }
}

/// Highest-scoring candidate above a minimum floor.
///
/// "Do the best thing available, but only if it's worth doing."
#[derive(Debug)]
pub struct HighestToScore {
    pub threshold: f32,
}

impl Picker for HighestToScore {
    fn pick(&self, scores: &[(usize, f32)]) -> Option<usize> {
        scores
            .iter()
            .filter(|(_, score)| *score >= self.threshold)
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| *idx)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_first_to_score() {
        let picker = FirstToScore { threshold: 0.5 };
        // Index 1 is first above 0.5
        let result = picker.pick(&[(0, 0.3), (1, 0.6), (2, 0.9)]);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_first_to_score_none() {
        let picker = FirstToScore { threshold: 0.9 };
        let result = picker.pick(&[(0, 0.3), (1, 0.5)]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_highest() {
        let picker = Highest;
        let result = picker.pick(&[(0, 0.3), (1, 0.9), (2, 0.5)]);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_highest_zero_scores() {
        let picker = Highest;
        let result = picker.pick(&[(0, 0.0), (1, 0.0)]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_highest_to_score() {
        let picker = HighestToScore { threshold: 0.4 };
        // 0.3 is below threshold, 0.5 and 0.9 qualify, 0.9 wins
        let result = picker.pick(&[(0, 0.3), (1, 0.9), (2, 0.5)]);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_highest_to_score_none_qualify() {
        let picker = HighestToScore { threshold: 0.8 };
        let result = picker.pick(&[(0, 0.3), (1, 0.5), (2, 0.7)]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_empty_scores() {
        assert_eq!(Highest.pick(&[]), None);
        assert_eq!(FirstToScore { threshold: 0.1 }.pick(&[]), None);
        assert_eq!(HighestToScore { threshold: 0.1 }.pick(&[]), None);
    }
}
