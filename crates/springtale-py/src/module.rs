//! The Python module entry point. Registering the classes is its own
//! concern, kept out of `lib.rs` so the crate root stays a table of
//! contents (`.claude/rules/backend/crate-structure.md`).

use pyo3::prelude::*;

use crate::formation::Formation;
use crate::formation_id::FormationId;
use crate::intent::Intent;
use crate::momentum::MomentumTier;

/// Python module entry point. `springtale.MomentumTier`, etc.
#[pymodule]
pub fn springtale(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MomentumTier>()?;
    m.add_class::<Intent>()?;
    m.add_class::<FormationId>()?;
    m.add_class::<Formation>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
