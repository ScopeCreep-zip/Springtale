//! `ProcId` — foca Identity for a springtaled process.
//!
//! Per COOPERATION.md §8.3: one SWIM node per springtaled process.
//! The address is the UDP socket the node listens on; the `bump`
//! counter breaks ties when foca sees multiple identities sharing the
//! same address (restart scenario — `renew()` bumps and the new
//! identity wins via `win_addr_conflict`).

use std::net::SocketAddr;

use foca::Identity;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Cluster-wide identity for a Springtale process. `addr` is the UDP
/// endpoint foca probes + sends to; `bump` is a monotonic version so
/// process restarts that re-bind the same port present as a new
/// identity that wins the address conflict.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Type)]
pub struct ProcId {
    pub addr: SocketAddr,
    pub bump: u64,
}

impl ProcId {
    /// Fresh identity at bump 0.
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr, bump: 0 }
    }
}

impl Identity for ProcId {
    type Addr = SocketAddr;

    fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn renew(&self) -> Option<Self> {
        // When foca declares us Down, bump the incarnation and
        // re-announce. Wraps on overflow — in practice a u64 restart
        // counter won't wrap.
        Some(Self {
            addr: self.addr,
            bump: self.bump.wrapping_add(1),
        })
    }

    fn win_addr_conflict(&self, other: &Self) -> bool {
        self.bump > other.bump
    }
}
