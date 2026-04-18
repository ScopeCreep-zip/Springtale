use std::sync::atomic::{AtomicU64, Ordering};

use super::error::OrchestratorError;

/// Atomic fuel budget for pipeline execution.
///
/// Uses compare-and-swap (CAS) for safe concurrent consumption.
/// NOT fetch_sub — that wraps on underflow (u64::MAX).
pub struct FuelBudget {
    remaining: AtomicU64,
    initial: u64,
}

impl Clone for FuelBudget {
    fn clone(&self) -> Self {
        Self {
            remaining: AtomicU64::new(self.remaining.load(Ordering::Acquire)),
            initial: self.initial,
        }
    }
}

impl FuelBudget {
    /// Create a new fuel budget with the given total.
    pub fn new(total: u64) -> Self {
        Self {
            remaining: AtomicU64::new(total),
            initial: total,
        }
    }

    /// Replenish fuel — used by L6 `InjectFuel` interventions. Saturates at
    /// u64::MAX so a misconfigured intervention cannot overflow the counter.
    pub fn replenish(&self, amount: u64) {
        loop {
            let current = self.remaining.load(Ordering::Acquire);
            let next = current.saturating_add(amount);
            if self
                .remaining
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }

    /// Consume fuel. Returns remaining after consumption.
    /// Uses CAS loop to prevent underflow.
    pub fn consume(&self, amount: u64) -> Result<u64, OrchestratorError> {
        loop {
            let current = self.remaining.load(Ordering::Acquire);
            if current < amount {
                return Err(OrchestratorError::FuelExhausted {
                    requested: amount,
                    remaining: current,
                });
            }
            match self.remaining.compare_exchange(
                current,
                current - amount,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(current - amount),
                Err(_) => continue, // retry on contention
            }
        }
    }

    /// Split remaining fuel among N children.
    /// Each child gets `remaining / n`. Returns a Vec of child budgets.
    pub fn split(&self, n: u32) -> Result<Vec<FuelBudget>, OrchestratorError> {
        if n == 0 {
            return Ok(vec![]);
        }
        let remaining = self.remaining.load(Ordering::Acquire);
        let per_child = remaining / u64::from(n);
        if per_child == 0 {
            return Err(OrchestratorError::FuelExhausted {
                requested: u64::from(n),
                remaining,
            });
        }

        // Deduct total from parent
        let total_deducted = per_child * u64::from(n);
        self.consume(total_deducted)?;

        Ok((0..n).map(|_| FuelBudget::new(per_child)).collect())
    }

    /// Get remaining fuel.
    pub fn remaining(&self) -> u64 {
        self.remaining.load(Ordering::Acquire)
    }

    /// Get initial fuel budget.
    pub fn initial(&self) -> u64 {
        self.initial
    }

    /// Get fuel consumed so far.
    pub fn consumed(&self) -> u64 {
        self.initial - self.remaining()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_consume_basic() {
        let budget = FuelBudget::new(1000);
        assert_eq!(budget.consume(100).unwrap(), 900);
        assert_eq!(budget.remaining(), 900);
    }

    #[test]
    fn test_consume_exhausted() {
        let budget = FuelBudget::new(50);
        let result = budget.consume(100);
        assert!(matches!(
            result,
            Err(OrchestratorError::FuelExhausted { .. })
        ));
    }

    #[test]
    fn test_consume_exact() {
        let budget = FuelBudget::new(100);
        assert_eq!(budget.consume(100).unwrap(), 0);
        assert!(budget.consume(1).is_err());
    }

    #[test]
    fn test_split_even() {
        let budget = FuelBudget::new(1000);
        let children = budget.split(4).unwrap();
        assert_eq!(children.len(), 4);
        for child in &children {
            assert_eq!(child.remaining(), 250);
        }
        assert_eq!(budget.remaining(), 0);
    }

    #[test]
    fn test_split_uneven() {
        let budget = FuelBudget::new(1000);
        let children = budget.split(3).unwrap();
        assert_eq!(children.len(), 3);
        for child in &children {
            assert_eq!(child.remaining(), 333);
        }
        // 1000 - 333*3 = 1 remaining in parent (rounding)
        assert_eq!(budget.remaining(), 1);
    }

    #[test]
    fn test_split_zero() {
        let budget = FuelBudget::new(1000);
        let children = budget.split(0).unwrap();
        assert!(children.is_empty());
        assert_eq!(budget.remaining(), 1000);
    }

    #[test]
    fn test_split_insufficient() {
        let budget = FuelBudget::new(2);
        let result = budget.split(3);
        assert!(matches!(
            result,
            Err(OrchestratorError::FuelExhausted { .. })
        ));
    }

    #[test]
    fn test_consumed_tracking() {
        let budget = FuelBudget::new(1000);
        budget.consume(300).unwrap();
        assert_eq!(budget.consumed(), 300);
        assert_eq!(budget.remaining(), 700);
    }

    #[test]
    fn test_concurrent_consume() {
        use std::sync::Arc;
        let budget = Arc::new(FuelBudget::new(1000));
        let mut handles = vec![];

        for _ in 0..10 {
            let b = budget.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..10 {
                    let _ = b.consume(1);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // 10 threads × 10 consumes × 1 each = 100 consumed
        assert_eq!(budget.consumed(), 100);
        assert_eq!(budget.remaining(), 900);
    }
}
