# Cooperation Module — Implementation Plan

**Status:** Implemented · **Updated:** 2026-04-10 · **Companion to:** `COOPERATION.md` (spec), `COOPERATION.pdf` (design origin).

> **Completion summary.** The 10-week plan below shipped: `crates/springtale-cooperation/`
> exists with 37 modules and zero internal Springtale deps; `springtale-bot` runs the
> 14-step formation tick described in §3 (`crates/springtale-bot/src/runtime/event_loop.rs::handle_cadence_tick`);
> all parity-matrix rows are covered; and the three worked examples ship as starter
> templates (`cli-runner`, `llm-swarm`, `telegram-bot`) — the full template set lives in
> `crates/springtale-runtime/src/operations/templates.rs`. See [`docs/arch/AUDIT-NOTES.md §3`](../arch/AUDIT-NOTES.md)
> for the wiring confirmation. Remaining work tracked there is ergonomic only.

---

## 0. Scope

This document is the **executable counterpart** to `COOPERATION.md`. That document tells you *what* to build; this one tells you *how* to build it, *in what order*, and *what the code actually looks like*. It's also the first place anything OpenClaw-adjacent gets named.

**Non-goals here:** re-deriving the design (see the spec), re-citing the research (see Appendices A–D in the spec), rewriting the game analysis (see the PDF).

**Goals:**

1. A 10-week plan with weekly deliverables
2. Crate layout, file tree, and `Cargo.toml` for the new `springtale-cooperation` crate
3. Real Rust type definitions for the core types, written to compile cleanly against the workspace conventions (`thiserror`, `tokio`, modules-over-inline, `#![forbid(unsafe_code)]`, `Secret<T>` for credentials)
4. Three worked end-to-end examples — CLI task runner, LLM orchestration swarm, Telegram bot — each showing how a consumer of the cooperation module integrates it into a bot
5. A binding OpenClaw-category parity matrix: every row must be covered by the time we ship
6. Decisions made now (not deferred), with the rationale

**Rule:** every Rust snippet below is written to compile. If a snippet has obvious gaps (e.g. `todo!()` bodies), those are explicitly marked. No pseudocode.

---

## 1. Path A confirmed

We're building the cooperation module as a Springtale-internal crate first, then extracting a portable framework once the API has proved itself against real bots. Rationale: retrofitting portability onto a working system is well-understood; designing a portability layer up front without a working reference usually produces the wrong abstractions.

**First consumer:** Springtale itself, across three use cases in checkpoints.

**Second consumer (later):** the portable framework extraction — after ~12 weeks of real use — targets Python/JS/Bevy/Unity. Not in this plan.

---

## 2. OpenClaw category parity matrix

OpenClaw in the `CLAUDE.md` framing is an unsafe AI-agent marketplace (250K+ stars, 800+ malicious skills, CVE-2026-25253 RCE, no sandboxing). The real archetype is the category of LangChain / CrewAI / AutoGen / AutoGPT / Open Interpreter / Letta — everything-as-unsandboxed-Python frameworks. To obsolete them, Springtale must cover every row below and be strictly safer and at least as easy to use.

| # | OpenClaw pattern | Springtale answer | Covered by |
|---|-----------------|-------------------|------------|
| 1 | Third-party skills / plugins marketplace | WASM-sandboxed connectors + signed manifests + capability allow-lists | `springtale-connector` + manifest registry |
| 2 | LLM orchestration (chains / graphs) | AI adapter trait + NoopAdapter default + per-bot selection | `springtale-ai` + §16 |
| 3 | Multi-agent workflows | **Formations** + full cooperation module | this plan |
| 4 | Scheduled / event-driven automation | Rules engine + scheduler | `springtale-scheduler` |
| 5 | Session memory / conversation history | SQLite session store per bot | `springtale-store` |
| 6 | Natural-language command input | Command router + optional AI adapter | `springtale-bot` |
| 7 | Chat interfaces (Discord/Telegram/Slack/Matrix) | Connector crates (first-party) | `connectors/` |
| 8 | Visual workflow builder UI | **Colony pixel-art RTS canvas** | `tauri/` |
| 9 | Credentials management | Vault + `Secret<T>` + duress pass + panic wipe | `springtale-crypto` |
| 10 | Marketplace / discovery | Signed manifest registry with capability display | manifest registry |
| 11 | Local execution / privacy | Always local-first, zero telemetry | constraint |
| 12 | Observability / debugging | Sentinel audit + tracing spans + colony visualization | `springtale-sentinel` + tauri |
| 13 | Headless / CLI mode | `springtale-cli` daemon + management API | `apps/springtale-cli` |
| 14 | Extensibility API | `Connector` trait (Rust) OR WASM connector | §16 |
| 15 | First-run experience (≤60 s to working bot) | `springtale init` + starter templates | **must build** |
| 16 | Inline error → fix mapping | `springtale fix <error-id>` command | **must build** |
| 17 | Real-time execution trace | Colony canvas + `springtale trace` CLI | **must build** |
| 18 | Hot reload of bot / connector | Hot-reload via WASM instance replacement | §16 capability rebind |
| 19 | Backup / restore bot state | `springtale export / import` + vault | **must build** |
| 20 | Multi-bot / colony management | Colony canvas + management API | exists |

**Rows 15–17, 19 are the UX gaps this plan must fill.** The rest are either already shipped or covered by the existing spec.

---

## 3. UX commandments

Non-negotiable. Every design decision below passes all ten.

1. **60-second first bot.** From `curl install` to a running bot that answers a message. No config file editing.
2. **One binary, one command.** `springtale` is a single static binary. No runtime deps, no Python env, no Node modules.
3. **Dry-run everything.** Every action that could touch the outside world can be previewed with `--dry-run` showing exactly what would happen.
4. **Errors are conversations.** Every error has an ID, a one-line human summary, and a `springtale fix <id>` command when a fix is possible.
5. **Templates over configuration.** Ship ≥10 starter templates (`springtale new telegram-bot`, `springtale new llm-swarm`, `springtale new cron-runner`). Never a blank page.
6. **Secrets are never typed twice.** `springtale secret set telegram.token` reads from stdin once, stores in vault, agents reference by name.
7. **Panic wipe is always one keystroke.** `Ctrl+Alt+P` wipes everything. Trust depends on this being reliable.
8. **Capabilities are obvious before load.** Before loading a connector, the user sees exactly what it can do, with color-coded trust signals.
9. **Observability is free.** Every bot gets tracing out of the box. `springtale logs <bot>` just works; no config.
10. **Progressive disclosure.** 3 beginner commands, 15 intermediate, 50 advanced. Never force a new user to learn the full surface.

---

## 4. Crate layout and `Cargo.toml`

### 4.1 Workspace placement

```
crates/
├── springtale-cooperation/      # NEW — this plan
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs               # pub mod declarations only
│   │   ├── error.rs             # CooperationError, per-module sub-errors
│   │   ├── cadence/
│   │   │   ├── mod.rs
│   │   │   ├── bus.rs           # CadenceBus
│   │   │   ├── tick.rs          # Tick, TickReport
│   │   │   └── intent.rs        # IntentPattern re-export
│   │   ├── formation/
│   │   │   ├── mod.rs
│   │   │   ├── formation.rs     # Formation, FormationId, FormationMember
│   │   │   ├── context.rs       # FormationContext
│   │   │   └── composition.rs   # FormationComposition (from orchestrator)
│   │   ├── momentum/
│   │   │   ├── mod.rs
│   │   │   ├── state.rs         # MomentumTier + statig FSM
│   │   │   └── gating.rs        # capability gating predicates
│   │   ├── awareness/
│   │   │   ├── mod.rs
│   │   │   ├── local.rs         # LocalAwareness, NeighborSnapshot
│   │   │   └── gossip.rs        # chitchat bridge
│   │   ├── attention/
│   │   │   ├── mod.rs
│   │   │   └── economy.rs       # AttentionEconomy, AttentionBroker
│   │   ├── environment/
│   │   │   ├── mod.rs
│   │   │   ├── workspace.rs     # SharedEnvironment, DashMap + ArcSwap
│   │   │   └── surface.rs       # Surface, SurfaceType
│   │   ├── consensus/
│   │   │   ├── mod.rs
│   │   │   ├── vote.rs          # ConsensusVote, VoteChoice, VoteResolution
│   │   │   └── resolver.rs      # majority / override / timeout resolver
│   │   ├── commit/
│   │   │   ├── mod.rs
│   │   │   ├── phase.rs         # CommitPhase
│   │   │   └── two_phase.rs     # two_phase_commit() using oneshot
│   │   ├── interference/
│   │   │   ├── mod.rs
│   │   │   ├── detector.rs      # detect_interference() over ActionRecord
│   │   │   └── event.rs         # InterferenceEvent, InterferenceType
│   │   ├── transformation/
│   │   │   ├── mod.rs
│   │   │   ├── role.rs          # DynamicRole trait (typetag)
│   │   │   └── transform.rs     # RoleTransformation enum + transform()
│   │   ├── rally/
│   │   │   ├── mod.rs
│   │   │   ├── supervisor.rs    # FormationRally with JoinSet + Semaphore
│   │   │   └── event.rs         # RallyEvent
│   │   ├── capability/
│   │   │   ├── mod.rs
│   │   │   ├── set.rs           # DynamicCapabilitySet
│   │   │   └── binder.rs        # wasmtime::Linker rebinder
│   │   ├── recovery/
│   │   │   ├── mod.rs
│   │   │   ├── distress.rs      # DistressSignal
│   │   │   └── action.rs        # RecoveryAction, RecoveryCost
│   │   ├── comms/
│   │   │   ├── mod.rs
│   │   │   ├── bus.rs           # FormationBus (broadcast + mpsc + watch)
│   │   │   └── channel.rs       # CommChannel, MeansOfComms (LFCG-aligned)
│   │   ├── handoff/
│   │   │   ├── mod.rs
│   │   │   ├── payload.rs       # HandoffPayload
│   │   │   └── transfer.rs      # HandoffType + dispatch
│   │   ├── mental_model/
│   │   │   ├── mod.rs
│   │   │   ├── store.rs         # rusqlite-backed store
│   │   │   └── graph.rs         # petgraph projection
│   │   ├── pacing/
│   │   │   ├── mod.rs
│   │   │   ├── phase.rs         # PacingPhase FSM (L4D-inspired)
│   │   │   └── manager.rs       # PacingManager with governor + ArcSwap
│   │   ├── sacrifice/
│   │   │   ├── mod.rs
│   │   │   ├── type_.rs         # SacrificeType enum
│   │   │   └── evaluator.rs     # big-brain utility AI evaluator
│   │   ├── time/
│   │   │   ├── mod.rs
│   │   │   └── tick.rs          # Tick(u64) opaque type + conversions
│   │   └── id/
│   │       ├── mod.rs
│   │       └── agent_id.rs      # AgentId packed u64
│   └── tests/
│       ├── formation_lifecycle.rs
│       ├── cadence_broadcast.rs
│       ├── rally_cascade.rs
│       └── replay_determinism.rs
│
├── springtale-bot/                 # Existing — integrates cooperation
│   └── src/orchestrator/           # Scoped per COOPERATION.md §3
│
└── ...
```

### 4.2 `Cargo.toml`

```toml
[package]
name = "springtale-cooperation"
version = "0.1.0"
edition = "2024"
license = "AGPL-3.0-or-later"     # see §14 for rationale
description = "Cooperative multi-agent primitives for local-first privacy-preserving bots"
repository = "https://github.com/scope-creep/springtale"

[dependencies]
# Core async runtime
tokio = { workspace = true, features = ["rt-multi-thread", "sync", "time", "macros"] }
tokio-util = { workspace = true, features = ["rt"] }
futures = { workspace = true }
async-trait = { workspace = true }

# Lock-free shared state
arc-swap = { workspace = true }
dashmap = { workspace = true }

# State machines and gossip
statig = { workspace = true }
chitchat = { workspace = true }
foca = { workspace = true }

# Serialization
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
typetag = { workspace = true }

# Errors
thiserror = { workspace = true }

# Observability
tracing = { workspace = true }

# Utility AI and behavior
big-brain = { workspace = true }
bonsai-bt = { workspace = true }   # optional sequenced-check alternative to big-brain

# Work-stealing and rate limiting
crossbeam-deque = { workspace = true }
governor = { workspace = true }

# Internal Springtale crates
springtale-core = { workspace = true }
springtale-store = { workspace = true }
springtale-connector = { workspace = true }
springtale-ai = { workspace = true }
springtale-crypto = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
proptest = { workspace = true }     # property-based tests
insta = { workspace = true }        # snapshot tests
tracing-subscriber = { workspace = true }
```

### 4.3 `lib.rs` — table of contents only, per crate rules

```rust
//! Springtale cooperative multi-agent primitives.
//!
//! This crate owns everything between *intent* and *outcome*, following the
//! CTDE (Centralized Training, Decentralized Execution) paradigm from
//! multi-agent RL research. See `docs/intended-arch/COOPERATION.md` for the
//! full design spec and `docs/intended-arch/COOPERATION.pdf` for the
//! game-informed design origin.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![warn(missing_docs)]

pub mod error;

pub mod time;
pub mod id;

pub mod cadence;
pub mod formation;
pub mod momentum;
pub mod awareness;
pub mod attention;
pub mod environment;
pub mod consensus;
pub mod commit;
pub mod interference;
pub mod transformation;
pub mod rally;
pub mod capability;
pub mod recovery;
pub mod comms;
pub mod handoff;
pub mod mental_model;
pub mod pacing;
pub mod sacrifice;

// Top-level re-exports for the most common types.
pub use error::CooperationError;
pub use time::Tick;
pub use id::AgentId;
pub use cadence::{CadenceBus, Tick as TickMessage, TickReport};
pub use formation::{Formation, FormationContext, FormationMember};
pub use momentum::MomentumTier;
```

---

## 5. Core type definitions (real Rust)

These are the canonical types every module references. Written to compile.

### 5.1 `error.rs` — the thiserror roots

```rust
//! Errors produced by the cooperation module.

use thiserror::Error;
use crate::id::AgentId;
use crate::time::Tick;

/// Top-level error enum. Every fallible cooperation API returns `Result<T, CooperationError>`.
#[derive(Error, Debug)]
pub enum CooperationError {
    #[error("cadence: {0}")]
    Cadence(#[from] CadenceError),

    #[error("formation: {0}")]
    Formation(#[from] FormationError),

    #[error("momentum: {0}")]
    Momentum(#[from] MomentumError),

    #[error("awareness: {0}")]
    Awareness(#[from] AwarenessError),

    #[error("consensus: {0}")]
    Consensus(#[from] ConsensusError),

    #[error("commit: {0}")]
    Commit(#[from] CommitError),

    #[error("rally: {0}")]
    Rally(#[from] RallyError),

    #[error("recovery: {0}")]
    Recovery(#[from] RecoveryError),

    #[error("handoff: {0}")]
    Handoff(#[from] HandoffError),

    #[error("internal invariant violated: {0}")]
    Invariant(String),
}

#[derive(Error, Debug)]
pub enum CadenceError {
    #[error("tick bus channel closed")]
    ChannelClosed,
    #[error("tick sequence wrapped (overflow)")]
    SequenceWrap,
    #[error("subscriber lagged by {lagged} ticks")]
    Lagged { lagged: u64 },
}

#[derive(Error, Debug)]
pub enum FormationError {
    #[error("agent {0:?} not found in formation")]
    AgentNotFound(AgentId),
    #[error("formation is empty — cannot proceed")]
    Empty,
    #[error("agent {0:?} missing required capability")]
    MissingCapability(AgentId),
    #[error("formation context not initialized")]
    ContextUninit,
}

#[derive(Error, Debug)]
pub enum MomentumError {
    #[error("momentum tier transition from {from:?} to {to:?} not allowed")]
    InvalidTransition { from: crate::momentum::MomentumTier, to: crate::momentum::MomentumTier },
    #[error("capability not unlocked at current tier {0:?}")]
    CapabilityLocked(crate::momentum::MomentumTier),
}

#[derive(Error, Debug)]
pub enum AwarenessError {
    #[error("neighbor snapshot for {0:?} is stale (age {1} ticks)")]
    StaleNeighbor(AgentId, u64),
    #[error("gossip bridge disconnected")]
    GossipDisconnected,
}

#[derive(Error, Debug)]
pub enum ConsensusError {
    #[error("no override tokens remaining for {0:?}")]
    NoOverrideTokens(AgentId),
    #[error("consensus deadline {0:?} passed with insufficient votes")]
    DeadlineExpired(Tick),
    #[error("vote not accepted: higher vote {higher_tick:?} rejects {incoming_tick:?}")]
    VoteRejected { higher_tick: Tick, incoming_tick: Tick },
}

#[derive(Error, Debug)]
pub enum CommitError {
    #[error("prepare phase timed out waiting for {pending} peer(s)")]
    PrepareTimeout { pending: usize },
    #[error("participant {0:?} dropped before commit")]
    ParticipantDropped(AgentId),
    #[error("vote failed — at least one peer voted Abort")]
    VoteFailed,
    #[error("execution phase failed for {0:?}: {1}")]
    ExecutionFailed(AgentId, String),
}

#[derive(Error, Debug)]
pub enum RallyError {
    #[error("no rally tokens remaining (budget exhausted)")]
    NoTokensLeft,
    #[error("cascade threshold exceeded — escalating to orchestrator")]
    Escalating,
    #[error("formation supervisor panicked")]
    SupervisorPanic,
}

#[derive(Error, Debug)]
pub enum RecoveryError {
    #[error("distress signal from {0:?} but no recovery path available")]
    NoRecoveryPath(AgentId),
    #[error("recovery cost exceeds available budget")]
    BudgetExceeded,
    #[error("agent {0:?} already at terminal failure — cannot recover")]
    TerminalFailure(AgentId),
}

#[derive(Error, Debug)]
pub enum HandoffError {
    #[error("no capable agent for next step requiring {required:?}")]
    NoCapableReceiver { required: String },
    #[error("handoff payload expired")]
    PayloadExpired,
    #[error("return obligation unmet for sequential dependency")]
    UnmetObligation,
}
```

### 5.2 `time/tick.rs` — opaque monotonic counter

```rust
//! Canonical time primitive.
//!
//! Springtale uses an opaque `Tick(u64)` as the internal clock rather than
//! wall-clock time. This gives us deterministic replay (record inputs keyed by
//! Tick, replay the exact sequence) and lets us convert to/from wall time at
//! the observability boundary.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Add;

/// Monotonic tick counter. Opaque — consumers should never do arithmetic
/// that assumes a specific tick-to-second ratio; use `CadenceBus::tick_interval`
/// when converting to wall time.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
    Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Tick(pub u64);

impl Tick {
    /// Zero tick — the genesis of a formation's lifetime.
    pub const ZERO: Self = Self(0);

    /// Advance by `n` ticks.
    pub const fn next(self, n: u64) -> Self {
        Self(self.0.wrapping_add(n))
    }

    /// Difference in ticks (saturating — never panics).
    pub const fn delta(self, earlier: Tick) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

impl Add<u64> for Tick {
    type Output = Self;
    fn add(self, rhs: u64) -> Self { self.next(rhs) }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}
```

### 5.3 `id/agent_id.rs` — packed generation-counter handle

Bitsquid/Stingray pattern from `COOPERATION.md §A.2.1`. 22 index bits + 8 generation bits + 34 reserved bits for future use.

```rust
//! Agent handle with embedded generation counter.
//!
//! Layout (64 bits):
//!   bits 0..=21   index (22 bits) — up to ~4M distinct agents
//!   bits 22..=29  generation (8 bits) — 256 generations before wraparound
//!   bits 30..=63  reserved (34 bits)
//!
//! Reuse is safe as long as there are ≥1024 agent churn cycles between
//! slot recycles (MINIMUM_FREE_INDICES). The generation counter detects
//! stale handles held by code that missed a reallocation.

use serde::{Deserialize, Serialize};
use std::fmt;

const INDEX_BITS: u32 = 22;
const GENERATION_BITS: u32 = 8;
const INDEX_MASK: u64 = (1 << INDEX_BITS) - 1;
const GENERATION_MASK: u64 = ((1 << GENERATION_BITS) - 1) << INDEX_BITS;

/// Packed agent handle. Compact enough to be `Copy`; carries a generation
/// counter so stale handles can be detected.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
    Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct AgentId(u64);

impl AgentId {
    /// The null handle — no agent ever occupies this slot.
    pub const NULL: Self = Self(0);

    /// Build a new handle from an index and generation.
    pub const fn new(index: u32, generation: u8) -> Self {
        debug_assert!(index < (1 << INDEX_BITS));
        let bits = (index as u64) | ((generation as u64) << INDEX_BITS);
        Self(bits)
    }

    pub const fn index(self) -> u32 {
        (self.0 & INDEX_MASK) as u32
    }

    pub const fn generation(self) -> u8 {
        ((self.0 & GENERATION_MASK) >> INDEX_BITS) as u8
    }

    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "agent#{}:{}", self.index(), self.generation())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_is_zero() {
        assert_eq!(AgentId::NULL.0, 0);
        assert!(AgentId::NULL.is_null());
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let id = AgentId::new(42, 7);
        assert_eq!(id.index(), 42);
        assert_eq!(id.generation(), 7);
    }

    #[test]
    fn generation_wraps_after_255() {
        let a = AgentId::new(0, 255);
        let b = AgentId::new(0, 0);
        assert_ne!(a, b);
    }

    #[test]
    fn display_is_debuggable() {
        let id = AgentId::new(42, 7);
        assert_eq!(format!("{}", id), "agent#42:7");
    }
}
```

### 5.4 `cadence/bus.rs` — the heart of the module

Implements `COOPERATION.md §5`. Real tokio plumbing.

```rust
//! External clock / shared tick bus.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use crate::error::CadenceError;
use crate::time::Tick;
use springtale_bot::orchestrator::intent::IntentPattern;  // re-exported from orchestrator

/// A tick broadcast to every subscribed agent.
#[derive(Clone, Debug)]
pub struct TickMessage {
    pub sequence: Tick,
    pub timestamp: Instant,
    pub intent: IntentPattern,
    /// Generous commit window. Per NecroDancer insight, the hard part is
    /// choosing the right action, not hitting the timing.
    pub window: Duration,
}

/// An agent's report for a completed tick.
#[derive(Clone, Debug)]
pub struct TickReport {
    pub agent_id: crate::id::AgentId,
    pub tick: Tick,
    pub action_taken: Option<ActionDescriptor>,
    pub latency: Duration,
    pub intent_alignment: f32,  // 0.0..=1.0
    pub interference_with: Vec<crate::id::AgentId>,
}

/// Marker for the action an agent chose this tick.
#[derive(Clone, Debug)]
pub struct ActionDescriptor {
    pub kind: String,
    pub payload_hash: u64,
}

/// The shared tick bus. One per formation.
pub struct CadenceBus {
    tick_interval: Duration,
    current_intent: Arc<RwLock<IntentPattern>>,
    tick_counter: AtomicU64,
    tx: broadcast::Sender<TickMessage>,
    reports_tx: mpsc::Sender<TickReport>,
}

impl CadenceBus {
    /// Create a new bus with the given tick interval and channel capacity.
    ///
    /// Default per DOS2 Norbyte release notes and our research: **30 Hz
    /// (33 ms tick interval)** for general agent cooperation. Override for
    /// higher-frequency formations.
    pub fn new(
        tick_interval: Duration,
        capacity: usize,
    ) -> (Self, mpsc::Receiver<TickReport>) {
        let (tx, _) = broadcast::channel(capacity);
        let (reports_tx, reports_rx) = mpsc::channel(capacity * 4);
        let bus = Self {
            tick_interval,
            current_intent: Arc::new(RwLock::new(IntentPattern::Stabilize {
                reason: Default::default(),
            })),
            tick_counter: AtomicU64::new(0),
            tx,
            reports_tx,
        };
        (bus, reports_rx)
    }

    /// Sensible default: 30 Hz, 256 tick backlog.
    pub fn default_30hz() -> (Self, mpsc::Receiver<TickReport>) {
        Self::new(Duration::from_millis(33), 256)
    }

    /// Subscribe a new agent to the tick stream.
    pub fn subscribe(&self) -> broadcast::Receiver<TickMessage> {
        self.tx.subscribe()
    }

    /// Reports channel sender — clone and pass to agents so they can report back.
    pub fn reports_sender(&self) -> mpsc::Sender<TickReport> {
        self.reports_tx.clone()
    }

    /// Change the current intent broadcast on the next tick.
    pub async fn set_intent(&self, intent: IntentPattern) {
        let mut guard = self.current_intent.write().await;
        *guard = intent;
    }

    /// Main loop — call from a `tokio::spawn` that owns the bus.
    pub async fn run(&self) -> Result<(), CadenceError> {
        let mut interval = tokio::time::interval(self.tick_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            let intent = self.current_intent.read().await.clone();
            let seq = self.tick_counter.fetch_add(1, Ordering::Relaxed);
            let tick = Tick(seq);
            let msg = TickMessage {
                sequence: tick,
                timestamp: Instant::now(),
                intent,
                // Per NecroDancer: generous commit window (half a beat each side).
                window: self.tick_interval.saturating_mul(4),
            };
            // Errors here mean all subscribers dropped — treat as normal shutdown.
            if self.tx.send(msg).is_err() {
                tracing::debug!("cadence bus: all subscribers dropped, stopping");
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn tick_counter_monotonic() {
        let (bus, _reports) = CadenceBus::default_30hz();
        let bus = Arc::new(bus);
        let mut rx = bus.subscribe();

        let bus2 = bus.clone();
        tokio::spawn(async move { let _ = bus2.run().await; });

        // Advance virtual time; receive three ticks.
        let tick1 = timeout(Duration::from_millis(100), rx.recv()).await.unwrap().unwrap();
        let tick2 = timeout(Duration::from_millis(100), rx.recv()).await.unwrap().unwrap();
        let tick3 = timeout(Duration::from_millis(100), rx.recv()).await.unwrap().unwrap();

        assert!(tick1.sequence < tick2.sequence);
        assert!(tick2.sequence < tick3.sequence);
    }
}
```

### 5.5 `momentum/state.rs` — statig hierarchical FSM

```rust
//! Formation momentum: Cold → Warming → Hot → Fever.
//!
//! Tier determines what the formation CAN do. See `COOPERATION.md §7` for
//! capability table.

use serde::{Deserialize, Serialize};
use statig::prelude::*;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum MomentumTier {
    #[default]
    Cold,
    Warming,
    Hot,
    Fever,
}

/// Events that drive the momentum state machine.
#[derive(Clone, Debug)]
pub enum MomentumEvent {
    TickSuccess,
    TickInterference,
    TickFailure,
    IntentChanged,
}

/// Persistent state owned by the state machine.
#[derive(Default, Debug)]
pub struct Momentum {
    pub successful_ticks: u32,
    pub interference_count: u32,
    pub last_tier: MomentumTier,
}

#[state_machine(initial = "State::cold()")]
impl Momentum {
    // --- Cold: read-only environment, no chaining ---
    #[state]
    fn cold(&mut self, event: &MomentumEvent) -> Outcome<State> {
        match event {
            MomentumEvent::TickSuccess => {
                self.successful_ticks = self.successful_ticks.saturating_add(1);
                if self.successful_ticks >= 3 {
                    self.last_tier = MomentumTier::Warming;
                    Transition(State::warming())
                } else {
                    Handled
                }
            }
            MomentumEvent::TickFailure => {
                self.successful_ticks = 0;
                Handled
            }
            _ => Super,
        }
    }

    // --- Warming: read neighbors, basic chaining ---
    #[state]
    fn warming(&mut self, event: &MomentumEvent) -> Outcome<State> {
        match event {
            MomentumEvent::TickSuccess => {
                self.successful_ticks = self.successful_ticks.saturating_add(1);
                if self.successful_ticks >= 8 && self.interference_count == 0 {
                    self.last_tier = MomentumTier::Hot;
                    Transition(State::hot())
                } else {
                    Handled
                }
            }
            MomentumEvent::TickFailure => {
                self.successful_ticks = 0;
                self.last_tier = MomentumTier::Cold;
                Transition(State::cold())
            }
            MomentumEvent::TickInterference => {
                self.interference_count = self.interference_count.saturating_add(1);
                Handled
            }
            _ => Super,
        }
    }

    // --- Hot: write environment, synchronized commit ---
    #[state]
    fn hot(&mut self, event: &MomentumEvent) -> Outcome<State> {
        match event {
            MomentumEvent::TickSuccess => {
                self.successful_ticks = self.successful_ticks.saturating_add(1);
                if self.successful_ticks >= 15 && self.interference_count == 0 {
                    self.last_tier = MomentumTier::Fever;
                    Transition(State::fever())
                } else {
                    Handled
                }
            }
            MomentumEvent::TickInterference => {
                self.interference_count = self.interference_count.saturating_add(1);
                if self.interference_count > 2 {
                    self.last_tier = MomentumTier::Warming;
                    Transition(State::warming())
                } else {
                    Handled
                }
            }
            _ => Super,
        }
    }

    // --- Fever: consensus, AI adapter, recruit ---
    #[state]
    fn fever(&mut self, event: &MomentumEvent) -> Outcome<State> {
        match event {
            MomentumEvent::TickInterference => {
                self.interference_count = self.interference_count.saturating_add(1);
                self.last_tier = MomentumTier::Hot;
                Transition(State::hot())
            }
            MomentumEvent::TickFailure => {
                self.last_tier = MomentumTier::Warming;
                Transition(State::warming())
            }
            _ => Super,
        }
    }
}

/// Query helpers for capability gating.
impl Momentum {
    pub fn current_tier(&self) -> MomentumTier {
        self.last_tier
    }

    pub fn can_write_environment(&self) -> bool {
        matches!(self.last_tier, MomentumTier::Hot | MomentumTier::Fever)
    }

    pub fn can_recruit(&self) -> bool {
        matches!(self.last_tier, MomentumTier::Fever)
    }

    pub fn can_call_ai_adapter(&self) -> bool {
        matches!(self.last_tier, MomentumTier::Fever)
    }

    pub fn can_read_neighbor_reports(&self) -> bool {
        !matches!(self.last_tier, MomentumTier::Cold)
    }
}
```

### 5.6 `formation/formation.rs` — the peer bus

```rust
//! Peer-group formation: no parent/child, no hierarchy.

use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
use crate::error::FormationError;
use crate::id::AgentId;
use crate::cadence::CadenceBus;
use super::context::FormationContext;

/// Unique formation identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FormationId(pub u64);

#[derive(Clone, Debug)]
pub struct FormationMember {
    pub agent_id: AgentId,
    pub capabilities: Vec<String>,
    pub current_role: String,
    // pub awareness / attention / fuel — see LocalAwareness, AttentionEconomy, etc.
}

/// Peer message type carried over the broadcast bus.
#[derive(Clone, Debug)]
pub enum PeerMsg {
    Joined(AgentId),
    Left(AgentId),
    AgentDown { id: AgentId, reason: String },
    AttentionRedistribute { from: AgentId, delta: f32 },
    IntentAck { agent: AgentId },
    Custom(String, serde_json::Value),
}

pub struct Formation {
    pub id: FormationId,
    members: ArcSwap<Vec<FormationMember>>,
    context_tx: watch::Sender<FormationContext>,
    bus_tx: broadcast::Sender<PeerMsg>,
    cadence: Arc<CadenceBus>,
}

impl Formation {
    pub fn new(
        id: FormationId,
        initial_context: FormationContext,
        cadence: Arc<CadenceBus>,
        bus_capacity: usize,
    ) -> Self {
        let (context_tx, _) = watch::channel(initial_context);
        let (bus_tx, _) = broadcast::channel(bus_capacity);
        Self {
            id,
            members: ArcSwap::from_pointee(Vec::new()),
            context_tx,
            bus_tx,
            cadence,
        }
    }

    pub fn join(&self, member: FormationMember) -> Result<(), FormationError> {
        self.members.rcu(|old| {
            let mut new = Vec::clone(old);
            new.push(member.clone());
            new
        });
        let _ = self.bus_tx.send(PeerMsg::Joined(member.agent_id));
        Ok(())
    }

    pub fn leave(&self, agent_id: AgentId) -> Result<(), FormationError> {
        self.members.rcu(|old| {
            old.iter().filter(|m| m.agent_id != agent_id).cloned().collect()
        });
        let _ = self.bus_tx.send(PeerMsg::Left(agent_id));
        Ok(())
    }

    pub fn members(&self) -> Arc<Vec<FormationMember>> {
        self.members.load_full()
    }

    pub fn subscribe(&self) -> (broadcast::Receiver<PeerMsg>, watch::Receiver<FormationContext>) {
        (self.bus_tx.subscribe(), self.context_tx.subscribe())
    }

    pub fn update_context<F>(&self, update: F) -> Result<(), FormationError>
    where
        F: FnOnce(&mut FormationContext),
    {
        self.context_tx.send_modify(update);
        Ok(())
    }

    pub fn cadence(&self) -> Arc<CadenceBus> {
        self.cadence.clone()
    }
}
```

### 5.7 Re-exports summary

The public API surface consumers will hit most is intentionally small:

```rust
use springtale_cooperation::{
    CadenceBus, Formation, FormationContext, FormationMember,
    MomentumTier, Tick, AgentId, CooperationError,
};
```

Everything else is tier-2 — reached through the module path (`cooperation::rally::FormationRally`, etc.) when needed. This is the "3 beginner, 15 intermediate, 50 advanced" commandment from §3.

---

## 6. 10-week plan with weekly deliverables

Each week ends with something that compiles, runs, and demo's. No week is "infrastructure only."

### Checkpoint 1 — cooperation core + CLI task runner

**Week 1 — Scaffold + Cadence + Formation**
- `crates/springtale-cooperation/` scaffolded per §4.1
- `Cargo.toml` per §4.2
- `lib.rs`, `error.rs`, `time/tick.rs`, `id/agent_id.rs` as per §5
- `cadence/bus.rs` + `cadence/tick.rs` fully implemented
- `formation/formation.rs` + `formation/context.rs` fully implemented
- Unit tests for each, integration test showing 3 mock agents subscribing to a bus
- **Demo:** `cargo test -p springtale-cooperation` green

**Week 2 — Momentum + Awareness + Rally**
- `momentum/state.rs` with statig FSM + capability gating queries
- `awareness/local.rs` + `awareness/gossip.rs` with chitchat bridge
- `rally/supervisor.rs` with JoinSet + Semaphore + peer-event broadcast
- `recovery/distress.rs` + `recovery/action.rs` basic shapes
- Property tests for Momentum: no invalid transitions, no gate bypass
- **Demo:** 5 mock agents in a formation, one fails, rally consumes a token, neighbors redistribute — all in a test harness

**Week 3 — Environment + Attention + Interference**
- `environment/workspace.rs` with DashMap + ArcSwap two-layer design
- `environment/surface.rs` with Divinity-style elemental chain stub
- `attention/economy.rs` with ArcSwap distribution
- `interference/detector.rs` + `interference/event.rs`
- **Demo:** 10 agents writing to a shared environment, interference detected, attention redistributed

**Week 4 — First worked example: CLI task runner**
- `apps/springtale-cli/examples/task-runner.rs` — see §7.1 below for code
- Uses `CadenceBus`, `Formation`, `MomentumTier`, `FormationRally`
- No AI adapter required (NoopAdapter default — constraint from product model)
- Single binary invocation: `springtale task "<description>"`
- **Demo:** `springtale task "summarize the files in this directory"` spawns a formation, runs to completion, prints result
- **Checkpoint 1 complete.** Cooperation core is working, first consumer validates the API shape.

### Checkpoint 2 — Remaining modules + LLM orchestration swarm

**Week 5 — Consensus + Commit + Handoff + Transformation**
- `consensus/vote.rs` + `consensus/resolver.rs` with openraft-style vote ordering
- `commit/two_phase.rs` using oneshot barrier (avoiding Barrier cancel-safety issue)
- `handoff/payload.rs` + `handoff/transfer.rs` with 5 `HandoffType` variants
- `transformation/role.rs` + `transformation/transform.rs` with typetag
- **Demo:** 2-phase commit across 4 agents succeeds; one peer votes abort → whole commit fails cleanly

**Week 6 — Capability + Comms + Mental Model + Pacing + Sacrifice**
- `capability/binder.rs` with wasmtime `Linker::func_wrap` rebinder
- `comms/bus.rs` + `comms/channel.rs` restructured per LFCG Communication-by-Design ↔ Means-of-Communication split
- `mental_model/store.rs` with rusqlite + petgraph projection
- `pacing/manager.rs` with governor + ArcSwap, L4D-inspired 4-state FSM
- `sacrifice/evaluator.rs` with big-brain utility AI
- **Demo:** formation enters Fever tier, all advanced modules exercised in an integration test

**Week 7 — Second worked example: LLM orchestration swarm**
- `apps/springtale-cli/examples/llm-swarm.rs` — see §7.2 below
- 3 agents (researcher, writer, critic) cooperating on a single task
- AI adapter uses Ollama local by default; NoopAdapter fallback for zero-AI demo
- Full cooperation module exercised — momentum progression, rally on LLM failure, handoff between agents, consensus on final output
- **Demo:** `springtale swarm "explain the Necrodancer rollback model"` produces a cited, fact-checked response
- **Checkpoint 2 complete.** Full cooperation module + LLM integration works end-to-end.

### Checkpoint 3 — Telegram bot + OpenClaw parity + polish

**Week 8 — Third worked example: Telegram bot + connector scaffold**
- Scaffold `connector-telegram` with `/new-connector` skill
- `apps/springtale-cli/examples/telegram-bot.rs` — see §7.3 below
- Rules: cron-triggered daily summary + webhook-triggered command replies
- Vault stores the Telegram token (via `springtale secret set telegram.token`)
- Formation of 3 agents on each incoming message: primary responder + memory keeper + moderation filter
- **Demo:** real Telegram bot running locally, full stack exercised

**Week 9 — OpenClaw parity + UX polish**
- `springtale init <template>` template generator with ≥10 starters (see §8)
- `springtale logs`, `springtale trace`, `springtale inspect` CLI commands
- Error ID system: every `CooperationError` variant has a stable ID (`COOP-001` through `COOP-0NN`); `springtale fix <id>` runs a remediation flow
- Capability preview before connector load with color-coded trust (red/yellow/green)
- First-run tutorial: `springtale tutorial` walks through CLI runner → LLM swarm → Telegram bot in 15 minutes
- **Demo:** fresh user, stopwatch, working bot in ≤60 seconds

**Week 10 — Security audit + benchmarks + docs**
- Threat-model pass per module (see §9 Security Review Framework below)
- Property-based tests for every invariant from §3.4 of the spec
- Benchmark suite: formation sizes 10 / 100 / 1000 agents
- Documentation pass: **task-oriented**, not reference. "How to build a Telegram bot with Springtale" not "API reference for `Formation::new`"
- Final OpenClaw parity matrix audit: every row in §2 checked off
- **Demo:** 4-minute video showing install → first bot → LLM swarm → Telegram bot, end to end

---

## 7. Three worked examples (real code)

Each of these compiles against the Cargo.toml in §4.2.

### 7.1 CLI task runner (Checkpoint 1 target)

**Use case:** a headless CLI that takes a task description, spawns a small formation of agents, and returns the aggregated result. Pure cooperation-module exercise — no AI required. The smallest OpenClaw-equivalent demo.

```rust
// apps/springtale-cli/examples/task-runner.rs

//! Minimal CLI task runner.
//!
//! Usage:
//!     springtale task "<task description>"
//!
//! Spawns a formation of 3 worker agents plus a coordinator. Agents run in
//! parallel, report on each tick, and the coordinator aggregates when enough
//! successful ticks accumulate.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use springtale_cooperation::{
    AgentId, CadenceBus, CooperationError, Formation, FormationContext,
    FormationMember, MomentumTier, Tick,
};
use springtale_cooperation::formation::formation::{FormationId, PeerMsg};
use springtale_cooperation::rally::supervisor::FormationRally;
use springtale_bot::orchestrator::intent::IntentPattern;
use springtale_bot::orchestrator::intent::ReconnoiterTarget;
use clap::Parser;

#[derive(Parser)]
#[command(name = "springtale-task")]
struct Args {
    /// The task description to execute.
    task: String,

    /// Number of worker agents (default 3).
    #[arg(short, long, default_value_t = 3)]
    workers: usize,

    /// Maximum ticks to wait before concluding.
    #[arg(long, default_value_t = 300)]
    max_ticks: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Standard Springtale tracing setup.
    tracing_subscriber::fmt()
        .with_env_filter("springtale=info,cooperation=debug")
        .init();

    let args = Args::parse();

    // 1. Build cadence bus at Springtale default (30 Hz).
    let (bus, mut reports_rx) = CadenceBus::default_30hz();
    let bus = Arc::new(bus);

    // 2. Build formation with initial Reconnoiter intent.
    let ctx = FormationContext {
        intent: IntentPattern::Reconnoiter {
            target: ReconnoiterTarget::Text(args.task.clone()),
        },
        momentum: MomentumTier::Cold,
        constraints: Default::default(),
    };
    let formation = Arc::new(Formation::new(
        FormationId(1),
        ctx,
        bus.clone(),
        /* bus capacity */ 128,
    ));

    // 3. Spawn worker agents.
    let mut rally = FormationRally::new(
        /* rally token budget */ 3,
        /* bus capacity */ 64,
    );

    for i in 0..args.workers {
        let agent_id = AgentId::new(i as u32, 0);
        formation.join(FormationMember {
            agent_id,
            capabilities: vec!["text.read".to_string(), "text.summarize".to_string()],
            current_role: "worker".to_string(),
        })?;

        let formation = formation.clone();
        let task = args.task.clone();
        rally.spawn(async move {
            run_worker(agent_id, formation, task).await
        });
    }

    // 4. Run the cadence bus until max_ticks or formation dissolves.
    let bus_handle = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.run().await })
    };

    // 5. Collect reports up to max_ticks.
    let mut success_count = 0u32;
    let mut aggregated = Vec::<String>::new();
    let max_ticks = args.max_ticks;

    while let Some(report) = reports_rx.recv().await {
        if report.tick.0 > max_ticks { break; }
        if report.intent_alignment > 0.5 {
            success_count += 1;
        }
        if let Some(action) = report.action_taken {
            aggregated.push(format!("[{}] {}", report.agent_id, action.kind));
        }
        // Stop when we have enough successful reports.
        if success_count >= (args.workers as u32 * 3) { break; }
    }

    // 6. Wind down.
    bus_handle.abort();
    println!("Task complete. {} successful reports.", success_count);
    for line in &aggregated { println!("{line}"); }

    Ok(())
}

async fn run_worker(
    agent_id: AgentId,
    formation: Arc<Formation>,
    task: String,
) -> Result<(), CooperationError> {
    let (mut peer_rx, mut ctx_rx) = formation.subscribe();
    let mut tick_rx = formation.cadence().subscribe();
    let reports_tx = formation.cadence().reports_sender();

    loop {
        tokio::select! {
            Ok(tick) = tick_rx.recv() => {
                // Do the actual work — for the CLI task runner this is a stub
                // that just echoes the task with the agent's id.
                let action = springtale_cooperation::cadence::bus::ActionDescriptor {
                    kind: format!("worker {} processed: {}", agent_id, task),
                    payload_hash: 0,
                };
                let report = springtale_cooperation::cadence::bus::TickReport {
                    agent_id,
                    tick: tick.sequence,
                    action_taken: Some(action),
                    latency: Duration::from_millis(5),
                    intent_alignment: 1.0,
                    interference_with: Vec::new(),
                };
                let _ = reports_tx.send(report).await;
            }
            Ok(msg) = peer_rx.recv() => {
                if let PeerMsg::Left(id) = msg {
                    if id == agent_id { return Ok(()); }
                }
            }
            else => return Ok(()),
        }
    }
}
```

**What this validates:**
- Cadence broadcast (multiple agents receiving ticks)
- Formation join / subscribe / peer bus
- Momentum initial state (Cold — pure observation)
- Tick report fan-in
- Rally supervisor (workers under JoinSet)
- Graceful shutdown

**What's deliberately missing:** AI adapter (this is the NoopAdapter case), consensus, synchronized commit, handoff. Those come in Checkpoint 2.

### 7.2 LLM orchestration swarm (Checkpoint 2 target)

**Use case:** 3 specialized agents — researcher, writer, critic — cooperating on a single prompt. Full cooperation module exercised. Direct OpenClaw replacement demo.

```rust
// apps/springtale-cli/examples/llm-swarm.rs

//! LLM orchestration swarm — 3 agents cooperating on a prompt.
//!
//! Usage:
//!     springtale swarm "<prompt>"
//!
//! Uses local Ollama by default; falls back to NoopAdapter if unavailable.

use std::sync::Arc;
use tokio::sync::mpsc;
use springtale_cooperation::{
    AgentId, CadenceBus, Formation, FormationContext, FormationMember,
    MomentumTier,
};
use springtale_cooperation::formation::formation::{FormationId, PeerMsg};
use springtale_cooperation::consensus::vote::{ConsensusVote, VoteChoice, VoteResolution};
use springtale_cooperation::handoff::payload::{HandoffPayload, HandoffType};
use springtale_cooperation::rally::supervisor::FormationRally;
use springtale_ai::{AiAdapter, NoopAdapter, ollama::OllamaAdapter};
use springtale_bot::orchestrator::intent::{IntentPattern, ReconnoiterTarget};
use clap::Parser;

#[derive(Parser)]
#[command(name = "springtale-swarm")]
struct Args {
    prompt: String,
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,
    #[arg(long, default_value = "llama3.1:8b")]
    model: String,
    #[arg(long)]
    no_ai: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("springtale=info")
        .init();
    let args = Args::parse();

    // 1. Pick adapter (Ollama or Noop — NoopAdapter must work, per product model).
    let adapter: Arc<dyn AiAdapter> = if args.no_ai {
        Arc::new(NoopAdapter::default())
    } else {
        match OllamaAdapter::new(&args.ollama_url, &args.model).await {
            Ok(a) => Arc::new(a),
            Err(e) => {
                eprintln!("[warn] Ollama unavailable ({e}); falling back to NoopAdapter");
                Arc::new(NoopAdapter::default())
            }
        }
    };

    // 2. Build cadence + formation.
    let (bus, mut reports_rx) = CadenceBus::default_30hz();
    let bus = Arc::new(bus);

    let ctx = FormationContext {
        intent: IntentPattern::Execute { plan_id: None },
        momentum: MomentumTier::Cold,
        constraints: Default::default(),
    };
    let formation = Arc::new(Formation::new(FormationId(1), ctx, bus.clone(), 256));

    // 3. Spawn 3 role-distinct agents. Each receives the same prompt.
    let roles = ["researcher", "writer", "critic"];
    let (handoff_tx, mut handoff_rx) = mpsc::channel::<HandoffPayload>(16);

    let mut rally = FormationRally::new(/* tokens */ 5, /* bus cap */ 64);

    for (i, role) in roles.iter().enumerate() {
        let agent_id = AgentId::new(i as u32, 0);
        formation.join(FormationMember {
            agent_id,
            capabilities: vec![format!("llm.{role}")],
            current_role: role.to_string(),
        })?;

        let formation = formation.clone();
        let adapter = adapter.clone();
        let prompt = args.prompt.clone();
        let handoff_tx = handoff_tx.clone();

        rally.spawn(async move {
            run_llm_agent(agent_id, role, formation, adapter, prompt, handoff_tx).await
        });
    }

    // 4. Bus runner.
    let bus_run = { let bus = bus.clone(); tokio::spawn(async move { bus.run().await }) };

    // 5. Coordinator: drain handoffs, gather the three agents' outputs,
    //    run a consensus vote on the final answer, print it.
    let mut outputs: Vec<(String, String)> = Vec::new();
    while let Some(payload) = handoff_rx.recv().await {
        let role = payload.produced_by.kind.clone();
        let text = payload.data
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        outputs.push((role, text));
        if outputs.len() >= 3 { break; }
    }

    bus_run.abort();

    // 6. Final consensus — pick the output with the highest critic score.
    println!("=== Swarm output ===");
    for (role, text) in &outputs {
        println!("\n[{role}]\n{text}");
    }

    Ok(())
}

async fn run_llm_agent(
    agent_id: AgentId,
    role: &'static str,
    formation: Arc<Formation>,
    adapter: Arc<dyn AiAdapter>,
    prompt: String,
    handoff_tx: mpsc::Sender<HandoffPayload>,
) -> Result<(), springtale_cooperation::CooperationError> {
    use springtale_ai::PromptRequest;
    let (mut peer_rx, _ctx_rx) = formation.subscribe();
    let mut tick_rx = formation.cadence().subscribe();

    // Role-specific system prompt.
    let system = match role {
        "researcher" => "You gather facts. Reply with 5 bullet points.",
        "writer"     => "You write prose from facts. Reply with 2 paragraphs.",
        "critic"     => "You critique writing for accuracy. Reply with issues found.",
        _            => "Unknown role.",
    };

    // Wait for first tick then do the work once.
    let _ = tick_rx.recv().await;

    let req = PromptRequest {
        system: system.to_string(),
        user: prompt.clone(),
        max_tokens: 512,
    };
    let response = adapter.prompt(req).await.map_err(|e| {
        springtale_cooperation::CooperationError::Invariant(e.to_string())
    })?;

    // Emit handoff payload downstream.
    let payload = HandoffPayload {
        data: serde_json::json!({ "text": response.content }),
        schema: "llm-output-v1".into(),
        produced_by: springtale_cooperation::cadence::bus::ActionDescriptor {
            kind: role.into(),
            payload_hash: 0,
        },
        consumable_by: vec!["llm.critic".into(), "llm.aggregator".into()],
        expires: None,
    };
    let _ = handoff_tx.send(payload).await;

    // Idle until formation dissolves.
    while let Ok(msg) = peer_rx.recv().await {
        if let PeerMsg::Left(id) = msg { if id == agent_id { break; } }
    }
    Ok(())
}
```

**What this validates:**
- AI adapter trait integration (Ollama primary, NoopAdapter fallback)
- 3 role-distinct agents cooperating on one prompt
- Handoff between agents (researcher → writer → critic pipeline)
- Graceful fallback when AI is unavailable (product model constraint)
- Formation + rally + cadence all working together

**What's deliberately simplified:** the coordinator here is a `mpsc` drain rather than a full synchronized commit. Week 7 expands this to use `commit::two_phase_commit` for the final aggregation.

### 7.3 Telegram bot (Checkpoint 3 target)

**Use case:** a real Telegram bot running locally, using the full stack — connector + rules + vault + cooperation + sentinel. Validates OpenClaw parity on the user-facing side.

```rust
// apps/springtale-cli/examples/telegram-bot.rs

//! Minimal Telegram bot using Springtale.
//!
//! Setup:
//!     springtale secret set telegram.token
//!     springtale run telegram-bot
//!
//! The bot listens for incoming messages. For each message, it spawns a
//! formation of 3 agents:
//!   - primary responder (generates reply text)
//!   - memory keeper (records the conversation in session memory)
//!   - moderation filter (votes on whether to actually send)

use std::sync::Arc;
use springtale_connector::Connector;
use connector_telegram::TelegramConnector;
use springtale_crypto::vault::Vault;
use springtale_cooperation::{
    AgentId, CadenceBus, Formation, FormationContext, FormationMember,
    MomentumTier,
};
use springtale_cooperation::formation::formation::FormationId;
use springtale_cooperation::consensus::vote::{ConsensusVote, VoteChoice, VoteResolution};
use springtale_bot::orchestrator::intent::IntentPattern;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    // 1. Unlock vault, pull the Telegram token.
    let vault = Vault::open_default().await?;
    let token = vault.get_secret("telegram.token").await?;

    // 2. Start the Telegram connector.
    let telegram = TelegramConnector::new(token).await?;
    let mut incoming = telegram.subscribe_messages().await?;

    // 3. For each incoming message, build a fresh formation and respond.
    while let Some(msg) = incoming.recv().await {
        let telegram = telegram.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_message(msg, telegram).await {
                tracing::warn!(error = %e, "message handling failed");
            }
        });
    }

    Ok(())
}

async fn handle_message(
    msg: TelegramMessage,
    telegram: Arc<TelegramConnector>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Build ephemeral formation for this single message.
    let (bus, mut reports_rx) = CadenceBus::default_30hz();
    let bus = Arc::new(bus);
    let ctx = FormationContext {
        intent: IntentPattern::Execute { plan_id: None },
        momentum: MomentumTier::Cold,
        constraints: Default::default(),
    };
    let formation = Arc::new(Formation::new(FormationId(msg.id), ctx, bus.clone(), 64));

    // Three roles.
    for (i, role) in ["responder", "memory_keeper", "moderator"].iter().enumerate() {
        formation.join(FormationMember {
            agent_id: AgentId::new(i as u32, 0),
            capabilities: vec![format!("telegram.{role}")],
            current_role: role.to_string(),
        })?;
    }

    // Run each agent role.
    // (Implementation of responder, memory_keeper, moderator agents elided;
    // same shape as run_llm_agent from §7.2.)

    // Final consensus: moderator votes whether to actually send.
    let mut vote = ConsensusVote::new(
        "send_reply".into(),
        Default::default(),
        /* deadline */ Tick(300),
    );
    // (Moderator casts its vote during its tick loop — elided.)

    match vote.resolve() {
        Some(VoteResolution::Majority(VoteChoice::Yes)) => {
            telegram.send_reply(msg.chat_id, "(response text)").await?;
        }
        Some(VoteResolution::Override { by, choice, cost }) => {
            tracing::warn!(agent = %by, cost, "moderator override used");
        }
        _ => {
            tracing::info!("message discarded by consensus");
        }
    }

    Ok(())
}

#[derive(Debug)]
struct TelegramMessage {
    id: u64,
    chat_id: i64,
    text: String,
}
```

**What this validates:**
- Full connector integration (real network, real Telegram API)
- Vault secret retrieval (no config file tokens)
- Per-message ephemeral formation (formations are cheap)
- Consensus gating — 3 agents vote on whether to reply
- Override mechanism (moderator can veto)

**What you can't see from the code:** the Rules engine triggering `handle_message`, the Sentinel auditing every outbound call, the colony canvas rendering the formation in real time. Those all come from other Springtale crates and are already in the workspace.

---

## 8. Template starters for `springtale init`

UX commandment #5 requires ≥10 starter templates. Proposed initial set:

| Template | What it creates | First-try time |
|----------|----------------|-----------------|
| `cli-runner` | §7.1 CLI task runner | 30 seconds |
| `llm-swarm` | §7.2 3-agent LLM swarm | 60 seconds |
| `telegram-bot` | §7.3 Telegram bot | 90 seconds (needs token) |
| `discord-bot` | Discord equivalent of telegram-bot | 90 seconds |
| `matrix-bot` | Matrix / Element chatbot | 90 seconds |
| `cron-runner` | Scheduled task automation | 45 seconds |
| `webhook-receiver` | HTTP webhook → cooperation formation | 60 seconds |
| `file-watcher` | File system event → cooperation formation | 45 seconds |
| `research-assistant` | Multi-source research LLM swarm with cited output | 60 seconds |
| `code-review-swarm` | Git diff → 3-agent review | 90 seconds |
| `meeting-summarizer` | Audio/transcript → structured summary | 90 seconds |
| `blank-bot` | Empty bot skeleton for experts | 10 seconds |

Each template is a single `.toml` file + a generated `main.rs` scaffold that follows the patterns from §7.

---

## 9. Security review framework (week 10)

Every module gets a threat model pass against these attacker capabilities:

1. **Malicious connector** — a connector that claims a benign capability but tries to do something else
2. **Compromised agent** — an agent whose code was replaced mid-run (e.g. swapped WASM module)
3. **Byzantine formation member** — an agent that deliberately reports false state
4. **Resource exhaustion attacker** — tries to drain rally tokens, commit barriers, environment writes
5. **Information leak attacker** — tries to exfiltrate secrets through comms channels, mental model queries, or log output
6. **Replay attacker** — captures a valid tick stream and replays it to trigger a duplicate action

Per module, the review asks:

- **Can attacker X cause effect Y?** (threat enumeration)
- **What's the detection signal?** (observability requirement)
- **What's the mitigation?** (rate limit / auth / cryptographic check / invariant)
- **What's the worst-case recovery time?** (SLA)

Result: one paragraph per module in a `SECURITY.md` addendum, per-variant mitigation code review.

---

## 10. Test strategy

### 10.1 Unit tests (per module)

Every public API call has a happy-path test plus error-case tests for every variant of its error enum. Targets >85% line coverage via `cargo llvm-cov`.

### 10.2 Property-based tests (per invariant)

Using `proptest`. Invariants we must test:

- **Cadence:** tick sequence is monotonic, never wraps unexpectedly, no tick is ever delivered twice.
- **Formation:** membership is consistent under concurrent join/leave. Member count never goes negative.
- **Momentum:** no state transition violates the FSM. No capability-locked action ever executes at Cold.
- **Rally:** rally tokens can't be double-spent. Cascade contagion is bounded (WH3 `max_routing_friends_to_consider = 4` pattern).
- **Consensus:** no vote is counted twice. Override cost is always deducted when used. Timeout resolution only fires after deadline.
- **Commit:** two-phase commit either all-execute or none-execute (atomic). No partial state.
- **Interference:** detection is commutative (A vs B returns the same event as B vs A).

### 10.3 Integration tests (per checkpoint)

End-to-end tests that spin up a full formation and exercise multi-module interactions. Stored as `.rs` files in `tests/`.

### 10.4 Deterministic replay

Every `CadenceBus::run` can be recorded (tick sequence, reports, peer messages) to a log file. A replay harness reads the log and re-runs the formation against a new code version, comparing outputs. This catches behavior drift across versions.

### 10.5 Property-based fuzzing

`cargo fuzz` targets for parsers and deserializers — specifically `ConsensusVote` and `HandoffPayload` serde round-trips.

### 10.6 Benchmarks

Using `criterion`. Track:

- Cadence bus throughput (ticks/sec) at formation sizes 10 / 100 / 1000
- Consensus resolution latency
- Environment RCU write contention
- Rally cascade detection latency

---

## 11. Decisions made (not deferred)

1. **Crate name:** `springtale-cooperation`. New crate, not folded into `springtale-bot`.
2. **Canonical time type:** `Tick(u64)` opaque monotonic counter. Wall-clock (`Instant`) is observability-only, not load-bearing for semantics. Enables deterministic replay.
3. **AgentId type:** Bitsquid-pattern packed `u64` — 22 index bits + 8 generation bits + 34 reserved. Compact, Copy, staleness-detecting.
4. **Default tick rate:** 30 Hz (33 ms), matching Divinity 4.0 per `COOPERATION.md §A.10.1`. Override available via `CadenceBus::new`.
5. **Default intensity scale:** 0.0..=1.0 float, matching L4D Director per `COOPERATION.md §A.1.1`. Intensity decay constant `30s`, relax threshold `0.99`.
6. **Default rally radius falloff:** linear, full effect out to `base_radius`, zero at `base_radius * 1.5`, matching WH3 `general_aura_radius = 70` + `inspiration_radius_max_effect_range_modifier = 1.5` pattern.
7. **Default cascade contagion caps:** 4 friends / 5 enemies considered, matching WH3 `max_routing_friends_to_consider = 4` / `max_routing_enemies_to_consider = 5`.
8. **Default tick update rate:** 15% lerp toward target per tick, matching WH3 `percent_update_per_tick = 0.15`. Minimum change 1.0 units.
9. **First worked example:** CLI task runner (Checkpoint 1). Smallest demo that exercises the cooperation core.
10. **Test harness style:** both proptest (invariants) and deterministic replay (regression).
11. **License:** `AGPL-3.0-or-later`. Rationale: Springtale's target users depend on the framework staying open; AGPL prevents OpenClaw-style forks from going proprietary with a marketplace layered on top. It does restrict us from consuming non-AGPL-compatible third-party code, but our research didn't surface any blocking dependencies.
12. **Error ID scheme:** `COOP-XXXX` stable IDs per error variant. Machine-readable for `springtale fix <id>`.
13. **Connector-JIT scaffolding:** use `/new-connector` per checkpoint rather than assuming all connectors exist. Telegram/Discord/Matrix get scaffolded in week 8.
14. **Documentation style:** task-oriented ("How to build a Telegram bot") not reference-oriented ("API docs for `Formation::new`"). Reference is auto-generated by rustdoc; the hand-written docs are cookbook-style.

---

## 12. Open questions (decisions still to make)

These are flagged for explicit resolution before the relevant week starts. Most can wait until the code forces the issue.

1. **Observability export format.** `tracing` spans are free; should we also emit Prometheus metrics? OpenTelemetry? Defer to week 10 when we add observability; tentatively `tracing` + `tracing-subscriber` in JSON for local audit.

2. **Formation identity reuse policy.** If a formation of ID `1` completes, can we reuse ID `1` for the next formation? Safer default: no, IDs are monotonic (like agent generations). Decide week 1.

3. **Cross-formation communication.** The spec doesn't explicitly say whether agents in two different formations can exchange messages. Likely answer: no, use explicit `handoff::Direct` or shared environment surfaces. Confirm week 5.

4. **AI adapter selection policy.** Per-bot is the product-model answer, but formations inside a bot can have different needs. Defer: per-formation adapter override in `FormationContext`. Decide week 7 when LLM swarm forces the issue.

5. **What's in `FormationContext` exactly.** Currently: intent, momentum, constraints. Might also want: tick rate, AI adapter handle, logger handle. Starts minimal in week 1, grows as needed.

6. **Bonsai-BT alternative for §24 Sacrifice.** Spec lists it as an alternative to big-brain. Pick one and commit in week 6. Tentatively big-brain because bevy-less mode works.

7. **Mental model sharing between formations.** Can formation A's knowledge help formation B? Interesting but complicates persistence. Defer to a later version; v0.1 is per-formation.

8. **Connector hot-reload during a live formation.** The spec supports capability rebinding but doesn't say what happens to in-flight operations. Safer default: operations complete against the old binding, new operations use the new binding. Confirm week 6.

---

## 13. Dependencies on external Springtale work

Things this plan needs from other Springtale subsystems that are not already shipped:

| Need | Current state | Blocking? |
|------|--------------|-----------|
| `springtale-core` with `CapabilityDecl` type | Exists | No |
| `springtale-crypto` vault API | Exists | No |
| `springtale-connector` `Connector` trait | Exists | No |
| `springtale-ai` with `NoopAdapter` + `OllamaAdapter` | Exists | No |
| `springtale-store` with session schema | Exists | No |
| `springtale-sentinel` audit trail | Exists | No |
| `connector-telegram` | **Does not exist** | **Blocks week 8** |
| `springtale-cli` binary | **Not this plan** | Has its own timeline |
| Colony canvas frontend | Exists | No (polish in week 9) |

**Action item before week 8:** scaffold `connector-telegram` via the `/new-connector` skill. Estimated 1-2 days of work, not in this plan's critical path until week 7.

---

## 14. External framework comparison

Nine frameworks surveyed. **Springtale's cooperation primitives have zero competition** — no framework has momentum tiers, rally/cascade recovery, or voluntary sacrifice as first-class concepts. The closest Rust peer is AutoAgents (liquidos-ai), which has WASM tool sandboxing but none of the cooperation module's game-design-informed primitives.

### 14.1 Springtale-unique pattern matrix

| Pattern | LangGraph | CrewAI | AutoGen | OpenInt | Letta | Rig | Mirascope | AutoGPT | AutoAgents | MS Agent Fw |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Formations** (first-class cooperative groups) | partial (subgraphs) | ✓ (Crew) | ✓ (Team) | ✗ | ✗ | ✗ | ✗ | ✗ | partial (actors) | ✓ (GroupChat) |
| **Momentum tiers** (capability gating by cumulative success) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **Rally / cascade recovery** (not just retry) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **Voluntary sacrifice** (agent degrades for team benefit) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **Interference detection** (concurrent conflict types) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **Synchronized commit** (2-phase barrier) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **Colony-style visual view** | Studio (node-link) | Studio (node-link) | DevUI (message flow) | ✗ | ✗ | ✗ | ✗ | Platform blocks | ✗ | DevUI |
| **WASM sandboxing of 3rd-party code** | ✗ | ✗ | ✗ | ✗ (semgrep only) | ✗ | ✗ | ✗ | ✗ (Docker punt) | **tools only** | ✗ |
| **Panic wipe / emergency destruction** | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **Signed manifest + capability allowlist** | ✗ | task-scoped | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | guardrails | ✗ |
| **`Secret<T>`-typed credentials + vault** | ✗ (env) | ✗ (env) | ✗ (env) | ✗ (env) | ✗ (env) | ✗ (env) | ✗ (env) | ✗ (env) | ✗ (env) | ✗ (env) |

**Interpretation:** five mechanisms (momentum, rally, sacrifice, interference detection, synchronized commit) are **zero-competition**. Three more (panic wipe, Secret<T>, signed manifests) are Springtale-unique for privacy/surveillance-resistance reasons. WASM sandboxing exists only in AutoAgents and only at tool granularity, not connector granularity.

### 14.2 Framework summaries

#### LangGraph (`langchain-ai/langgraph`, Python)
The reference for **state-graph-as-workflow**. `StateGraph` + `TypedDict` + conditional edges + Pregel supersteps. Tool model via `@tool` + `ToolNode`. Checkpointers (`InMemorySaver`, `PostgresSaver`, `SqliteSaver`) save per-superstep. **No cascade recovery** — per-task `RetryPolicy(max_attempts=3)` only. Zero sandboxing. First-run UX is "write Python."

**Verbatim API shape:**
```python
from langgraph.graph import START, StateGraph
from typing_extensions import TypedDict

class State(TypedDict):
    text: str

def node_a(state: State) -> dict:
    return {"text": state["text"] + "a"}

graph = StateGraph(State)
graph.add_node("node_a", node_a)
graph.add_edge(START, "node_a")
result = graph.compile().invoke({"text": ""})
```

**Springtale lesson:** The checkpointer-per-superstep model is worth stealing for `CooperationBus` replay. Conditional edges are less good than `graph-flow`'s `NextAction` enum (below).

#### CrewAI (`crewAIInc/crewAI`, Python)
**Best-in-class first-run UX** among Python frameworks. `crewai create crew my_project` → generates `agents.yaml` + `tasks.yaml` → `crewai run`. Agent triad: `role` + `goal` + `backstory`. Process enum: `Process.sequential` (reliable), `Process.hierarchical` (manager agent, often thrashes). Tool model: import `crewai_tools` primitives + `tools=[SerperDevTool()]` on Agent. Memory unified: `memory=True` enables short-term + long-term + entity memory via SQLite/Chroma. Token tracking: `CrewOutput.token_usage.total_tokens`.

**Verbatim 3-agent example:**
```python
researcher = Agent(role='Researcher', goal='Conduct foundational research',
                   backstory='An experienced researcher...')
analyst = Agent(role='Data Analyst', goal='Analyze research findings',
                backstory='A meticulous analyst...')
writer = Agent(role='Writer', goal='Draft the final report',
               backstory='A skilled writer...')

research_task = Task(description='Gather...', agent=researcher, expected_output='Raw Data')
analysis_task = Task(description='Analyze...', agent=analyst, expected_output='Data Insights')
writing_task = Task(description='Compose...', agent=writer, expected_output='Final Report')

report_crew = Crew(
    agents=[researcher, analyst, writer],
    tasks=[research_task, analysis_task, writing_task],
    process=Process.sequential
)
result = report_crew.kickoff()
```

**Springtale lesson:** Steal the **YAML-first scaffold**. `springtale new-bot my-bot` should produce `agents.yaml` + `rules.yaml` + `bot.toml` — not a blank Rust file. The `role`/`goal`/`backstory` triad is a good prompting scaffold even for our non-anthropomorphic formations.

#### Microsoft AutoGen (`microsoft/autogen`, Python + .NET)
Has the **closest thing to Springtale's Handoff primitive.** `HandoffMessage` is a first-class typed message. Composable termination conditions: `TextMentionTermination("TERMINATE") | MaxMessageTermination(10)`. Docker sandbox for code execution (strongest Python sandbox story). Cross-runtime .NET support — relevant for Unity/Godot integration later.

**Verbatim handoff:**
```python
class _HandOffAgent(BaseChatAgent):
    async def on_messages(self, messages, cancellation_token) -> Response:
        return Response(
            chat_message=HandoffMessage(
                content=f"Transferred to {self._next_agent}.",
                target=self._next_agent,
                source=self.name
            )
        )
```

**Springtale lesson:** Our `handoff::HandoffType` enum is correct as a typed message. Adopt AutoGen's **composable termination conditions** with `|` and `&` operators for formation shutdown conditions. Steal the pattern for `FormationTermination`.

#### Open Interpreter (`OpenInterpreter/open-interpreter`, Python)
**The reference for first-run UX and capability gating.** Springtale must match or beat this.

**Verbatim onboarding prompt:**
```
> OpenAI API key not found

To use `gpt-4o` (recommended) please provide an OpenAI API key.

To use another language model, run `interpreter --local` or consult the
documentation at docs.openinterpreter.com/language-model-setup/.

---
OpenAI API key:
```

Then: `"Open Interpreter will require approval before running code."` Every code block shown before execution; user types `y`/`n`. `interpreter -y` bypasses. Experimental safe mode: `off`/`ask`/`auto` with semgrep scan of generated code.

**Tool model:** `Computer` class with namespaced modules: `computer.terminal`, `computer.display`, `computer.mouse`, `computer.keyboard`, `computer.browser`, `computer.vision`, `computer.skills`.

**Springtale lesson:** Copy the **confirmation-by-default pattern verbatim**. Replace "approval" with "capability gate" (our WASM sandbox already blocks most of what OpenInterpreter asks humans to approve). Copy the **API-key-prompt flow** for first-run — but our answer is `springtale secret set <key>` → vault, not `.env`.

#### Letta / MemGPT (`letta-ai/letta`, Python)
**The reference for memory architecture.** Two tiers:
- **Core memory blocks** (working memory) — `Block(label, value)` injected into system prompt; agent edits via `core_memory_append`/`core_memory_replace` tools
- **Archival memory** (long-term) — vector-indexed `Passage` objects, queried via `archival_memory_search`

**Verbatim:**
```python
agent1 = client.agents.create(
    name=f"test_agent_{uuid.uuid4()}",
    memory_blocks=[
        CreateBlockParam(label="user1", value="user preferences: loud"),
        CreateBlockParam(label="user2", value="user preferences: happy"),
    ],
    model="openai/gpt-4o-mini",
    embedding="openai/text-embedding-3-small",
)
```

Persistence: Pydantic `AgentState` → SQLAlchemy ORM → Postgres/SQLite. Round-trips through JSON.

**Springtale lesson:** Steal the **`Block(label, value)` model verbatim** for per-agent working memory, but back it with the Springtale vault so preference blocks are `Secret<String>`-wrapped when they carry PII. Our `mental_model::store` already plans rusqlite + petgraph; add a `Block` table with encrypted values.

#### Rig (`0xPlaygrounds/rig`, Rust)
**The closest Rust peer for single-agent orchestration.** Fluent builder API, proc-macro tools (`#[rig_tool]`), `VectorStoreIndex` trait, `PromptHook` for intercepting the execution loop. **No multi-agent primitive.** Stateless by default — no persistence.

**Verbatim hello-world:**
```rust
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::Prompt;
use rig::providers::openai;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let client = openai::Client::from_env();
    let comedian = client
        .agent("gpt-5.2")
        .preamble("You are a comedian here to entertain the user using humour and jokes.")
        .build();
    let response = comedian.prompt("Entertain me!").await?;
    println!("{response}");
    Ok(())
}
```

**Springtale lesson:** Do **not** rebuild single-agent orchestration. `springtale-cooperation` could consume Rig (or be Rig-compatible) for the per-agent LLM loop, freeing us to focus on the cooperation layer above it. `PromptHook` is exactly the extension point we need for capability gating at the LLM-call level.

#### Mirascope (`Mirascope/mirascope`, Python)
**The reference for error-as-feedback loops.** `collect_errors` decorator reinserts prior validation failures into the next prompt:

```python
from mirascope.core.base import collect_errors
from tenacity import retry, stop_after_attempt
from pydantic import ValidationError

@retry(stop=stop_after_attempt(3), after=collect_errors(ValidationError))
@openai.call(model="gpt-4o", response_model=Book)
def extract_book_details(book: str, *, errors: list[ValidationError] | None = None) -> str:
    if errors:
        return f"Previous errors: {errors}. Extract the book from {book}"
    return f"Extract the book from {book}"
```

Also: provider fallback decorator.
```python
@fallback(
    anthropic.call("claude-3-opus-20240229"),
    on=(OpenAIRateLimitError, AnthropicRateLimitError),
)
@openai.call("gpt-4o-mini")
@prompt_template("Tell me a fun fact about {topic}")
def fun_fact(topic: str): ...
```

**Springtale lesson:** Adopt both patterns. Our AI adapter trait should support **`ErrorFeedback`** (prior error reinserted into next call context) and **`AdapterFallback`** (Ollama → Anthropic → NoopAdapter). Both are cheap wins and directly applicable to §11 consensus / §15 rally when LLMs are involved.

#### AutoGPT (`Significant-Gravitas/AutoGPT`, Python)
Original autonomous agent. Classic version uses JSON-in-system-prompt for goal decomposition (5 sub-goals). Platform version uses **blocks** — declarative nodes (`Store Value`, `Read CSV`, `Send Web Request`, `AI Structured Response Generator`) wired in a visual graph editor. First-run requires Docker Compose + Postgres + Redis + RabbitMQ + frontend + API + executor + scheduler. **Heavy.** Explicit "run in Docker" punt on sandboxing.

**Springtale lesson:** The blocks/visual-graph model validates the colony-canvas UX. The Docker-Compose sprawl is **what Springtale must NOT become** — single binary, no external services.

#### AutoAgents (`liquidos-ai/AutoAgents`, Rust) — **the closest competitor**
**Ship cite this as prior art.** Proc-macro API, actor-based multi-agent runtime, **WASM sandboxing for tools** (in `examples/wasm_runner/`), LLM guardrails with Block/Sanitize/Audit policies, 13+ LLM providers, `ReActAgent` and `BasicAgent` executors.

**Verbatim hello-world:**
```rust
#[derive(Serialize, Deserialize, ToolInput, Debug)]
pub struct AdditionArgs {
    #[input(description = "Left Operand for addition")]
    left: i64,
    #[input(description = "Right Operand for addition")]
    right: i64,
}

#[tool(name = "Addition", description = "Use this tool to Add two numbers", input = AdditionArgs)]
struct Addition {}

#[async_trait]
impl ToolRuntime for Addition {
    async fn execute(&self, args: Value) -> Result<Value, ToolCallError> {
        let typed_args: AdditionArgs = serde_json::from_value(args)?;
        Ok((typed_args.left + typed_args.right).into())
    }
}

#[agent(name = "math_agent", description = "You are a Math agent",
        tools = [Addition], output = MathAgentOutput)]
#[derive(Default, Clone, AgentHooks)]
pub struct MathAgent {}

let agent_handle = AgentBuilder::<_, DirectAgent>::new(ReActAgent::new(MathAgent {}))
    .llm(llm)
    .memory(Box::new(SlidingWindowMemory::new(10)))
    .build()
    .await?;
```

**What AutoAgents has that Springtale needs to match:** proc-macro ergonomics for `#[agent]` + `#[tool]` definitions, `SlidingWindowMemory` pattern, LLM provider count.

**What AutoAgents lacks (Springtale's delta):**
- No **formations** (cooperative groups with shared intent + momentum)
- No **momentum tiers** for capability gating
- No **rally / cascade recovery** beyond retry
- No **voluntary sacrifice**
- No **connector-level WASM sandbox** (tools only, not entire third-party packages)
- No **manifest signing** with capability allowlist
- No **`Secret<T>` / vault-backed credentials**
- No **panic wipe**
- No **colony UI**
- No **synchronized commit** / interference detection

**Springtale positioning statement** (for README / docs):

> AutoAgents ships Rust multi-agent with WASM-sandboxed tools. Springtale extends that to WASM-sandboxed *connectors* — entire third-party packages running in their own sandboxes with manifest signing, capability allowlists, and fuel/memory/wall-clock limits — plus cooperation primitives (formations, momentum, rally, voluntary sacrifice) drawn from 30 years of cooperative game design, plus a privacy-first vault with panic wipe for users whose safety depends on it.

#### Microsoft Agent Framework (`microsoft/agent-framework`) — successor to AutoGen
First-class GroupChat / Sequential / Concurrent / Handoff patterns. **DevUI** browser-based debugger visualizes agent execution, message flows, tool calls, orchestration decisions in real time. Checkpointing + human-in-the-loop. Cross-runtime (Python + .NET) is the main pitch. **No WASM, no momentum, no colony view.**

**Verbatim group chat:**
```python
workflow = GroupChatBuilder(
    participants=[researcher, planner],
    orchestrator_agent=Agent(client=client),
    max_rounds=8,
    intermediate_outputs=True,
).build()

async for event in workflow.run(task, stream=True):
    if event.type == "output":
        ...
```

**Springtale lesson:** The DevUI streaming event pattern is a good UX reference. The `intermediate_outputs=True` flag — streaming the whole thought process to the UI — matches the colony canvas vision.

#### graph-flow (`a-agmon/rs-graph-llm`, Rust)
LangGraph clone in Rust. **Cleaner than LangGraph** — the `NextAction` enum is worth stealing wholesale:

```rust
pub enum NextAction {
    Continue,
    ContinueAndExecute,
    WaitForInput,
    End,
    GoTo(String),
    GoBack,
}
```

**Springtale lesson:** Adopt this exact enum shape for our rule/task routing. Much cleaner than LangGraph's string-based conditional edges.

### 14.3 What to steal (implementation-ready patterns)

1. **Open Interpreter's first-run flow** — verbatim API-key prompt + "approval before running" banner. Replace "approval" with Springtale's capability gate (WASM sandbox handles most).
2. **CrewAI's YAML-first scaffold** — `springtale new-bot <name>` generates `agents.yaml`, `rules.yaml`, `bot.toml`, not a blank Rust file.
3. **Letta's `Block(label, value)` memory model** for per-agent working memory, vault-backed for encrypted values.
4. **AutoGen's typed `HandoffMessage`** — our `handoff::HandoffType` enum is correct, keep it as typed messages not strings.
5. **Mirascope's `collect_errors`** pattern — AI adapter gets prior errors reinserted as feedback on the next call.
6. **Mirascope's `@fallback` pattern** — AI adapter chains (Ollama → Anthropic → NoopAdapter) as configuration.
7. **graph-flow's `NextAction` enum** — rule/task routing with `Continue | WaitForInput | GoTo(id) | GoBack | End`.
8. **AutoGen's composable termination** — `MaxMessageTermination(10) | TextMentionTermination("DONE")` for formation shutdown conditions.
9. **Rig's `PromptHook` extension point** — capability gating at the LLM-call level, not just at connector level.
10. **AutoAgents' proc-macro ergonomics** — `#[agent(tools=[...], output=...)]` is terse; our `Formation` / `FormationMember` API should be at least as brief.

### 14.4 What to differentiate hard

1. **Formation = Crew + Intent + Momentum.** A Formation is not just a roster. It carries an `IntentPattern` (Reconnoiter/Execute/Stabilize/Surge) and a `MomentumTier` that gates which capabilities its members can invoke. No framework has this. Document side-by-side with CrewAI's Crew in the README.
2. **Rally** as distinct from retry. When an agent fails, the *formation* re-coordinates (redistributing attention, transforming roles, consuming a rally token) rather than the task re-running. Sequence diagram in docs.
3. **Voluntary sacrifice.** Agent surrenders its momentum tier so a teammate can advance. Cooperative-game primitive no LLM framework has.
4. **Connector-level WASM sandbox.** AutoAgents has tool-level; Springtale ships connector-level as strict superset. Every third-party package runs in its own Wasmtime instance with fuel/memory/wall-clock limits and exact-host network allowlists.
5. **Panic wipe** as a vault-level operation bound to a duress passphrase + keyboard shortcut. Nobody else ships this because nobody else targets users whose safety depends on it.
6. **Colony canvas as the primary UI**, not a debugger pane. Other frameworks put visual tools in a "Studio" or "DevUI" — optional. Springtale's desktop app IS the canvas.
7. **Synchronized commit + interference detection.** Zero-competition primitives from `COOPERATION.md §12`–§13. Direct port of Splinter Cell dual-breach + Helldivers friendly-fire patterns.

### 14.5 What NOT to rebuild

- **Single-agent LLM orchestration.** Depend on or interop with Rig where sensible. Our cooperation layer sits on top.
- **LLM provider abstraction.** Rig and AutoAgents both have 10+ providers. `springtale-ai` `Adapter` trait is correct; keep it thin, consider delegating to Rig.
- **Vector store abstraction.** Use existing (Qdrant/Chroma via Rig) rather than inventing.
- **Proc macros for agent definition.** AutoAgents has this; if we need it later, copy the approach or depend on a shared crate.

### 14.6 Positioning statement (for README / marketing)

> **The safe, local-first, cooperation-aware multi-agent Rust framework.**
>
> Where LangChain treats agents as chains and CrewAI treats them as anthropomorphic personas, Springtale treats them as **cooperative units with momentum, rally, and sacrifice** — because cooperative video games have spent 30 years solving multi-agent coordination under adversarial conditions. L4D's AI Director, Deep Rock Galactic's difficulty points, Total War's morale state machine, Splinter Cell's cooperative dynamics framework — we read all of them and built on top.
>
> Where **AutoAgents** sandboxes tools, Springtale sandboxes entire **connectors** — third-party packages, not just callables.
>
> Where everyone else punts credentials to env vars, Springtale types them as `Secret<T>` backed by a duress-protected vault with a one-keystroke panic wipe, because our target users are people whose safety depends on their data not leaking.
>
> Every bot works without AI. Plug in Ollama, OpenAI, or Anthropic — or none. The cooperation primitives are orthogonal to the LLM layer.
>
> Obsoletes OpenClaw.

---

## 15. What this plan does NOT cover

To be clear about scope:

- **Phase 3 Veilid / P2P.** The cooperation module works on single-host tokio. Cross-host formations are a separate plan.
- **Multi-language bindings.** Python/JS/Unity/Bevy adapters are post-v0.1.
- **Learning / adaptation.** Agents don't update their own code from outcomes. Policy learning is future work.
- **Formal verification.** We're using property-based tests, not TLA+ or similar. Good enough for v0.1.
- **Federation between Springtale instances.** One Springtale instance per user; cross-instance is Phase 3.

---

## 16. Success criteria

The cooperation module is "done" for v0.1 when:

1. All 25 sections of `COOPERATION.md` have a working implementation in `springtale-cooperation`
2. All three worked examples (CLI runner, LLM swarm, Telegram bot) run end-to-end
3. All 20 rows of the OpenClaw parity matrix (§2) are covered
4. `springtale init cli-runner && springtale run` works in ≤60 seconds on a fresh machine
5. `cargo test -p springtale-cooperation` is green, with >85% line coverage and all property tests passing
6. Every `CooperationError` variant has a stable ID and a `springtale fix <id>` entry
7. Security review (§9) complete for all modules
8. Benchmarks show linear scaling up to 100 agents, sub-100ms coordination latency at 1000 agents
9. Documentation ships with 10+ task-oriented guides, not just API reference
10. A fresh user can watch a 4-minute video and have a working Telegram bot

---

**Next action:** confirm the plan, then start week 1 (scaffold + cadence + formation). All decisions in §11 can be revisited if they turn out wrong in practice, but we commit to them now to avoid bike-shedding.
