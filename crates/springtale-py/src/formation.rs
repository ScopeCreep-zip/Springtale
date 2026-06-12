//! `Formation` — read-only Python view of a formation handle. Mirrors
//! the `FormationView` gossip record without the live runtime hookup.
//! Live runtime lives in `springtaled`; Python embeds the cooperation
//! *model* only.

use pyo3::prelude::*;

use crate::formation_id::FormationId;
use crate::intent::Intent;
use crate::momentum::MomentumTier;

/// Lightweight Formation handle — read-only view a Python script gets
/// over a known formation. Mirrors the `FormationView` gossip record
/// without the live runtime hookup.
#[pyclass(frozen, from_py_object)]
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
