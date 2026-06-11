//! `Intent` — Python pyclass facade for `IntentPattern`. Constructors
//! map the four operational tiers (Reconnoiter / Execute / Stabilize /
//! Surge / Dissolve) into static factory methods Python can call by
//! name.

use pyo3::prelude::*;

use springtale_cooperation::cadence::IntentPattern as CoreIntent;

/// Intent pattern facade. Variants carry their payload as Python
/// strings — the Rust newtype layer (`TaskDescriptor`, `PlanId`,
/// `StabilizeReason`, `DissolveReason`) is collapsed to `Optional[str]`
/// in the Python surface so callers don't have to model every newtype.
#[pyclass(frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct Intent {
    pub(crate) inner: CoreIntent,
}

#[pymethods]
impl Intent {
    /// Reconnoiter — gather information. `target` describes what to
    /// observe ("news/feed", "github/issues", etc.).
    #[staticmethod]
    pub fn reconnoiter(target: String) -> Self {
        Self {
            inner: CoreIntent::Reconnoiter {
                target: springtale_cooperation::cadence::TaskDescriptor(target),
            },
        }
    }

    /// Execute — act on a known plan. `plan_id` is opaque; passing
    /// `None` lets the orchestrator pick.
    #[staticmethod]
    #[pyo3(signature = (plan_id=None))]
    pub fn execute(plan_id: Option<String>) -> Self {
        Self {
            inner: CoreIntent::Execute {
                plan_id: plan_id.map(springtale_cooperation::cadence::PlanId),
            },
        }
    }

    /// Stabilize — defensive hold. `reason` documents why the formation
    /// is pausing.
    #[staticmethod]
    pub fn stabilize(reason: String) -> Self {
        Self {
            inner: CoreIntent::Stabilize {
                reason: springtale_cooperation::cadence::StabilizeReason(reason),
            },
        }
    }

    /// Surge — maximum commitment to one objective.
    #[staticmethod]
    pub fn surge(objective: String) -> Self {
        Self {
            inner: CoreIntent::Surge {
                objective: springtale_cooperation::cadence::TaskDescriptor(objective),
            },
        }
    }

    /// Dissolve — graceful wind-down. `reason` is recorded into the
    /// global knowledge store (G2) so future formations see it.
    #[staticmethod]
    pub fn dissolve(reason: String) -> Self {
        Self {
            inner: CoreIntent::Dissolve {
                reason: springtale_cooperation::cadence::DissolveReason(reason),
            },
        }
    }

    /// Variant name as a string — `"reconnoiter" | "execute" |
    /// "stabilize" | "surge" | "dissolve"`. Matches the snake_case
    /// serde tags the rest of the system uses.
    pub fn kind(&self) -> &'static str {
        match &self.inner {
            CoreIntent::Reconnoiter { .. } => "reconnoiter",
            CoreIntent::Execute { .. } => "execute",
            CoreIntent::Stabilize { .. } => "stabilize",
            CoreIntent::Surge { .. } => "surge",
            CoreIntent::Dissolve { .. } => "dissolve",
        }
    }

    fn __repr__(&self) -> String {
        format!("Intent({:?})", self.inner)
    }
}
