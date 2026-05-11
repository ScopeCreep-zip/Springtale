//! Cross-formation gossip bus (G6 / `COOPERATION_IMPLEMENTATION_PLAN.md §12.2`).
//!
//! Every formation publishes a small `FormationView` whenever its
//! intent / momentum / outcome changes; peer formations subscribe to
//! the stream so they can adapt their own intent in response. Per the
//! plan, this rides on the same `chitchat`-backed substrate the per-agent
//! `awareness` gossip uses (`docs/intended-arch/COOPERATION_IMPLEMENTATION_PLAN.md §12.2`)
//! but exposes a different *grain* of state — one entry per formation
//! rather than one per agent.
//!
//! ## Why a separate trait from `awareness::GossipStore`
//!
//! `GossipStore` was specced for **agent-level** state (capabilities,
//! load, liveness). Folding formation views into the same trait would
//! force every implementor to reason about two key namespaces with
//! different lifecycle and TTL semantics:
//!
//! - Agent entries are dense, churn fast (every tick), and tied to
//!   per-process liveness.
//! - Formation entries are sparse, change at "decision time" only
//!   (intent change, dissolve, replan), and need durable broadcast
//!   even after the originating formation dissolves.
//!
//! Two traits keep each store's invariants tight. They can share
//! transport (chitchat) without sharing schema.
//!
//! ## Use cases (per spec §17.2)
//!
//! - Formation A succeeds at "investigate news" — formation B handling
//!   the same connector sees the success and lowers its retry budget.
//! - Formation A's rally is exhausted — peer formation B preempts its
//!   own intent to support A.
//! - Formation A dissolves with reason "fuel exhausted" — neighboring
//!   formations on the same connector graph back off.
//!
//! Not used for request/reply (rally requests stay on the existing
//! transport channel) — gossip is one-way soft-state per Quickwit's
//! cluster model.
//!
//! ## Security
//!
//! Trust zone: **Z4 (cooperation-internal)** for in-process; **Z5
//! (transport)** when chitchat-backed. The bus carries
//! aggregate-only state (counts, intent variant, tier) — no agent
//! identities, no task contents — so a byzantine peer publishing
//! false views can mislead another formation's decisions but not
//! exfiltrate data. Subscriber filtering by `FormationId`
//! (excluding self) prevents trivial self-amplification loops.
//! Lagged subscribers drop silently per the broadcast-channel
//! convention; the bus never blocks producers. See
//! [`docs/intended-arch/COOPERATION_SECURITY_REVIEW.md §gossip`](../../../docs/intended-arch/COOPERATION_SECURITY_REVIEW.md)
//! for the full attacker-capability mapping.

pub mod bus;
pub mod trait_;
pub mod types;

pub use bus::InMemoryFormationGossipBus;
pub use trait_::FormationGossipBus;
pub use types::{FormationDelta, FormationOutcome, FormationStatus, FormationView};
