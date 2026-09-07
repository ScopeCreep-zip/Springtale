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
//!
//! Rust-side unit tests live behind a feature flag: `cargo test
//! -p springtale-py --features tests` from inside an environment with
//! a linkable Python (so `_PyExc_*` symbols resolve). Default `cargo
//! test` invocations skip these because pyo3 with `extension-module`
//! defers Python symbol resolution to the host interpreter — the test
//! binary has no interpreter to bind against. The Python-side test
//! suite (run via `pytest`) exercises the bindings end-to-end after a
//! `maturin develop` install.

#![forbid(unsafe_code)]
#![allow(clippy::needless_pass_by_value)]

pub mod convert;
pub mod formation;
pub mod formation_id;
pub mod intent;
pub mod module;
pub mod momentum;

pub use formation::Formation;
pub use formation_id::FormationId;
pub use intent::Intent;
pub use momentum::MomentumTier;
