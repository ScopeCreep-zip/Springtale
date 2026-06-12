//! `FormationId` — Python pyclass facade wrapping the 128-bit UUID
//! `springtale_cooperation::types::FormationId` uses elsewhere.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use springtale_cooperation::types::FormationId as CoreFormationId;

/// Formation identity. Wraps the 128-bit UUID the rest of the system
/// uses; Python sees it as a string.
#[pyclass(frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct FormationId {
    pub(crate) inner: CoreFormationId,
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
