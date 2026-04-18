//! L1 CAS ownership — atomic claim/release tracked in a sharded map.

pub mod cas;
pub mod manager;

pub use manager::ClaimManager;
