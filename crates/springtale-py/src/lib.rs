//! Python bindings for Springtale's cooperation primitives.
//!
//! Curated facade — exposes just the types community Python tooling
//! needs to model cooperation without pulling the full Rust runtime
//! into Python. Per the plan (`COOPERATION_IMPLEMENTATION_PLAN.md §15`),
//! the in-scope surface is:
//!
//! - `Formation` — identity + intent + status
//! - `IntentPattern` — variant + payload
//! - `MomentumTier` — Cold / Warming / Hot / Fever
//! - `AgentId` — opaque integer-packed identity
//!
//! Out of scope: the actual runtime, transport, sentinel, and connector
//! dispatch. Python embeds the cooperation *model* — it doesn't host
//! the live bot loop. Hosts that want to embed the live runtime use
//! the WIT world from `springtale-wit` instead.
//!
//! ## Building locally
//!
//! From the workspace root:
//! ```bash
//! cargo build -p springtale-py --release
//! cp target/release/libspringtale.so springtale.so   # or .pyd on Windows
//! python -c "import springtale; print(springtale.MomentumTier.HOT)"
//! ```
//!
//! Production builds use `maturin build --release -m crates/springtale-py/Cargo.toml`
//! which wraps the cdylib in a Python wheel + ships the curated `.pyi`
//! type stubs alongside.

#![forbid(unsafe_code)]
#![allow(clippy::needless_pass_by_value)]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use springtale_cooperation::cadence::IntentPattern as CoreIntent;
use springtale_cooperation::momentum::MomentumTier as CoreTier;
use springtale_cooperation::types::FormationId as CoreFormationId;

/// Momentum tier — capability gate per `COOPERATION.md §7`. Python sees
/// this as an enum with four members; Rust round-trips through the
/// `MomentumTier::parse` / `Display` pair the rest of the system uses.
#[pyclass(eq, eq_int, frozen)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MomentumTier {
    Cold,
    Warming,
    Hot,
    Fever,
}

impl From<CoreTier> for MomentumTier {
    fn from(t: CoreTier) -> Self {
        match t {
            CoreTier::Cold => Self::Cold,
            CoreTier::Warming => Self::Warming,
            CoreTier::Hot => Self::Hot,
            CoreTier::Fever => Self::Fever,
        }
    }
}

impl From<MomentumTier> for CoreTier {
    fn from(t: MomentumTier) -> Self {
        match t {
            MomentumTier::Cold => CoreTier::Cold,
            MomentumTier::Warming => CoreTier::Warming,
            MomentumTier::Hot => CoreTier::Hot,
            MomentumTier::Fever => CoreTier::Fever,
        }
    }
}

/// Intent pattern facade. Variants carry their payload as Python
/// strings — the Rust newtype layer (`TaskDescriptor`, `PlanId`,
/// `StabilizeReason`, `DissolveReason`) is collapsed to `Optional[str]`
/// in the Python surface so callers don't have to model every newtype.
#[pyclass(frozen)]
#[derive(Clone, Debug)]
pub struct Intent {
    inner: CoreIntent,
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

/// Formation identity. Wraps the 128-bit UUID the rest of the system
/// uses; Python sees it as a string.
#[pyclass(frozen)]
#[derive(Clone, Debug)]
pub struct FormationId {
    inner: CoreFormationId,
}

#[pymethods]
impl FormationId {
    /// Generate a fresh formation id.
    #[new]
    pub fn new() -> Self {
        Self {
            inner: CoreFormationId::new(),
        }
    }

    /// Parse a formation id from its canonical UUID string.
    #[staticmethod]
    pub fn parse(s: &str) -> PyResult<Self> {
        CoreFormationId::parse(s)
            .map(|inner| Self { inner })
            .map_err(|e| PyValueError::new_err(format!("invalid formation id: {e}")))
    }

    /// Canonical UUID string form.
    fn __str__(&self) -> String {
        self.inner.0.to_string()
    }

    fn __repr__(&self) -> String {
        format!("FormationId({})", self.inner.0)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        // Stable hash over the UUID's 128 bits; Python's hash is i64
        // so we fold the high 64 into the low 64 via XOR.
        let (hi, lo) = self.inner.0.as_u64_pair();
        hi ^ lo
    }
}

impl Default for FormationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight Formation handle — read-only view a Python script gets
/// over a known formation. Mirrors the `FormationView` gossip record
/// without the live runtime hookup.
#[pyclass(frozen)]
#[derive(Clone, Debug)]
pub struct Formation {
    id: FormationId,
    intent: Intent,
    momentum_tier: MomentumTier,
}

#[pymethods]
impl Formation {
    /// Construct a new Formation handle. Pure-Python use case is for
    /// scripting / simulation; the live runtime in `springtaled` owns
    /// the real one.
    #[new]
    pub fn new(intent: Intent) -> Self {
        Self {
            id: FormationId::new(),
            intent,
            momentum_tier: MomentumTier::Cold,
        }
    }

    #[getter]
    pub fn id(&self) -> FormationId {
        self.id.clone()
    }

    #[getter]
    pub fn intent(&self) -> Intent {
        self.intent.clone()
    }

    #[getter]
    pub fn momentum_tier(&self) -> MomentumTier {
        self.momentum_tier
    }

    fn __repr__(&self) -> String {
        format!(
            "Formation(id={}, intent={}, tier={:?})",
            self.id.inner.0,
            self.intent.kind(),
            self.momentum_tier,
        )
    }
}

/// Python module entry point. `springtale.MomentumTier`, etc.
#[pymodule]
fn springtale(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MomentumTier>()?;
    m.add_class::<Intent>()?;
    m.add_class::<FormationId>()?;
    m.add_class::<Formation>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

// Rust-side unit tests live behind a feature flag: `cargo test
// -p springtale-py --features tests` from inside an environment with
// a linkable Python (so `_PyExc_*` symbols resolve). Default `cargo
// test` invocations skip these because pyo3 with `extension-module`
// defers Python symbol resolution to the host interpreter — the test
// binary has no interpreter to bind against. The Python-side test
// suite (run via `pytest`) exercises the bindings end-to-end after a
// `maturin develop` install.
