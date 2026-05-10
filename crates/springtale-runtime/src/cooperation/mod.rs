//! Runtime-level cooperation plumbing — glue between
//! `springtale-cooperation` primitives (momentum, formation state) and
//! `springtale-connector` execution (capability checker + WASM tier
//! cache).
//!
//! This module exists because the connector crate cannot depend on the
//! cooperation crate (connector sits below cooperation in the
//! dep graph). Anything that translates momentum state into capability
//! decisions lives here.

pub mod capability_bridge;
pub mod role_registration;

pub use capability_bridge::{
    momentum_to_wasm_tier, BridgeError, CapabilityBridge,
};
pub use role_registration::{register_manifest_roles, unregister_manifest_roles};
