//! Per-step functions called in order by `AgentLoop::tick()`.
//!
//! Each step is in its own file so it can be unit-tested against a mocked
//! router/sensor/bus without spinning up the full loop. During scaffolding
//! each file is an empty stub; real logic lands in Phase K step 2 (code move)
//! and step 5/6 (L4/L5 wire-up).

pub mod inbox;
pub mod react;
pub mod respond_cfp;
pub mod scan;
pub mod sense;
