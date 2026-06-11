# Cooperative Agent Architecture — Technical Companion

**Status:** Implemented (April 2026 — see `crates/springtale-cooperation/`) · **Phase:** 1b/2a · **Updated:** 2026-06-10
**Companion to:** `COOPERATION.pdf` (game-design spec) and `ARCHITECTURE.md §14`.

> The 41-module crate this document specifies is shipped and wired through the
> formation tick pipeline (`tick_steps/`, a superset of the original 14 steps).
> User-facing tour: [`docs/guide/cooperation.md`](../guide/cooperation.md).
> Wiring report: [`docs/arch/AUDIT-NOTES.md §3`](../arch/AUDIT-NOTES.md). As-built
> deviations and the June 2026 gap-closure pass are recorded in §25.1. The text
> below is preserved as the design rationale of record.

---

## What this document is

`COOPERATION.pdf` is the *design spec*. It establishes the game-informed model — why
cooperation lives between intent and outcome, which game mechanics map to which
module, what structs should exist. It is deliberately light on implementation: the
Rust sketches are shape-only, there are no crate choices, and no real-world code is
cited for how any of this actually gets built.

This document is the *technical companion*, in three layers:

**Layer 1 — Rust implementation (§§3–25).** For every mechanism the PDF proposes:

- The real Rust crate (with version) that implements the primitive
- A verbatim code snippet from that crate's source or docs
- A link to the source with line refs when possible
- Why it fits the cooperation use case
- Alternatives with honest tradeoffs
- Known gaps in the Rust ecosystem

**Layer 2 — Closed-game source verification (Appendix A).** For every game the PDF
cites by name:

- What primary sources (GDC talks, developer interviews, datamines, RE projects)
  actually document
- Real data structures, field names, and numeric values from public reverse-
  engineering and community datamines (e.g., Total War's morale FSM from the
  Warhammer 3 database schema, NecroDancer's `GameState` struct from ChoregraphAI,
  Monster Hunter's per-part HP from Kiranico, L4D `DirectorOptions` from decompiled
  mutation scripts)
- Verbatim developer quotes with URLs
- Honest gaps — claims in the PDF that primary sources do not support (notably:
  the "220 Sun Tzu rules" in Total War is marketing, not implementation)

**Layer 3 — Open-source reference implementations (Appendix B).** For every
mechanism the PDF attributes to a closed game, Appendix B names an open-source
alternative with readable code:

- Adaptive director? Payday 2's `GroupAIStateBesiege.lua` (verbatim state machine).
- Spawn budgets? DRG wiki tables + `trumank/drg-custom-difficulties` JSON schema.
- Morale systems? `0AD-Morale-System` JS mod with real lerp-to-target code.
- Elemental surfaces? Noita's `<Reaction/>` XML schema + Cataclysm DDA `field_type.json`.
- Threat tables? TrinityCore `ThreatManager.cpp` (1000ms tick, 110/130% hysteresis).
- Narrative branching? Inkle's Ink `VariablesState.cs` with one-shot visit-counted choices.
- Motion input parsers? OpenBOR `check_combo` (Konami-code state machine with priority).
- Stagger / posture? OpenMW `combat.cpp` knockdown rule (real C++).
- Rhythm hit windows? osu!'s `OsuHitWindows.cs` and StepMania's `TimingWindowSecondsInit`.

**Layer 4 — Academic anchor (Appendix C).** The Living Framework for Cooperative
Games (Pais et al., CHI 2024) is the most rigorous peer-reviewed cooperative-game
taxonomy currently published — 11 authors, 129 games, Template Analysis
methodology, CC BY 4.0. Appendix C maps every Springtale module section to LFCG's
vocabulary, shows which of our 14 reference games LFCG has already coded, lists
the predecessor chain (Zagal, Rocha, Harris, Björk, Toups, Reuter, El-Nasr), and
identifies §7 Momentum and §9 Attention Economy as Springtale's genuine novel
contributions — extensions that directly fill limitations LFCG's own §5.4 and §6
flag as open problems. Use LFCG vocabulary in the 10 strong-fit sections
identified in C.5 to anchor this document in the peer-reviewed lineage.

**Rule of thumb:** cite Appendix C for academic framing, Appendix B for
implementation code, and Appendix A for design provenance from the closed games
the PDF references by name.

Nothing in here is invented. If a snippet appears in a code block, it was copied
verbatim from upstream source at the version noted. Every quoted phrase has a URL.
Gaps are flagged explicitly rather than papered over with plausible-looking
pseudocode.

Read the PDF first for the "why". Read the main body (§§3–25) for the Rust "how".
Read Appendix A when you want to verify the PDF's game claims against what's
actually documented — or when you're designing a new module and want the real
field names from the source games.

---

## Table of Contents

1. [Design Thesis (recap)](#1-design-thesis-recap)
2. [Game Mechanic Inventory (recap)](#2-game-mechanic-inventory-recap)
3. [Orchestration Boundary — Concrete Types](#3-orchestration-boundary--concrete-types)
4. [Module Structure — Crate Layout](#4-module-structure--crate-layout)
5. [Cadence System — `tokio::sync::broadcast`](#5-cadence-system)
6. [Formation System — peer bus, no hierarchy](#6-formation-system)
7. [Momentum System — `statig` hierarchical states](#7-momentum-system)
8. [Awareness System — `chitchat` / `foca` gossip](#8-awareness-system)
9. [Attention Economy — `ArcSwap<Distribution>`](#9-attention-economy)
10. [Shared Environment — `dashmap` + `arc-swap` RCU](#10-shared-environment)
11. [Consensus Engine — `openraft` vote semantics](#11-consensus-engine)
12. [Synchronized Commit — barrier + oneshot 2PC](#12-synchronized-commit)
13. [Interference Detection — `sled::compare_and_swap`](#13-interference-detection)
14. [Role Transformation — `typetag` trait objects](#14-role-transformation)
15. [Rally & Cascade Recovery — `JoinSet` + `Semaphore`](#15-rally--cascade-recovery)
16. [Dynamic Capability Binding — `wasmtime::Linker`](#16-dynamic-capability-binding)
17. [Integration with Existing Architecture](#17-integration-with-existing-architecture)
18. [Recovery & Mutual Aid — distress patterns](#18-recovery--mutual-aid)
19. [Communication Protocols — channel matrix](#19-communication-protocols)
20. [Handoff & Transition — `crossbeam-deque` + `sled`](#20-handoff--transition)
21. [Shared Mental Model — `rusqlite` + `petgraph`](#21-shared-mental-model)
22. [Tempo & Pacing — `governor` + `ArcSwap`](#22-tempo--pacing)
23. [Specialization vs Generalization (principle)](#23-specialization-vs-generalization)
24. [Sacrifice & Covering — `big-brain` utility AI](#24-sacrifice--covering)
25. [Dependency Summary & Honest Gaps](#25-dependency-summary--honest-gaps)

**Appendix A — Game source verification** (primary-source research per closed game):
A.1 Left 4 Dead · A.2 Helldivers 2 · A.3 Army of Two · A.4 Total War ·
A.5 Patapon · A.6 Crypt of the NecroDancer · A.7 Monster Hunter ·
A.8 Deep Rock Galactic · A.9 Overcooked · A.10 Divinity: Original Sin 2 ·
A.11 Rainbow Six Siege · A.12 Splinter Cell · A.13 It Takes Two ·
A.14 As Dusk Falls · A.15 Academic papers

**Appendix B — Open-source reference implementations** (readable code for every mechanism):
B.1 Adaptive Director (L4D PDFs + Payday 2 + PaceMaker) ·
B.2 Swarm budgeting (DRG + KF2 + Minecraft + DayZ + Warframe) ·
B.3 Morale (0AD-Morale-System + Spring/BAR) ·
B.4 Elemental surfaces (Noita + CDDA + Powder Toy + DCSS) ·
B.5 Destruction (Nvidia Blast + Unreal Chaos + Teardown + voro++) ·
B.6 Per-part HP / posture (Kiranico + Sekiro datamines + OpenMW `combat.cpp`) ·
B.7 Beat clock + hit windows (osu! + StepMania + FNF + BMS/LR2) ·
B.8 Threat / aggro (TrinityCore `ThreatManager`) ·
B.9 Narrative branching (Ink + Yarn Spinner + ChoiceScript) ·
B.10 Motion input (OpenBOR + fighting game DFA) ·
B.11 Paired animations (UE Contextual Animation only — weak) ·
B.12 Mission pacing (DayZ + Vampire Survivors) ·
B.13 Swap table

**Appendix C — LFCG alignment** (Pais et al., CHI 2024 — academic anchor for the whole document):
C.1 Full taxonomy tree · C.2 Methodology · C.3 Corpus overlap with Springtale's 14 games ·
C.4 Predecessor lineage chain · C.5 Springtale § → LFCG axis mapping ·
C.6 Novel Springtale contributions beyond LFCG (§7 momentum, §9 attention) ·
C.7 Restructuring §19 around Communication-by-Design ↔ Means-of-Communication ·
C.8 Follow-up opportunities · C.9 What LFCG does NOT close · C.10 Recommendations

**Appendix D — Source verification summary** (provenance of every code block and quote)

---

## 1. Design Thesis (recap)

See `COOPERATION.pdf §1`. In one sentence: the existing `orchestrator/` module is
command-and-control (parent → child pipeline tree) and this document's `cooperation/`
module owns everything between intent and outcome, following the CTDE paradigm
(Centralized Training, Decentralized Execution) from multi-agent RL research.

What orchestration owns:
- **Composition** — who is in the formation (pre-mission)
- **Intent** — what the formation should accomplish (not how)
- **Constraints** — fuel budgets, guard mode, autonomy ceiling, destructive-action gates
- **Intervention** — recovery when cooperation itself breaks down

What cooperation owns:
- Task decomposition within intent
- Timing coordination
- Role adaptation
- Information fusion
- Failure recovery within the formation
- Resource allocation within constraints

Everything below is the implementation side of that split.

---

## 2. Game Mechanic Inventory (recap)

The PDF catalogs 14 games and extracts the mechanical patterns. This companion does
not duplicate that inventory — read §2 of the PDF. Every implementation section below
references the source games by name, so the game→mechanic mapping stays in the PDF
where it belongs, and the mechanic→code mapping stays here.

---

## 3. Orchestration Boundary — Concrete Types

The `orchestrator/` module is preserved and scoped. These are its four files with
concrete Rust types that should compile against the existing workspace conventions
(`thiserror` errors, `Secret<T>` for any credentials, modules-over-inline).

```
crates/springtale-bot/src/orchestrator/
├── mod.rs
├── composer.rs      — select which agents form a group (pre-mission)
├── intent.rs        — publish intent patterns to cadence bus
├── constraints.rs   — fuel budgets, guard mode, autonomy ceiling
└── intervention.rs  — rally on cascade failure, dissolve stuck formations
```

### 3.1 Composer — Pre-Mission Army Selection

Game source: Patapon army select, Total War recruitment, Siege operator pick, Deep
Rock class select.

```rust
// orchestrator/composer.rs

pub struct FormationComposition {
    pub formation_id: FormationId,
    pub members: Vec<AgentSlot>,
    pub intent: IntentPattern,
    pub constraints: FormationConstraints,
}

pub struct AgentSlot {
    pub agent_id: AgentId,
    pub capabilities: Vec<CapabilityDecl>,
    pub role_hint: Option<RoleHint>,  // suggestion, never mandate
}

#[derive(thiserror::Error, Debug)]
pub enum ComposeError {
    #[error("agent {0} not found")]
    AgentNotFound(AgentId),
    #[error("agent {0} missing required capability {1:?}")]
    MissingCapability(AgentId, CapabilityDecl),
    #[error("formation has no members")]
    Empty,
}
```

The `role_hint` is like equipping a Patapon with fire arrows — it biases behavior
without locking it. Per §23 of the PDF, agents must never be forced into a role.

### 3.2 Intent — The Drum Pattern

```rust
// orchestrator/intent.rs

pub enum IntentPattern {
    /// Gather information. Sensor agents activate.
    /// Patapon: PATA PATA PATA PON. Siege: Drone phase.
    Reconnoiter { target: TaskDescriptor },

    /// Execute against a known target.
    /// Patapon: PON PON PATA PON. Total War: Charge.
    Execute { plan_id: Option<PlanId> },

    /// Hold current state. Defensive agents activate.
    /// Patapon: CHAKA CHAKA PATA PON. Total War: Guard mode.
    Stabilize { reason: StabilizeReason },

    /// Maximum commitment to singular objective.
    /// Patapon: DON DON CHAKA CHAKA. Army of Two: Overkill.
    Surge { objective: TaskDescriptor },

    /// Graceful wind-down.
    Dissolve { reason: DissolveReason },
}
```

Intent is published to the cadence bus (§5) — one producer (orchestrator), many
consumers (formation members). The `broadcast::Sender<Tick>` carries `IntentPattern`
inside every tick.

### 3.3 Constraints — Guard Mode Toggles

```rust
// orchestrator/constraints.rs

pub struct FormationConstraints {
    pub fuel_budget: FuelBudget,
    pub timeout: Duration,                         // Siege round timer
    pub max_concurrent_actions: usize,
    pub destructive_action_policy: ApprovalPolicy, // always L1
    pub guard_mode: bool,                          // Total War: don't pursue
    pub autonomy_ceiling: AutonomyLevel,
}
```

These are enforced by the existing `sentinel` crate. Cooperation cannot weaken them.

### 3.4 Intervention — The General's Rally

```rust
// orchestrator/intervention.rs

pub enum Intervention {
    ChangeIntent(IntentPattern),      // Patapon: switch rhythm
    InjectFuel(FuelBudget),           // L4D: health kit spawn
    ForcedDissolve { reason: String },
    EscalateToUser { summary: ActionSummary },
}
```

Intervention is reactive and exceptional — only fires when formation self-rally (§15)
has exhausted its rally tokens.

---

## 4. Module Structure — Crate Layout

Following `crate-structure.md` rules: `lib.rs` declares modules, nothing else.

```
crates/springtale-bot/src/
├── orchestrator/          # SCOPED: composition, intent, constraints, intervention
│   ├── mod.rs
│   ├── composer.rs
│   ├── intent.rs
│   ├── constraints.rs
│   └── intervention.rs
│
├── cooperation/           # NEW: everything between intent and outcome
│   ├── mod.rs
│   ├── cadence.rs         # §5  — shared tick bus
│   ├── formation.rs       # §6  — peer agent grouping
│   ├── momentum.rs        # §7  — coherence accumulator / Fever
│   ├── awareness.rs       # §8  — local neighbor perception
│   ├── attention.rs       # §9  — workload economy / aggro
│   ├── environment.rs     # §10 — shared mutable workspace
│   ├── consensus.rs       # §11 — weighted decision resolution
│   ├── commit.rs          # §12 — synchronized execution barriers
│   ├── interference.rs    # §13 — conflict detection
│   ├── transformation.rs  # §14 — role change on capability loss
│   ├── rally.rs           # §15 — cascade recovery
│   ├── capability.rs      # §16 — dynamic capability binding
│   ├── recovery.rs        # §18 — distress detection, mutual aid
│   ├── comms.rs           # §19 — multi-layer communication
│   ├── handoff.rs         # §20 — work product transfer
│   ├── mental_model.rs    # §21 — shared context accumulation
│   ├── pacing.rs          # §22 — work/rest cycle management
│   └── sacrifice.rs       # §24 — deliberate self-cost for team benefit
```

### 4.1 lib.rs surface

```rust
// crates/springtale-bot/src/lib.rs
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod orchestrator;
pub mod cooperation;
// ... other modules
```

No functions. No types. No impl blocks. `cooperation/mod.rs` re-exports its
submodules' public API and declares a crate-level `CooperationError` enum using
`thiserror`.

### 4.2 Workspace dependency additions

New crates this module needs, to be pinned at workspace root `Cargo.toml`:

```toml
# Shared primitives (likely already present)
tokio = { version = "1.50", features = ["rt-multi-thread", "sync", "time", "macros"] }
tokio-util = { version = "0.7", features = ["rt"] }
arc-swap = "1.9"
dashmap = "6.1"

# New
statig = "0.4"         # §7   hierarchical state machines
chitchat = "0.10"      # §8   scuttlebutt gossip (primary)
foca = "1.0"           # §8   SWIM gossip (secondary for liveness)
crossbeam-deque = "0.8" # §20 work-stealing for FlexibleChain handoffs
governor = "0.6"       # §22 GCRA rate limiting
big-brain = "0.22"     # §24 utility AI scoring
typetag = "0.2"        # §14, §16 — serializable trait objects
petgraph = "0.8"       # §21 knowledge graph
```

`openraft` (§11) and `wasmtime` (§16) are already or intended to be in the tree per
`ARCHITECTURE.md`, so not listed again. `rusqlite` (§21) is already used by
`springtale-store`.

---

## 5. Cadence System

> **PDF section:** §5. Game sources: Necrodancer (external clock), Patapon (rhythm as
> intent), Overcooked (implicit timing).

### 5.1 Mechanism

```
      ┌──────────────────────┐
      │     Orchestrator     │
      │    owns IntentPattern│
      └──────────┬───────────┘
                 │ publishes IntentPattern
                 ▼
        ┌────────────────┐
        │  CadenceBus    │       ┌─────────┐
        │  (tokio broad- │──────▶│ Agent A │
        │   cast::Sender)│       └─────────┘
        │                │       ┌─────────┐
        │  Tick {        │──────▶│ Agent B │
        │    sequence,   │       └─────────┘
        │    timestamp,  │       ┌─────────┐
        │    intent,     │──────▶│ Agent C │
        │    window,     │       └─────────┘
        │  }             │
        └────────────────┘
```

The Necrodancer insight: neither player owns the beat. The music IS the clock. Ryan
Clark's 100% leeway discovery means the hard part is choosing the right action, not
hitting the timing. Agents should have generous commit windows.

### 5.2 Primary implementation — `tokio` 1.50

Two tokio primitives compose into the cadence bus:

- `tokio::time::interval(Duration)` — generates ticks at a fixed period
- `tokio::sync::broadcast::channel(cap)` — fans out to N subscribers with
  per-receiver cursors and explicit lag detection

Verbatim from `tokio-1.50.0/src/sync/broadcast.rs` lines 73–94 (module-level doctest
in tokio's CI):

```rust
use tokio::sync::broadcast;

let (tx, mut rx1) = broadcast::channel(16);
let mut rx2 = tx.subscribe();

tokio::spawn(async move {
    assert_eq!(rx1.recv().await.unwrap(), 10);
    assert_eq!(rx1.recv().await.unwrap(), 20);
});

tokio::spawn(async move {
    assert_eq!(rx2.recv().await.unwrap(), 10);
    assert_eq!(rx2.recv().await.unwrap(), 20);
});

tx.send(10).unwrap();
tx.send(20).unwrap();
```

Verbatim from `tokio-1.50.0/src/time/interval.rs` lines 62–66:

```rust
let mut interval = time::interval(time::Duration::from_secs(2));
for _i in 0..5 {
    interval.tick().await;
    task_that_takes_a_second().await;
}
```

The lag-handling contract (same file, lines 96–117) is load-bearing for the generous
commit window:

```rust
let (tx, mut rx) = broadcast::channel(2);
tx.send(10).unwrap();
tx.send(20).unwrap();
tx.send(30).unwrap();
// The receiver lagged behind
assert!(rx.recv().await.is_err()); // RecvError::Lagged(1)
// At this point, we can abort or continue with lost messages
assert_eq!(20, rx.recv().await.unwrap());
assert_eq!(30, rx.recv().await.unwrap());
```

Module docs: *"broadcast channels are susceptible to the 'slow receiver' problem… if
a value is sent when the channel is at capacity, the oldest value currently held by
the channel is released… any receiver that has not yet seen the released value will
return `RecvError::Lagged`."*

This is exactly the Necrodancer semantics: the bus never blocks fast agents waiting
on slow ones. A slow agent gets `Lagged(n)` as an explicit signal, which is its
commit-window-missed indicator.

Docs: <https://docs.rs/tokio/1.50.0/tokio/sync/broadcast/index.html>,
<https://docs.rs/tokio/1.50.0/tokio/time/fn.interval.html>

### 5.3 Concrete CadenceBus

```rust
// cooperation/cadence.rs
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{broadcast, RwLock, mpsc};

pub struct CadenceBus {
    tick_interval: Duration,
    current_intent: Arc<RwLock<IntentPattern>>,
    tick_counter: AtomicU64,
    tx: broadcast::Sender<Tick>,
    reports_tx: mpsc::Sender<TickReport>, // fan-in
}

#[derive(Clone, Debug)]
pub struct Tick {
    pub sequence: u64,
    pub timestamp: Instant,
    pub intent: IntentPattern,
    pub window: Duration, // generous, per Necrodancer insight
}

#[derive(Clone, Debug)]
pub struct TickReport {
    pub agent_id: AgentId,
    pub tick_sequence: u64,
    pub action_taken: Option<ActionDescriptor>,
    pub latency: Duration,
    pub intent_alignment: f32,
    pub interference_with: Vec<AgentId>,
}

impl CadenceBus {
    pub fn new(tick_interval: Duration, capacity: usize) -> (Self, mpsc::Receiver<TickReport>) {
        let (tx, _) = broadcast::channel(capacity);
        let (reports_tx, reports_rx) = mpsc::channel(capacity * 4);
        let bus = Self {
            tick_interval,
            current_intent: Arc::new(RwLock::new(IntentPattern::Stabilize {
                reason: StabilizeReason::Initial,
            })),
            tick_counter: AtomicU64::new(0),
            tx,
            reports_tx,
        };
        (bus, reports_rx)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Tick> {
        self.tx.subscribe()
    }

    pub async fn run(&self) {
        let mut interval = tokio::time::interval(self.tick_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let intent = self.current_intent.read().await.clone();
            let seq = self.tick_counter.fetch_add(1, Ordering::Relaxed);
            let _ = self.tx.send(Tick {
                sequence: seq,
                timestamp: Instant::now(),
                intent,
                window: self.tick_interval * 4, // generous
            });
        }
    }
}
```

### 5.4 Alternatives and tradeoffs

| Primitive | When to use | Tradeoff |
|-----------|-------------|----------|
| `tokio::sync::broadcast` (chosen) | Single global drumbeat to N peers | Lossy under slow consumer (detectable as `Lagged`) |
| `tokio_util::time::DelayQueue` | Heterogeneous per-agent deadlines | No single authoritative sequence |
| `async-broadcast` | Runtime-agnostic (not tokio) | Loses tokio integration |
| `bevy_ecs` `FixedUpdate` | Already in Bevy | Fights tokio for task ownership |

Do **not** build this on `mpsc` with manual cloning — you lose per-receiver cursors
and lag detection.

### 5.5 Intent transitions

Like Patapon switching rhythms mid-mission. Three sources:
1. `orchestrator::intent::publish` (explicit command)
2. Formation self-governance via consensus at Fever tier (§7, §11)
3. Momentum-gated access to new intent options

All three paths write to `CadenceBus::current_intent`, which is read into the next
tick.

---

## 6. Formation System

> **PDF section:** §6. Game sources: Deep Rock (interdependent classes), Monster
> Hunter (emergent roles), Siege (anchor/roamer).

### 6.1 Mechanism

Peer groups, not hierarchies. Deep Rock's team has no leader. Nobody assigns the
Scout to grapple up. Each class's capabilities intersect with the environment to
produce self-organized cooperation. Formations replace the parent→child pipeline.

```
    ┌──────────────────────────────────────────┐
    │              Formation                    │
    │                                            │
    │   ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐    │
    │   │ A   │  │ B   │  │ C   │  │ D   │    │  ← no parent
    │   └──┬──┘  └──┬──┘  └──┬──┘  └──┬──┘    │
    │      └────────┼────────┼────────┘         │
    │               │        │                   │
    │        ┌──────┴────────┴──────┐           │
    │        │   shared context     │           │
    │        │  (watch + broadcast) │           │
    │        └──────────────────────┘           │
    └──────────────────────────────────────────┘
```

### 6.2 Primary implementation — `tokio::sync::watch` + `broadcast` + `arc-swap`

The peer pattern composes three primitives:

1. `tokio::sync::broadcast::Sender<PeerMsg>` — the peer bus. Every agent sends to the
   same sender cloned from the formation. Every peer sees every message. Agents
   filter by sender_id.
2. `tokio::sync::watch::Sender<FormationContext>` — shared intent, momentum tier,
   current phase. Coordinator role (not hereditary) calls `tx.send(new_context)` and
   every peer sees it on next `.changed().await`.
3. `arc_swap::ArcSwap<Vec<PeerId>>` — roster membership, atomic swap, zero-lock reads
   so joining/leaving doesn't block live traffic.

Verbatim from `tokio-1.50.0/src/sync/watch.rs` lines 533–547:

```rust
let (tx, mut rx) = watch::channel("hello");

tokio::spawn(async move {
    // Use the equivalent of a "do-while" loop so the initial value is
    // processed before awaiting the `changed()` future.
    loop {
        println!("{}! ", *rx.borrow_and_update());
        if rx.changed().await.is_err() {
            break;
        }
    }
});

sleep(Duration::from_millis(100)).await;
tx.send("world")?;
```

Verbatim from `arc-swap-1.9.1/src/lib.rs` (constructor + load + swap at lines 386,
472, 485):

```rust
pub fn new(val: T) -> Self
pub fn load(&self) -> Guard<T, S>       // sync-free read path
pub fn swap(&self, new: T) -> T          // atomic replace
```

The arc-swap module docs (lines 8–35) explain the motivation: O(1) reads with zero
locks, strictly better than `RwLock<Arc<T>>` for read-dominated workloads.

### 6.3 Concrete Formation

```rust
// cooperation/formation.rs
use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

pub struct Formation {
    pub id: FormationId,
    members: ArcSwap<Vec<FormationMember>>,
    context: watch::Sender<FormationContext>,
    bus: broadcast::Sender<PeerMsg>,
    cadence: Arc<CadenceBus>,
}

#[derive(Clone)]
pub struct FormationContext {
    pub intent: IntentPattern,
    pub constraints: FormationConstraints,
    pub momentum: MomentumTier,
    pub phase: PacingPhase,
}

pub struct FormationMember {
    pub agent_id: AgentId,
    pub capabilities: Vec<CapabilityDecl>,
    pub current_role: DynamicRole,
    pub awareness: LocalAwareness,
    pub attention_load: f32,
    pub fuel_remaining: FuelBudget,
    pub health: AgentHealth,
}

impl Formation {
    pub fn join(&self, member: FormationMember) {
        self.members.rcu(|old| {
            let mut new = Vec::clone(old);
            new.push(member.clone());
            Arc::new(new)
        });
    }

    pub fn leave(&self, agent_id: AgentId) {
        self.members.rcu(|old| {
            let new: Vec<_> = old.iter().filter(|m| m.agent_id != agent_id).cloned().collect();
            Arc::new(new)
        });
    }

    pub fn subscribe(&self) -> (broadcast::Receiver<PeerMsg>, watch::Receiver<FormationContext>) {
        (self.bus.subscribe(), self.context.subscribe())
    }
}
```

### 6.4 Alternatives

- **`ractor` 0.15** — Erlang-style actor crate with `ActorRef`, supervision. Its
  supervision model is fundamentally parent-child; you'd fight it for peer-level
  rally. Use only if you want named actor lookup via its PID registry.
- **`actix` 0.13** — heavier, couples to Actix runtime. Not recommended for new
  designs in 2026.
- **`bastion`** — unmaintained since ~2021. Patterns are good reading but do not
  depend on it.
- **Raw `mpsc` mesh (N×N)** — don't. O(N²) channels and no dynamic membership.

**Tradeoff on watch:** `watch` drops intermediate values. If formation context
transitions are themselves meaningful events ("we went Cold→Warming→Hot in 50ms and
you need to know Warming happened"), emit phase-change events on `broadcast` in
parallel. Use `watch` only for current steady-state tier.

---

## 7. Momentum System

> **PDF section:** §7. Game sources: Patapon Fever, Total War veterans, Monster
> Hunter topple windows.
>
> **LFCG status: genuinely novel Springtale extension.** No LFCG axis models
> time-varying activation state. LFCG's own §5.4 Limitations and §6 Outlook
> explicitly flag *"capturing games as static artefacts, not as play sessions
> across time"* as an open problem. Springtale's Momentum tiers (Cold / Warming /
> Hot / Fever) are the direct answer: a colony-wide state machine that gates
> capabilities on cumulative successful-tick history. Cite Pais et al. §6 when
> justifying §7's novelty. See Appendix C.6.

### 7.1 Mechanism

Tiers, not scores. Patapon Fever doesn't make units 10% stronger — it unlocks attack
patterns that don't exist outside Fever. Momentum determines what agents CAN do.

| Capability | Cold | Warming | Hot | Fever |
|------------|:----:|:-------:|:---:|:-----:|
| Read shared environment | ✓ | ✓ | ✓ | ✓ |
| Read neighbor TickReports | — | ✓ | ✓ | ✓ |
| Chain: A's output feeds B | — | ✓ | ✓ | ✓ |
| Write shared environment | — | — | ✓ | ✓ |
| Synchronized commit | — | — | ✓ | ✓ |
| Formation consensus | — | — | — | ✓ |
| AI adapter calls | — | — | — | ✓ |
| Recruit additional agents | — | — | — | ✓ |

### 7.2 Primary implementation — `statig` 0.4

`statig` supports hierarchical state machines with **superstate inheritance** — a
handler defined on a superstate automatically applies to all nested states unless
overridden. This is exactly the capability-gating semantics needed: put Fever-only
handlers under a `#[superstate = "hot_or_fever"]` and they're unreachable from Cold.

Verbatim from the `statig` calculator example (`examples/macro/calculator/src/state.rs`,
lines 36–60 and 81–110, run as a doctest in the crate's CI):

```rust
#[state_machine(initial = "State::begin()")]
impl Calculator {
    #[action]
    fn enter_begin(&mut self) {
        self.display = "0".to_string();
    }

    #[state(superstate = "ready", entry_action = "enter_begin")]
    fn begin(&mut self, event: &Event) -> Outcome<State> {
        match event {
            Event::Operator { operator: Operator::Sub } => {
                self.display = "-0".to_string();
                Transition(State::negated1())
            }
            _ => Super,   // delegate up the superstate chain
        }
    }

    #[superstate(superstate = "on")]
    fn ready(&mut self, event: &Event) -> Outcome<State> {
        match event {
            Event::Ac => Transition(State::begin()),
            Event::Ce => Transition(State::begin()),
            _ => Super,
        }
    }
}
```

Basic shape from `examples/macro/basic/src/main.rs` lines 20–35:

```rust
#[state_machine(initial = "State::off()", state(derive(Debug)))]
impl Blinky {
    #[state]
    fn on(&mut self, event: &Event) -> Outcome<State> {
        self.led = false;
        Transition(State::off())
    }

    #[state]
    fn off(&mut self, event: &Event) -> Outcome<State> {
        self.led = true;
        Transition(State::on())
    }
}
```

Source: <https://github.com/mdeloof/statig>

### 7.3 Concrete momentum state machine

```rust
// cooperation/momentum.rs
use statig::prelude::*;

#[derive(Default)]
pub struct Momentum {
    successful_ticks: u32,
    interference_count: u32,
    last_intent: IntentPattern,
}

pub enum MomentumEvent {
    TickSuccess,
    TickInterference,
    TickFailure,
    IntentChanged(IntentPattern),
}

#[state_machine(initial = "State::cold()")]
impl Momentum {
    // Cold: read-only environment, no chaining
    #[state]
    fn cold(&mut self, event: &MomentumEvent) -> Outcome<State> {
        match event {
            MomentumEvent::TickSuccess => {
                self.successful_ticks += 1;
                if self.successful_ticks >= 3 { Transition(State::warming()) }
                else { Handled }
            }
            _ => Super,
        }
    }

    // Warming: can read neighbor reports, basic chaining
    #[state(superstate = "warm_or_above")]
    fn warming(&mut self, event: &MomentumEvent) -> Outcome<State> {
        match event {
            MomentumEvent::TickSuccess => {
                self.successful_ticks += 1;
                if self.successful_ticks >= 8 && self.interference_count == 0 {
                    Transition(State::hot())
                } else { Handled }
            }
            MomentumEvent::TickFailure => Transition(State::cold()),
            _ => Super,
        }
    }

    // Hot: write environment, complex chains, synchronized commit
    #[state(superstate = "hot_or_fever")]
    fn hot(&mut self, event: &MomentumEvent) -> Outcome<State> {
        match event {
            MomentumEvent::TickSuccess => {
                self.successful_ticks += 1;
                if self.successful_ticks >= 15 { Transition(State::fever()) }
                else { Handled }
            }
            MomentumEvent::TickInterference => {
                self.interference_count += 1;
                if self.interference_count > 2 { Transition(State::warming()) }
                else { Handled }
            }
            _ => Super,
        }
    }

    // Fever: consensus, AI adapter, recruit
    #[state(superstate = "hot_or_fever")]
    fn fever(&mut self, event: &MomentumEvent) -> Outcome<State> {
        match event {
            MomentumEvent::TickInterference => Transition(State::hot()),
            _ => Super,
        }
    }

    // Superstates
    #[superstate]
    fn warm_or_above(&mut self, _: &MomentumEvent) -> Outcome<State> { Super }

    #[superstate(superstate = "warm_or_above")]
    fn hot_or_fever(&mut self, _: &MomentumEvent) -> Outcome<State> { Super }
}
```

### 7.4 Capability gating API

```rust
impl Momentum {
    pub fn can_write_environment(&self) -> bool {
        matches!(self.state(), State::Hot | State::Fever)
    }
    pub fn can_recruit(&self) -> bool {
        matches!(self.state(), State::Fever)
    }
    pub fn can_call_ai_adapter(&self) -> bool {
        matches!(self.state(), State::Fever)
    }
}
```

### 7.5 Alternatives

- **`rust-fsm`** — no nested superstates; you'd re-check tier in every action body.
- **Cliffle's typestate pattern** (phantom type parameter moving `Bot<Cold>` →
  `Bot<Hot>`) — elegant but compile-time only. You can't store `Vec<Bot<?>>`, which
  formations need.
- **`openraft`'s `ServerState` enum** (`Follower`/`Candidate`/`Leader`) — flat enum,
  simpler than statig but no capability inheritance.

---

## 8. Awareness System

> **PDF section:** §8. Game sources: Total War morale (composite local signal), Siege
> callouts, Splinter Cell split approach.

### 8.1 Mechanism

Each agent holds `HashMap<AgentId, NeighborSnapshot>` updated through gossip — not by
asking a central store. Think Total War morale propagation: each unit sees adjacent
allies routing and takes a morale penalty locally. No general needs to tell them.

### 8.2 Primary implementation — `chitchat` 0.10 (Scuttlebutt)

`chitchat` is the gossip protocol used by Quickwit. It carries arbitrary KV pairs per
node, which maps directly onto `NeighborSnapshot { morale, task, tier, fuel, ... }`.

Verbatim from `chitchat/src/state.rs` lines 27–33:

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct NodeState {
    chitchat_id: ChitchatId,
    heartbeat: Heartbeat,
    key_values: BTreeMap<String, VersionedValue>,
    #[serde(skip)]
    listeners: Listeners,
    max_version: Version,
    last_gc_version: Version,
}
```

Read/write API from `chitchat/src/lib.rs` lines 257–268:

```rust
pub fn node_states(&self) -> &BTreeMap<ChitchatId, NodeState> {
    self.cluster_state.node_states()
}
pub fn node_state(&self, chitchat_id: &ChitchatId) -> Option<&NodeState> {
    self.cluster_state.node_state(chitchat_id)
}
pub fn self_node_state(&mut self) -> &mut NodeState {
    self.cluster_state.node_state_mut_or_init(&self.config.chitchat_id)
}
```

Source: <https://github.com/quickwit-oss/chitchat>

Each agent writes its morale/task/fuel into `self_node_state().set("morale", "hot")`,
neighbors observe via `node_states()`, and chitchat's version counter handles
last-writer-wins.

### 8.3 Secondary — `foca` 1.0 for liveness (SWIM)

`foca` gives you the SWIM suspicion-refutation incarnation counter — the canonical
way to detect agents that crashed mid-formation.

Verbatim from `foca/src/member.rs` lines 17–53:

```rust
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum State {
    Alive,
    Suspect,
    Down,
}

pub type Incarnation = u16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member<T> {
    id: T,
    incarnation: Incarnation,
    state: State,
}
```

State transition rules (lines 105–120 — newer incarnation wins; state precedence
`Down > Suspect > Alive`):

```rust
pub(crate) fn change_state(&mut self, incarnation: Incarnation, state: State) -> bool {
    if self.can_change(incarnation, state) {
        self.state = state;
        self.incarnation = incarnation;
        true
    } else { false }
}

const fn can_change(&self, other_incarnation: Incarnation, other: State) -> bool {
    match self.state {
        State::Alive => match other {
            State::Alive => other_incarnation > self.incarnation,
            State::Suspect => other_incarnation >= self.incarnation,
            State::Down => true,
            // ...
```

Source: <https://github.com/caio/foca>

### 8.4 Concrete LocalAwareness

```rust
// cooperation/awareness.rs
use std::collections::HashMap;
use std::time::Instant;

pub struct LocalAwareness {
    pub neighbor_states: HashMap<AgentId, NeighborSnapshot>,
    pub formation_momentum: MomentumTier,
    pub last_tick_reports: Vec<TickReport>, // only populated at Warming+
}

#[derive(Clone)]
pub struct NeighborSnapshot {
    pub agent_id: AgentId,
    pub health: AgentHealth,
    pub current_role: DynamicRole,
    pub fuel_remaining_pct: f32,
    pub last_action_success: bool,
    pub attention_load: f32,
    pub last_updated: Instant,
    pub liveness: foca::State, // Alive / Suspect / Down
}
```

Each agent runs a chitchat node in-process and bridges it to a `broadcast::Receiver`
of `NeighborSnapshotDelta` events. When chitchat observes a new `VersionedValue` for
a neighbor's `morale` key, the bridge publishes a delta on the peer bus.

### 8.5 Why not CRDTs

`automerge-rs` and `yrs` are overkill. They're full CRDTs built for collaborative
text editing. You don't need merge semantics for rapidly-decaying morale telemetry
where last-writer-wins-per-version is correct. Chitchat is the right abstraction
level.

---

## 9. Attention Economy

> **PDF section:** §9. Game source: Army of Two aggro system.
>
> **LFCG status: genuinely novel Springtale extension.** LFCG implicitly treats
> human player attention as uncapped per-player; no taxonomic axis models
> per-agent bounded attention budgets. Machine agents have hard attention
> constraints (fuel meters, tick quotas, concurrent action ceilings) that LFCG
> legitimately leaves out of scope. For implementation, adopt TrinityCore's
> `ThreatManager` as the concrete reference — 1000 ms tick, 110%/130% hysteresis,
> Fibonacci heap reselect — see Appendix B.8.

### 9.1 Mechanism

One agent's high workload consumption directly enables another's freedom. Zero-sum
within the formation. Army of Two's aggro meter is a visible pendulum — whoever has
higher aggro draws all enemy attention, the other becomes semi-transparent.

### 9.2 Implementation — `arc_swap::ArcSwap<AttentionEconomy>`

The attention distribution is read on every tick by every agent, and written
occasionally when an agent takes an attention-drawing action. This is a classic
read-dominated workload, and `ArcSwap` is the canonical primitive.

```rust
// cooperation/attention.rs
use arc_swap::ArcSwap;
use std::collections::HashMap;
use std::sync::Arc;

pub struct AttentionEconomy {
    pub total_attention: f32,
    pub distribution: HashMap<AgentId, f32>, // sums to 1.0
}

pub struct AttentionBroker {
    state: ArcSwap<AttentionEconomy>,
}

impl AttentionBroker {
    pub fn current(&self) -> Arc<AttentionEconomy> {
        self.state.load_full()
    }

    /// Redistribute after an action that drew attention.
    pub fn absorb(&self, agent: AgentId, delta: f32) {
        self.state.rcu(|prev| {
            let mut new = (**prev).clone();
            let current = new.distribution.get(&agent).copied().unwrap_or(0.0);
            new.distribution.insert(agent, (current + delta).clamp(0.0, 1.0));
            // Renormalize
            let sum: f32 = new.distribution.values().sum();
            if sum > 0.0 {
                for v in new.distribution.values_mut() { *v /= sum; }
            }
            Arc::new(new)
        });
    }

    /// Army of Two Overkill trigger: >90% concentration unlocks power state.
    pub fn in_overkill(&self) -> Option<AgentId> {
        let snapshot = self.state.load();
        snapshot.distribution.iter()
            .find(|(_, &v)| v > 0.9)
            .map(|(id, _)| *id)
    }
}
```

`ArcSwap::rcu` (verbatim from `arc-swap-1.9.1/src/lib.rs` lines 622–639):

```rust
pub fn rcu<R, F>(&self, mut f: F) -> T
where
    F: FnMut(&T) -> R,
    R: Into<T>,
    S: CaS<T>,
{
    let mut cur = self.load();
    loop {
        let new = f(&cur).into();
        let prev = self.compare_and_swap(&*cur, new);
        let swapped = ptr_eq(&*cur, &*prev);
        if swapped {
            return Guard::into_inner(prev);
        } else {
            cur = prev;   // someone else wrote; retry with their version
        }
    }
}
```

This is read-copy-update: readers never block, writers retry on contention.
Contention here is low (attention shifts are seconds apart, reads are per-tick), so
the pattern is correct and fast.

---

## 10. Shared Environment

> **PDF section:** §10. Game sources: Siege destructible walls, Divinity surface
> system.

### 10.1 Mechanism

A shared mutable workspace with typed "surfaces" — Divinity's water → oil → fire
elemental chain. Multiple agents read and write concurrently. Conflicts must be
detectable, not silently clobbered.

### 10.2 Two-layer design — `dashmap` + `arc-swap`

- **Inner layer:** `DashMap<String, Value>` for high-churn per-cell writes. Dashmap
  shards keys across locks; when agents write to different keys contention is near
  zero. This is the unstructured blackboard.
- **Outer layer:** `ArcSwap<Arc<WorkspaceSnapshot>>` where `WorkspaceSnapshot`
  contains the write-log and the typed `Vec<Surface>`. This gives you the Divinity
  water→oil→fire chain as an atomic RCU rebuild.

Verbatim from `dashmap-6.1.0/src/lib.rs` lines 787–793 and the doctest at 771–781:

```rust
pub fn alter<Q>(&self, key: &Q, f: impl FnOnce(&K, V) -> V)
where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
{
    self._alter(key, f);
}

// doc example:
// let stats = DashMap::new();
// stats.insert("Goals", 4);
// stats.alter("Goals", |_, v| v * 2);
// assert_eq!(*stats.get("Goals").unwrap(), 8);
```

And the arc-swap RCU pattern for the outer snapshot (same `rcu` code shown in §9):

```rust
// arc-swap-1.9.1/src/lib.rs lines 594–606 — HashMap RCU
// CACHE.rcu(|cache| {
//     let mut cache = HashMap::clone(&cache);
//     cache.insert(x, result);
//     cache
// });
```

### 10.3 Concrete SharedEnvironment

```rust
// cooperation/environment.rs
use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use std::time::Duration;

pub struct SharedEnvironment {
    workspace: DashMap<String, serde_json::Value>,
    snapshot: ArcSwap<WorkspaceSnapshot>,
}

#[derive(Clone)]
pub struct WorkspaceSnapshot {
    pub write_log: Vec<EnvironmentWrite>,
    pub surfaces: Vec<Surface>,
    pub version: u64,
}

#[derive(Clone)]
pub struct Surface {
    pub created_by: AgentId,
    pub surface_type: SurfaceType,
    pub data: serde_json::Value,
    pub expires: Option<Instant>,
}

#[derive(Clone)]
pub enum SurfaceType {
    Substrate,                              // Divinity: water on ground
    Primed { trigger: ActionDescriptor },   // Divinity: oil ready to ignite
    Active { remaining: Duration },         // Divinity: fire burning
}

impl SharedEnvironment {
    pub fn write(&self, key: &str, value: serde_json::Value, author: AgentId) {
        self.workspace.insert(key.to_string(), value.clone());
        self.snapshot.rcu(|prev| {
            let mut new = (**prev).clone();
            new.write_log.push(EnvironmentWrite {
                author, key: key.to_string(), value: value.clone(),
                timestamp: Instant::now(),
            });
            new.version += 1;
            Arc::new(new)
        });
    }

    pub fn add_surface(&self, s: Surface) {
        self.snapshot.rcu(|prev| {
            let mut new = (**prev).clone();
            // Divinity chain: water + electricity = shocked water
            // water + fire = evaporate
            new.surfaces = compose_surfaces(&new.surfaces, &s);
            new.version += 1;
            Arc::new(new)
        });
    }
}
```

### 10.4 Why not sled / stm

- **`sled` transactional API** — works, but sled is a full on-disk B-tree. Too heavy
  for an in-memory workspace that resets per mission.
- **`stm` crate (software transactional memory)** — academically correct, last
  release 2019 per crates.io. Unmaintained; do not depend.
- **No mature blackboard crate** exists in the Rust ecosystem. The dashmap +
  arc-swap combo is the idiomatic substitute and is the pattern used by e.g.
  `tokio-console` and `rustc_query_system` for similar workloads.

---

## 11. Consensus Engine

> **PDF section:** §11. Game source: As Dusk Falls voting + override system.

### 11.1 Mechanism

N agents vote on `DecisionDescriptor` with deadline. Resolution = `Majority` |
`Override { by, choice, cost }` | `Timeout`. Overrides are visible to all and cost a
scarce token. Only available at Fever momentum tier (§7).

### 11.2 Primary implementation — `openraft` 0.9 vote semantics

Don't run full Raft — this is overkill for in-process agent consensus. Instead,
**steal the `Vote` ordering and vote-handling pattern** from openraft. Openraft's
`VoteOf<C>` is totally ordered `(term, node_id, committed)`, which maps cleanly onto
"majority vs. override": an override is a vote with `committed = true`, which is
strictly greater than any uncommitted ballot of the same term, so it wins without
quorum. That is exactly the As Dusk Falls semantics.

Verbatim from openraft main branch, `openraft/src/engine/handler/vote_handler/mod.rs`
(the heart of openraft's vote acceptance):

```rust
pub(crate) struct VoteHandler<'st, C, SM = ()>
where C: RaftTypeConfig
{
    pub(crate) config: &'st mut EngineConfig<C>,
    pub(crate) state: &'st mut RaftState<C>,
    pub(crate) output: &'st mut EngineOutput<C, SM>,
    pub(crate) leader: &'st mut LeaderState<C>,
    pub(crate) candidate: &'st mut CandidateState<C>,
}

#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn update_vote(&mut self, vote: &VoteOf<C>) -> Result<(), RejectVote<C>> {
    if vote.as_ref_vote() >= self.state.vote_ref().as_ref_vote() {
        // Ok
    } else {
        tracing::info!("vote {} is rejected by local vote: {}", vote, self.state.vote_ref());
        return Err(RejectVote { higher: self.state.vote_ref().clone() });
    }

    let leader_lease = if vote.is_committed() {
        self.config.timer_config.leader_lease
    } else {
        Duration::default()
    };

    if vote.as_ref_vote() > self.state.vote_ref().as_ref_vote() {
        self.state.vote.update(C::now(), leader_lease, vote.clone());
        self.state.accept_log_io(IOId::new(vote));
        self.output.push_command(Command::SaveVote { vote: vote.clone() });
    } else {
        let now = C::now();
        self.state.vote.touch(now, leader_lease);
    }
    self.update_internal_server_state();
    Ok(())
}
```

And the `accept_vote` wrapper with a `RejectVote` responder callback — this is
exactly the pattern we need for deadline-bounded consensus with a resolver hook:

```rust
pub(crate) fn accept_vote<T, F>(
    &mut self,
    vote: &VoteOf<C>,
    tx: OneshotSenderOf<C, T>,
    f: F,
) -> Option<OneshotSenderOf<C, T>>
where
    T: Debug + Eq + OptionalSend,
    Respond<C>: From<ValueSender<C, T>>,
    F: Fn(&RaftState<C>, RejectVote<C>) -> T,
```

Source: <https://github.com/databendlabs/openraft/blob/main/openraft/src/engine/handler/vote_handler/mod.rs>

### 11.3 Concrete ConsensusVote

```rust
// cooperation/consensus.rs
use std::collections::HashMap;
use std::time::Instant;

pub struct ConsensusVote {
    pub question: DecisionDescriptor,
    pub votes: HashMap<AgentId, VoteChoice>,
    pub deadline: Instant,
    pub overrides_remaining: HashMap<AgentId, u32>, // scarce token
}

pub enum VoteResolution {
    Majority(VoteChoice),
    Override { by: AgentId, choice: VoteChoice, cost: u32 },
    Timeout(VoteChoice),
}

impl ConsensusVote {
    pub fn cast(&mut self, agent: AgentId, choice: VoteChoice) {
        self.votes.insert(agent, choice);
    }

    /// Override always wins, but consumes a token and is visible to all.
    pub fn override_resolution(&mut self, agent: AgentId, choice: VoteChoice)
        -> Result<VoteResolution, ConsensusError>
    {
        let remaining = self.overrides_remaining.get(&agent).copied().unwrap_or(0);
        if remaining == 0 {
            return Err(ConsensusError::NoOverrideTokens(agent));
        }
        *self.overrides_remaining.get_mut(&agent).unwrap() -= 1;
        Ok(VoteResolution::Override { by: agent, choice, cost: 1 })
    }

    pub fn resolve(&self) -> Option<VoteResolution> {
        if Instant::now() > self.deadline {
            let winner = majority(&self.votes);
            return Some(VoteResolution::Timeout(winner));
        }
        if self.votes.len() >= self.question.required_participants as usize {
            Some(VoteResolution::Majority(majority(&self.votes)))
        } else {
            None
        }
    }
}
```

### 11.4 Honest gap — scarce override tokens

Openraft does not model per-vote cost accounting or an override ledger. The
scarce-token mechanic is our addition. The object-capability pattern from
`capnproto-rust` (`capnp-rpc` uses `Client` handles as unforgeable references;
dropping the handle consumes the capability) is the right inspiration but was **not**
verified against source in this pass. If unforgeable override tokens matter for your
threat model, research `capnp-rpc` before building.

A simpler approach: store override counts in `springtale-store` SQLite with a row
lock per agent per formation. Correct, less elegant, less footgun.

---

## 12. Synchronized Commit

> **PDF section:** §12. Game sources: Splinter Cell dual breach, Army of Two co-op
> snipe.

### 12.1 Mechanism

```
   Prepare ──▶ Ready ──▶ Countdown ──▶ Execute ──▶ Collect
      │         │           │             │          │
      │         │           │             │          │
   each      every        all           all        gather
   agent      peer         peers         peers      results
   sets up    hits         wait          commit     via mpsc
   state      barrier      deadline      simul-
                           or abort      taneously
```

All agents must reach Ready before any agent begins Countdown. All agents must be
present at Execute. Available at Hot+ tier. Cooperation is in planning; execution is
deterministic.

### 12.2 Primary implementation — `tokio::sync::Barrier` + custom oneshot 2PC

Tokio's `Barrier` is built on `watch::channel` internally (verbatim from
`tokio-1.50.0/src/sync/barrier.rs` line 64) and is re-usable across generations
(lines 115–116: "Barriers are re-usable after all tasks have rendezvoused once, and
can be used continuously").

Verbatim from `barrier.rs` lines 58–111, 113–138:

```rust
impl Barrier {
    /// Creates a new barrier that can block a given number of tasks.
    ///
    /// A barrier will block `n`-1 tasks which call [`Barrier::wait`] and then wake
    /// up all tasks at once when the `n`th task calls `wait`.
    #[track_caller]
    pub fn new(mut n: usize) -> Barrier {
        let (waker, wait) = crate::sync::watch::channel(0);
        if n == 0 {
            n = 1;
        }
        Barrier {
            state: Mutex::new(BarrierState {
                waker,
                arrived: 0,
                generation: 1,
            }),
            n,
            wait,
        }
    }

    pub async fn wait(&self) -> BarrierWaitResult {
        return self.wait_internal().await;
    }
}
```

### 12.3 Critical — Barrier::wait is NOT cancel safe

**Verbatim from `barrier.rs` lines 122–124:** *"# Cancel safety — This method is not
cancel safe."*

This means you cannot `select!` a timeout against `barrier.wait()` and expect the
barrier to survive a cancelled branch. The correct pattern is:

1. Wrap each peer's `wait()` call in a spawned task
2. Signal abort via a shared `tokio_util::sync::CancellationToken`
3. Peers observe the token and drop out rather than having their `wait()` cancelled

OR — and this is probably cleaner — build the 2PC from `tokio::sync::oneshot`:

```rust
// cooperation/commit.rs
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};
use futures::future::try_join_all;

pub enum CommitPhase {
    Prepare,
    Ready,
    Countdown { remaining: Duration },
    Execute,
    Collect,
}

pub async fn two_phase_commit(
    participants: Vec<AgentId>,
    deadline: Duration,
) -> Result<Vec<ExecutionResult>, CommitError> {
    // Phase 1: every peer returns a oneshot::Sender<Vote>
    let (ready_senders, ready_receivers): (Vec<_>, Vec<_>) =
        participants.iter().map(|_| oneshot::channel::<Vote>()).unzip();

    // dispatch prepare to each agent (pseudocode)
    for (agent, tx) in participants.iter().zip(ready_senders.into_iter()) {
        dispatch_prepare(*agent, tx).await;
    }

    // Collect all votes or timeout
    let votes = timeout(deadline, try_join_all(ready_receivers))
        .await
        .map_err(|_| CommitError::ReadyTimeout)?
        .map_err(|_| CommitError::ParticipantDropped)?;

    if !votes.iter().all(|v| matches!(v, Vote::Ready)) {
        return Err(CommitError::VoteFailed);
    }

    // Phase 2: broadcast commit via a second round of oneshots
    let (commit_senders, commit_receivers): (Vec<_>, Vec<_>) =
        participants.iter().map(|_| oneshot::channel::<ExecutionResult>()).unzip();

    for (agent, tx) in participants.iter().zip(commit_senders.into_iter()) {
        dispatch_commit(*agent, tx).await;
    }

    let results = try_join_all(commit_receivers)
        .await
        .map_err(|_| CommitError::ExecutionFailed)?;

    Ok(results)
}
```

This has no cancel-safety footgun, no shared state beyond the channels, and maps 1:1
to the Prepare→Ready→Execute→Collect sequence.

### 12.4 Alternatives

- **`openraft` full Raft** — overkill for in-process commit. Use only if the
  formation spans processes.
- **`std::sync::Barrier`** — blocks the OS thread. Never use in async code.

---

## 13. Interference Detection

> **PDF section:** §13. Game sources: Helldivers 2 friendly fire, Divinity combos
> hitting allies, Total War archers hitting own infantry.

### 13.1 Mechanism

Four kinds of interference:
1. **ResourceConflict** — both agents modified the same resource
2. **ActionNegation** — A undid B's work
3. **CollateralDamage** — A's side effect harmed B
4. **Redundancy** — both did the same thing

Tick-by-tick detection.

### 13.2 Primary implementation — `sled::compare_and_swap`

`sled`'s optimistic concurrency primitive returns **both** `current` and `proposed`
on failure — the ingredients for classifying conflicts.

Verbatim from sled main branch, `src/tree.rs`:

```rust
pub fn compare_and_swap<K, OV, NV>(
    &self,
    key: K,
    old: Option<OV>,
    new: Option<NV>,
) -> CompareAndSwapResult
where
    K: AsRef<[u8]>,
    OV: AsRef<[u8]>,
    NV: Into<InlineArray>,
{
    self.check_error()?;
    let key_ref = key.as_ref();
    let mut leaf_guard = self.leaf_for_key_mut(key_ref)?;
    let proposed: Option<InlineArray> = new.map(Into::into);
    let leaf = leaf_guard.leaf_write.leaf.as_mut().unwrap();
    let current = leaf.get(key_ref).cloned();

    let previous_matches = match (old, &current) {
        (None, None) => true,
        (Some(conditional), Some(current))
            if conditional.as_ref() == current.as_ref() => true,
        _ => false,
    };

    let ret = if previous_matches {
        if let Some(ref new_value) = proposed {
            leaf.insert(key_ref.into(), new_value.clone())
        } else {
            leaf.remove(key_ref)
        };
        Ok(CompareAndSwapSuccess {
            new_value: proposed,
            previous_value: current,
        })
    } else {
        Err(CompareAndSwapError { current, proposed })
    };
    // ...
}
```

Source: <https://github.com/spacejam/sled/blob/main/src/tree.rs>

### 13.3 Concrete InterferenceDetector

```rust
// cooperation/interference.rs
use std::collections::{HashMap, HashSet};

pub struct InterferenceEvent {
    pub tick_sequence: u64,
    pub agent_a: AgentId,
    pub agent_b: AgentId,
    pub interference_type: InterferenceType,
    pub severity: f32,
}

pub enum InterferenceType {
    ResourceConflict,   // both modified same resource
    ActionNegation,     // A undid B's work
    CollateralDamage,   // A's side effect harmed B
    Redundancy,         // both did the same thing
}

/// Per-tick record each agent emits.
pub struct ActionRecord {
    pub agent: AgentId,
    pub read_set: HashSet<String>,
    pub write_set: HashMap<String, serde_json::Value>,
    pub side_effects: Vec<SideEffect>,
}

pub fn detect_interference(
    tick: u64,
    records: &[ActionRecord],
) -> Vec<InterferenceEvent> {
    let mut events = Vec::new();

    for (i, a) in records.iter().enumerate() {
        for b in records.iter().skip(i + 1) {
            // ResourceConflict: write-set intersection
            for key in a.write_set.keys() {
                if b.write_set.contains_key(key) {
                    let redundant = a.write_set[key] == b.write_set[key];
                    events.push(InterferenceEvent {
                        tick_sequence: tick,
                        agent_a: a.agent, agent_b: b.agent,
                        interference_type: if redundant {
                            InterferenceType::Redundancy
                        } else {
                            InterferenceType::ResourceConflict
                        },
                        severity: if redundant { 0.2 } else { 0.8 },
                    });
                }
            }

            // CollateralDamage: A's side effect into B's read set
            for se in &a.side_effects {
                if b.read_set.contains(&se.affected_key) {
                    events.push(InterferenceEvent {
                        tick_sequence: tick,
                        agent_a: a.agent, agent_b: b.agent,
                        interference_type: InterferenceType::CollateralDamage,
                        severity: se.magnitude,
                    });
                }
            }
        }
    }
    events
}
```

Interference decreases momentum (§7). Repeated interference triggers role adaptation
(§14).

### 13.4 Honest gap

`automerge` has operation-level `ActionNegation` detection via Lamport timestamps on
`OpId`, but that source was not verified in this research pass. If you need
document-level undo detection for rich structured documents, read
`automerge/src/op_tree.rs` before designing.

`crossbeam-epoch` is **not** the right tool here — it's a memory reclamation scheme
for lock-free structures, not a conflict detector.

---

## 14. Role Transformation

> **PDF section:** §14. Game sources: Siege dead→intel, Army of Two role oscillation,
> It Takes Two chapter abilities.

### 14.1 Mechanism

Agent loses primary capability but doesn't fail — its role transforms. Siege: dead
player switches to cameras. Army of Two: low-aggro player becomes overwatch. It Takes
Two: new chapter gives new tools.

### 14.2 Implementation — `typetag` 0.2 for serializable trait objects

Role transformation is runtime dynamic dispatch. Capabilities are trait objects. We
want them persistable (so a formation can survive a process restart) and
enumerable (so the current role can be introspected).

Verbatim from <https://docs.rs/typetag/latest/typetag/>:

```rust
#[typetag::serde(tag = "type")]
trait WebEvent {
    fn inspect(&self);
}

#[derive(Serialize, Deserialize)]
struct PageLoad;

#[typetag::serde]
impl WebEvent for PageLoad {
    fn inspect(&self) {
        println!("200 milliseconds or bust");
    }
}

#[derive(Serialize, Deserialize)]
struct Click {
    x: i32,
    y: i32,
}

#[typetag::serde]
impl WebEvent for Click {
    fn inspect(&self) {
        println!("negative space between the ads: x={} y={}", self.x, self.y);
    }
}

fn process_event_from_clickfarm(json: &str) -> Result<()> {
    let event: Box<dyn WebEvent> = serde_json::from_str(json)?;
    Ok(())
}
```

`Box<dyn WebEvent>` round-trips through JSON via the `inventory`-based registry.

### 14.3 Known gotcha — `inventory::submit!` in transitive deps

**This is important.** Typetag's registry uses the `inventory` crate, which relies on
linker-side distributed slices. When an `impl` lives in a transitive dependency
(e.g., a connector crate that declares a new Role), the linker may dead-code eliminate
it if nothing in the parent binary references the crate. You need an explicit
`extern crate connector_foo;` in the binary or a `use connector_foo::_;` to force
linking.

This is already a known constraint in the Springtale memory (`feedback_inventory_linker.md`)
and applies directly here.

### 14.4 Concrete RoleTransformation

```rust
// cooperation/transformation.rs
pub enum RoleTransformation {
    /// Primary capability lost. Becomes information-only. Siege: dead→cameras.
    ToInformationAgent,
    /// Primary exhausted. Becomes support. Army of Two: low-aggro→overwatch.
    ToSupportAgent,
    /// Context changed. New tools. It Takes Two: new chapter.
    ReassignCapabilities(Vec<CapabilityDecl>),
}

#[typetag::serde(tag = "role")]
pub trait DynamicRole: Send + Sync {
    fn name(&self) -> &'static str;
    fn can_execute(&self, action: &ActionDescriptor) -> bool;
    fn execute(&self, action: ActionDescriptor, ctx: &FormationContext)
        -> Result<ExecutionResult, RoleError>;
}

pub fn transform(
    member: &mut FormationMember,
    transformation: RoleTransformation,
) -> Result<(), TransformError> {
    let new_role: Box<dyn DynamicRole> = match transformation {
        RoleTransformation::ToInformationAgent => Box::new(InformationAgent::new()),
        RoleTransformation::ToSupportAgent => Box::new(SupportAgent::new()),
        RoleTransformation::ReassignCapabilities(caps) => {
            Box::new(GeneralAgent::with_capabilities(caps))
        }
    };
    member.current_role = new_role;
    Ok(())
}
```

---

## 15. Rally & Cascade Recovery

> **PDF section:** §15. Game sources: Total War general rally, routing cascade,
> Monster Hunter carts.

### 15.1 Mechanism

```
    Agent A fails
         │
         ▼
    neighbors detect (§8 gossip)
         │
         ▼
    ┌─────────────────────────┐
    │ self-rally (local)      │
    │  1. redistribute        │
    │     attention (§9)      │
    │  2. transform role (§14)│
    │  3. reduce momentum     │
    │  4. consume rally token │
    └─────────────────────────┘
         │
         ▼ token exhausted?
         │
         ▼
    orchestrator::intervention (§3.4)
```

Only if the formation's self-rally fails (tokens consumed, Cold momentum, multiple
agents failing) does it escalate.

### 15.2 Primary implementation — `JoinSet` + `Semaphore` + `broadcast`

`tokio::task::JoinSet` is the failure detector: dropping an agent's task goes
through `join_next().await` and returns `Result<T, JoinError>` where
`JoinError::is_panic()` distinguishes crash from clean exit.

Verbatim from `tokio-1.50.0/src/task/join_set.rs` lines 36–50:

```rust
let mut set = JoinSet::new();

for i in 0..10 {
    set.spawn(async move { i });
}

let mut seen = [false; 10];
while let Some(res) = set.join_next().await {
    let idx = res.unwrap();
    seen[idx] = true;
}
```

**Critical detail** (verified at `join_set.rs` line 25): *"When the JoinSet is
dropped, all tasks in the JoinSet are immediately aborted."* The formation lifetime
must outlive every member. You can't "adopt" a peer's task into a different formation
— you'd need `tokio::spawn` + manually-tracked `AbortHandle`, losing the automatic
failure channel.

### 15.3 Rally token as `Semaphore`

`tokio::sync::Semaphore::try_acquire_owned()` is non-blocking and returns `Err` when
the budget is exhausted. That's exactly the "rally failed, escalate" signal.

Docs: <https://docs.rs/tokio/1.50.0/tokio/sync/struct.Semaphore.html>

### 15.4 Concrete Rally

```rust
// cooperation/rally.rs
use tokio::task::JoinSet;
use tokio::sync::{broadcast, Semaphore};
use std::sync::Arc;

pub struct FormationRally {
    members: JoinSet<AgentOutcome>,
    rally_tokens: Arc<Semaphore>,
    events: broadcast::Sender<RallyEvent>,
}

pub enum RallyEvent {
    PeerDown { id: AgentId, reason: FailureReason },
    AttentionRedistributed { from: AgentId, to: HashMap<AgentId, f32> },
    RoleTransformed { agent: AgentId, new_role: String },
    MomentumDowngrade { from: MomentumTier, to: MomentumTier },
    TokenConsumed { remaining: usize },
    EscalationRequested { reason: String },
}

impl FormationRally {
    pub fn new(token_budget: usize, bus_capacity: usize) -> Self {
        let (events, _) = broadcast::channel(bus_capacity);
        Self {
            members: JoinSet::new(),
            rally_tokens: Arc::new(Semaphore::new(token_budget)),
            events,
        }
    }

    /// Main supervision loop.
    pub async fn supervise(&mut self) -> Result<(), RallyFailure> {
        while let Some(result) = self.members.join_next().await {
            match result {
                Ok(outcome) if outcome.clean_exit() => continue,
                Ok(outcome) => self.handle_peer_failure(outcome.reason()).await?,
                Err(join_err) if join_err.is_panic() => {
                    self.handle_peer_failure(FailureReason::Panic).await?
                }
                Err(join_err) if join_err.is_cancelled() => {
                    // Planned abort; no rally needed
                    continue;
                }
                Err(_) => break,
            }
        }
        Ok(())
    }

    async fn handle_peer_failure(&self, reason: FailureReason) -> Result<(), RallyFailure> {
        let token = self.rally_tokens.clone().try_acquire_owned()
            .map_err(|_| RallyFailure::NoTokensLeft)?;
        let _ = self.events.send(RallyEvent::PeerDown {
            id: AgentId::default(), reason
        });

        // 1. Redistribute attention (§9)
        // 2. Transform roles (§14)
        // 3. Reduce momentum (§7)
        // ... each step can fail back to escalation

        let _ = self.events.send(RallyEvent::TokenConsumed {
            remaining: self.rally_tokens.available_permits(),
        });
        drop(token); // token consumed
        Ok(())
    }
}
```

### 15.5 Alternatives

- **`ractor` supervision** — parent-child by design. Wrong shape for peer rally.
- **`tower::retry::budget::Budget`** — exactly the "rally token" pattern for RPCs.
  Steal the idea, not the crate; it operates on `Service<Request>` not long-lived
  tasks.
- **`futures::FuturesUnordered`** — similar collection semantics to `JoinSet` but
  without `AbortHandle` integration or `JoinError` discrimination. `JoinSet` is
  strictly better.

---

## 16. Dynamic Capability Binding

> **PDF section:** §16. Game source: It Takes Two chapter-based reassignment.

### 16.1 Mechanism

```rust
pub struct DynamicCapabilitySet {
    pub base_capabilities: Vec<CapabilityDecl>,        // connector manifest
    pub context_capabilities: Vec<CapabilityDecl>,     // formation context
    pub momentum_unlocked: Vec<CapabilityDecl>,        // momentum tier
    pub transformed_capabilities: Vec<CapabilityDecl>, // role transformation
}
```

### 16.2 Primary implementation — `wasmtime::Linker` rebinding

For sandboxed connector capabilities, `wasmtime::Linker<T>` is the correct
enforcement boundary. The Linker is **mutable** — you can build a different linker
per formation context. The same compiled `Module` can be instantiated against
multiple linkers, each exposing a different subset of host capabilities.

Verbatim from `wasmtime-43.0.1/src/runtime/linker.rs` lines 515–542 (doctest from
`Linker::func_wrap`, runs in wasmtime CI):

```rust
use wasmtime::*;
let engine = Engine::default();
let mut linker = Linker::new(&engine);
linker.func_wrap("host", "double", |x: i32| x * 2)?;
linker.func_wrap("host", "log_i32", |x: i32| println!("{}", x))?;
linker.func_wrap("host", "log_str", |caller: Caller<'_, ()>, ptr: i32, len: i32| {
    // ...
})?;

let wat = r#"
    (module
        (import "host" "double" (func (param i32) (result i32)))
        (import "host" "log_i32" (func (param i32)))
        (import "host" "log_str" (func (param i32 i32)))
    )
"#;
let module = Module::new(&engine, wat)?;

for _ in 0..10 {
    let mut store = Store::new(&engine, ());
    linker.instantiate(&mut store, &module)?;
}
```

Signature (same file, 543–553):

```rust
pub fn func_wrap<Params, Args>(
    &mut self,
    module: &str,
    name: &str,
    func: impl IntoFunc<T, Params, Args>,
) -> Result<&mut Self>
```

### 16.3 Mapping to `DynamicCapabilitySet`

```
┌──────────────────────────────────────┐
│          Compiled Module             │  ← connector bytecode, signed
└──────────────┬───────────────────────┘
               │
               │ instantiate against…
               ▼
┌──────────────────────────────────────┐
│    Linker for current context        │
│                                       │
│   ┌── base caps (always bound) ───┐  │
│   ├── context caps (intent-based)─┤  │
│   ├── momentum caps (tier-gated) ─┤  │
│   └── transformed caps (role) ────┘  │
└──────────────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────┐
│     Store<T> — cheap per-instance    │
└──────────────────────────────────────┘
```

Rebinding is literally "build a new linker, teardown the old Store, instantiate a
fresh one against the new linker." Wasmtime enforces at the WASM ABI boundary that
the agent cannot reach a capability that isn't linked — which is how you make runtime
capability sets *unforgeable*.

### 16.4 In-process (non-WASM) capabilities

For trusted in-process capabilities, use `typetag` (§14) with a registry of
`Box<dyn Capability>`. Persist to SQLite as JSON; reload on formation restart. Same
caveat about the `inventory::submit!` transitive-dep gotcha applies.

### 16.5 Honest gap

`extism` wraps wasmtime and exposes a host-function registration API similar in
shape to `Linker::func_wrap`, but its source was not verified in this pass. Since
Springtale already depends on wasmtime directly per architecture docs, extism would
add a layer without clear benefit.

---

## 17. Integration with Existing Architecture

### 17.1 Sentinel — unchanged

Monitors both orchestrator and cooperation layers. Rate limiters, circuit breakers,
dead-man switch, audit trail apply regardless of source.

### 17.2 Pipeline Engine — recontextualized

`compose_pipeline()` still works. Pipelines become **playbooks** that formations
execute together, rather than trees a parent imposes on children.

### 17.3 Connector sandbox — unchanged

WASM sandbox, manifest signing, capability enforcement remain. §16 adds the
capability-rebinding step per formation context.

### 17.4 Autonomy levels — extended

L1–L4 still apply. Momentum tiers interact: an L2 agent in a Fever formation can
participate in consensus but still requires plan approval. Destructive actions remain
L1 regardless of tier.

### 17.5 Migration path

```
Phase 2a: cooperation/ ships alongside existing orchestrator/. Legacy pipelines work.
Phase 2b: Legacy users migrate to formations. recursive.rs deprecated.
Phase 3:  recursive.rs and subagent.rs removed. Formation-only coordination.
```

> **As-built (June 2026):** `recursive.rs` and `subagent.rs` were removed
> ahead of schedule, in Phase 2b — a workspace-wide sweep found zero callers
> (formations had fully replaced the parent→child pipeline), and pre-launch
> dead code is deleted wholesale rather than deprecated in place. Coordination
> is formation-only as of that removal.

---

## 18. Recovery & Mutual Aid

> **PDF section:** §18. This section in the PDF is exhaustive (8 games' recovery
> patterns). The technical side is a thin layer on top of §15 (rally) and §19
> (communication).

### 18.1 Distress & recovery types

```rust
// cooperation/recovery.rs
pub enum DistressSignal {
    /// Agent health below threshold. Total War: morale dropping.
    HealthLow { agent_id: AgentId, health_pct: f32 },
    /// Agent incapacitated. L4D: downed state. DRG: downed dwarf.
    Incapacitated { agent_id: AgentId, bleedout_remaining: Duration },
    /// Agent dead/disconnected. Helldivers: needs reinforce.
    Dead { agent_id: AgentId, recoverable: bool },
    /// Agent capability degraded. Siege: DBNO with limited actions.
    Degraded { agent_id: AgentId, remaining_capabilities: Vec<CapabilityDecl> },
}

pub enum RecoveryAction {
    /// L4D medkit, DRG revive, Splinter Cell revive, Army of Two drag-heal.
    PeerRevive {
        healer: AgentId,
        target: AgentId,
        duration: Duration,
        healer_vulnerability: f32, // 0.0–1.0
    },

    /// MH Hunting Horn: attack combos heal.
    /// MH Wide-Range: self-healing shares to team.
    /// Divinity Necromancy: dealing damage heals self.
    ByproductRecovery {
        source: AgentId,
        beneficiaries: Vec<AgentId>,
        recovery_amount: f32,
        primary_action: ActionDescriptor, // the productive work that caused healing
    },

    /// Siege Finka boost. Helldivers reinforce. Patapon defend rhythm.
    FormationPulse { source: AgentId, recovery_amount: f32, cost: RecoveryCost },

    /// DRG Red Sugar. L4D safe room. Helldivers resupply convergence.
    EnvironmentalRecovery {
        source_resource: ResourceId,
        beneficiary: AgentId,
        depletes_resource: bool,
    },

    /// Helldivers reinforce. L4D rescue closet.
    Redeployment {
        dead_agent: AgentId,
        replacement_capabilities: Vec<CapabilityDecl>,
        cost: RecoveryCost,
        degraded: bool, // L4D rescue closet: Tier 1 weapons only
    },

    /// Patapon defend rhythm. DRG Gunner shield. Rook armor plates.
    ProactiveProtection {
        protector: AgentId,
        beneficiaries: Vec<AgentId>,
        protection_type: ProtectionType,
    },
}

pub enum RecoveryCost {
    Fuel(FuelBudget),
    SharedFuel(FuelBudget),
    Time(Duration),
    Token { token_type: String, remaining_after: u32 },
    Free,
}
```

### 18.2 Escalating fragility — L4D pattern

Following Left 4 Dead's "black and white" model, recovery quality degrades with
repeated use:

| Recovery Count | State | Capability | Next Failure |
|----------------|-------|------------|--------------|
| 0 (healthy) | Full operational | All capabilities | → Tier 1 recovery |
| 1 (quick-fixed) | Degraded (L4D: 30 HP temp) | Most capabilities, reduced resources | → Tier 2 recovery |
| 2 (quick-fixed twice) | Critical (L4D: black & white) | Limited capabilities, visual degradation | → Dead/removed |
| Proper recovery | Restored | Full capabilities, counter reset | → Tier 1 recovery |

Quick-fix recovery (peer revive, byproduct healing) **increments** the counter.
Proper recovery (environmental convergence, formation rest) **resets** it. This
prevents formations from indefinitely limping on band-aid fixes.

### 18.3 Decision framework

When a distress signal is detected through the awareness system (§8), neighboring
agents evaluate locally:

1. **Can I help?** — capability/resource check
2. **Should I help?** — Army of Two press-attack-vs-save-partner dilemma
3. **Should someone else help?** — Monster Hunter nearest-player-revives
4. **Should we prevent instead?** — Patapon defend-rhythm question
5. **Should we let them transform?** — Siege dead→intel

This evaluation is the `big-brain` utility AI pattern from §24. Decision is local
through the awareness system, NOT centrally through the orchestrator.

### 18.4 Mutual aid principle (Rekindle crosslink)

Every recovery action follows Rekindle's Design Principle 12: *"Mutual aid is the
incentive. No tokens. No payments. No reward systems. The reward is the network
getting better for you."* An agent that helps recover a neighbor benefits from
having that neighbor operational again. The Hunting Horn pattern is the purest
expression: you heal by fighting, and a healed team fights better, which makes your
fighting more effective. Recovery is not charity — it's mutual aid that improves the
system for everyone, including the helper.

---

## 19. Communication Protocols

> **PDF section:** §19. Six communication patterns across nine games. The technical
> reality is that Rust gives us three channel primitives with sharply different
> semantics, and each communication layer maps to the right one.

### 19.1 Channel-to-primitive matrix

| Channel type (PDF §19.2) | Tokio primitive | Semantics |
|--------------------------|-----------------|-----------|
| `StateBroadcast` (auto L4D callouts) | `broadcast::channel` | Every agent must see every callout; lag-drop is correct (stale callouts = noise) |
| `ProtocolMessage` (typed cross-context, MH translated) | `mpsc::channel` (bounded) | Point-to-point, backpressure-safe, no loss |
| `DirectionalSignal` (DRG laser, Siege ping) | `watch::channel` | Only latest target matters; observers conflate updates |
| `CohesionSignal` ("Rock and Stone") | `broadcast::channel` | Shared morale event; clone-to-all, lag is fine |
| `IntentAcknowledgment` (Patapon sing-back) | `mpsc::channel` back-channel | Must be reliable and typed |
| `ImplicitSignal` (Overcooked chicken-throwing) | `watch::channel<AgentState>` | Observers read current state, no history |

### 19.2 Why this mapping

From `tokio-1.50.0/src/sync/broadcast.rs` module docs lines 1–40: `broadcast` is
MPMC; every sent value is *cloned* to every active receiver and retained in a ring
until all receivers have observed it. Slow receivers get `RecvError::Lagged(n)` and
the oldest entry is dropped — lossy by design under overload. `mpsc` is MPSC,
lossless, with backpressure via bounded `send().await`. `watch` is MPMC but keeps
only the latest value — conflating all intermediate updates.

### 19.3 Concrete FormationBus

```rust
// cooperation/comms.rs
use tokio::sync::{broadcast, mpsc, watch};

pub struct FormationBus {
    pub state_broadcast: broadcast::Sender<StateBroadcast>,
    pub cohesion: broadcast::Sender<CohesionSignal>,
    pub directional: watch::Sender<Option<DirectionalSignal>>,
    pub protocol_tx: mpsc::Sender<ProtocolMessage>,
    pub intent_ack_tx: mpsc::Sender<IntentAcknowledgment>,
    pub implicit: watch::Sender<HashMap<AgentId, ActionDescriptor>>,
}

pub enum CommChannel {
    StateBroadcast { source: AgentId, condition: BroadcastTrigger, message: StateMessage },
    ProtocolMessage { source: AgentId, target: MessageTarget, message: ProtocolPayload },
    DirectionalSignal { source: AgentId, target_object: ObjectReference, urgency: f32 },
    CohesionSignal { source: AgentId },
    IntentAcknowledgment { source: AgentId, intent_confirmed: IntentPattern, interpretation: ActionDescriptor },
    ImplicitSignal { source: AgentId, observed_action: ActionDescriptor, inferred_meaning: Option<InferredIntent> },
}

pub enum BroadcastTrigger {
    HealthBelowThreshold(f32),      // L4D: "I'm hurt pretty bad"
    ThreatDetected(ThreatDescriptor), // L4D: "WITCH!"
    ResourceFound(ResourceDescriptor), // L4D: "Pills here!"
    AgentDown(AgentId),              // L4D: "Man down!"
    CapabilityExhausted(CapabilityDecl), // "I'm out of ammo"
}

pub enum MessageTarget {
    Formation,                          // all members
    Specific(AgentId),                  // one agent
    NearestCapable(CapabilityDecl),     // whoever can help
}
```

### 19.4 Phase 3 — p2p gossip for cross-host formations

For formations that span processes (Phase 3 with Veilid), `iroh-gossip` 0.97 gives
the same `Sender/Receiver` mental model as tokio broadcast but over p2p topics. From
<https://docs.rs/iroh-gossip/latest/iroh_gossip/>:

```rust
use iroh_gossip::{api::Event, Gossip, TopicId};

let gossip = Gossip::builder().spawn(endpoint.clone());
let topic_id = TopicId::from_bytes([23u8; 32]);
let (sender, mut receiver) = gossip
    .subscribe(topic_id, bootstrap_peers)
    .await?
    .split();
receiver.joined().await?;
sender.broadcast(b"hello world this is a gossip message".to_vec().into()).await?;
while let Some(event) = receiver.next().await {
    if let Event::Received(message) = event? {
        println!("received: {:?}", std::str::from_utf8(&message.content));
    }
}
```

Do **not** use `async-nats` — it requires a central broker, which violates the
Springtale threat model.

### 19.5 Design principles

- **Overcooked insight:** agents invent communication from any observable behavior.
  So the system should make agent actions observable to neighbors by default. This is
  why `ImplicitSignal` uses `watch<HashMap<AgentId, ActionDescriptor>>` — every
  action write is visible to every observer with zero extra code.
- **Patapon insight:** agents should confirm receipt of intent. `IntentAcknowledgment`
  feeds back into the cadence system (§5), letting the formation know the beat was
  heard.
- **Siege insight:** dead agents become pure information agents. This ties into role
  transformation (§14).
- **MH insight:** cross-context translation for heterogeneous agents — protocol
  messages need a common schema even when agents have different internal
  representations. Use `serde_json::Value` as the wire format.

---

## 20. Handoff & Transition

> **PDF section:** §20. Game sources: Overcooked (handoff as critical coordination
> point), Splinter Cell (sequential dependency), It Takes Two (transformative),
> DRG (flexible chain), MH (shared-target), Divinity (surface), Siege (information).
>
> **LFCG anchor:** a Springtale handoff is an instance of LFCG's
> **Coupled Arrangement + Sequential Synchronicity** (Pais et al. 2024 §4.3.1 +
> §4.3.2). See Appendix C.5 — this is the single strongest vocabulary match in the
> spec.

### 20.1 Five handoff types

```rust
// cooperation/handoff.rs
pub enum HandoffType {
    /// Direct transfer. Overcooked: pass ingredient across counter.
    /// Synchronous — both agents must be ready.
    Direct {
        sender: AgentId,
        receiver: AgentId,
        payload: HandoffPayload,
    },

    /// Environment-mediated. Divinity surfaces, MH monster state.
    /// Asynchronous — sender deposits, receiver collects when ready.
    EnvironmentMediated {
        depositor: AgentId,
        deposit_location: EnvironmentKey,
        payload: HandoffPayload,
        transform_required: Option<TransformDescriptor>,
    },

    /// Flexible chain. DRG minerals.
    /// Any capable agent can perform the next step.
    FlexibleChain {
        originator: AgentId,
        current_step: usize,
        total_steps: usize,
        payload: HandoffPayload,
        next_capability_required: CapabilityDecl,
    },

    /// Sequential dependency. Splinter Cell boost.
    /// Agent A enables B, then B must enable A.
    SequentialDependency {
        enabler: AgentId,
        enabled: AgentId,
        return_obligation: ActionDescriptor,
    },

    /// Information handoff. Siege callouts.
    /// No physical payload — knowledge transfer.
    InformationTransfer {
        source: AgentId,
        recipients: Vec<AgentId>,
        intelligence: IntelligencePayload,
        perishable: bool, // does this info expire quickly?
    },
}

pub struct HandoffPayload {
    pub data: serde_json::Value,
    pub schema: String,
    pub produced_by: ActionDescriptor,
    pub consumable_by: Vec<CapabilityDecl>,
    pub expires: Option<Instant>,
}
```

### 20.2 Direct handoff — `mpsc` + `oneshot`

For `Direct`, use `tokio::sync::mpsc::Sender<HandoffPayload>` per receiving agent. For
`Direct` with a reply, pair with `oneshot`. Nothing exotic.

### 20.3 Environment-mediated — `sled` deposit/collect

`sled::Tree::compare_and_swap` is the primitive. An agent deposits by calling
`tree.insert(key, value)`. The receiver atomically claims by `compare_and_swap(key,
Some(value), None)` — if the claim succeeds, no other agent can double-collect.

Verbatim from <https://docs.rs/sled/latest/sled/>:

```rust
let db: sled::Db = sled::open("my_db").unwrap();

db.insert(b"yo!", b"v1");
assert_eq!(&db.get(b"yo!").unwrap().unwrap(), b"v1");

db.compare_and_swap(
    b"yo!",      // key
    Some(b"v1"), // old value, None for not present
    Some(b"v2"), // new value, None for delete
)
.unwrap();

let scan_key: &[u8] = b"a non-present key before yo!";
let mut iter = db.range(scan_key..);

db.remove(b"yo!");
let other_tree: sled::Tree = db.open_tree(b"cool db facts").unwrap();
```

**Honest gap — TTL:** sled does not have native key TTL. You need a sidecar sweeper
task that scans a `deposited_at` timestamp column and removes stale deposits. If TTL
matters, `redb` also lacks native TTL. This isn't a sled flaw; it's an intentional
design choice across embedded KV stores in Rust. Build the sweeper.

### 20.4 Flexible chain — `crossbeam-deque` work stealing

`crossbeam-deque` is the Chase-Lev work-stealing deque that Rayon's internal scheduler
is built on. "Any capable agent can pick up the next step" is literally work stealing.

Verbatim from `crossbeam-deque-0.8.6/src/lib.rs` lines 1–76 (the crate's module-level
doctest — this is the canonical work-stealing scheduler pattern):

```rust
use crossbeam_deque::{Injector, Stealer, Worker};
use std::iter;

fn find_task<T>(
    local: &Worker<T>,
    global: &Injector<T>,
    stealers: &[Stealer<T>],
) -> Option<T> {
    // Pop a task from the local queue, if not empty.
    local.pop().or_else(|| {
        // Otherwise, we need to look for a task elsewhere.
        iter::repeat_with(|| {
            // Try stealing a batch of tasks from the global queue.
            global.steal_batch_and_pop(local)
                // Or try stealing a task from one of the other threads.
                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
        })
        // Loop while no task was stolen and any steal operation needs retry.
        .find(|s| !s.is_retry())
        // Extract the stolen task, if there is one.
        .and_then(|s| s.success())
    })
}
```

Public types from the same file (line 106): `Injector`, `Steal`, `Stealer`, `Worker`.
Constructor methods: `Worker::new_fifo()`, `Worker::new_lifo()`, `worker.stealer()`,
`worker.push()`, `worker.pop()`, `stealer.steal()`, `stealer.steal_batch()`,
`stealer.steal_batch_and_pop()`. `Steal` is `Empty | Success(T) | Retry`.

Map directly onto `FlexibleChain`:
- Each agent holds its own `Worker<HandoffPayload>`
- Formation holds an `Injector<HandoffPayload>` as the entry point for new chain work
- Every agent holds a `Vec<Stealer<HandoffPayload>>` of its peers' stealers
- An idle agent calls `find_task(local, injector, &peer_stealers)`

This is exactly the DRG mineral-chain semantics: any capable dwarf picks up, any
capable dwarf deposits in Molly.

### 20.5 Design principles

- **Overcooked lesson:** handoff points are where cooperative failures concentrate.
  Track handoff success/failure rates and surface them through the awareness system
  (§8).
- **Divinity/MH lesson:** environment-mediated handoffs decouple sender and receiver
  timing — the sender doesn't need to wait.
- **DRG lesson:** flexible chains are more resilient than fixed assignments because
  any capable agent can pick up a dropped handoff.
- **Splinter Cell lesson:** sequential dependencies create mutual obligation. If A
  enables B, B has a responsibility to enable A in return. Track this in the
  `return_obligation` field; the awareness system should flag agents that accept
  enables without reciprocating.

---

## 21. Shared Mental Model

> **PDF section:** §21. Teams that share a mental model cooperate with less
> communication overhead. The model must be built, not assumed.

### 21.1 Implementation — `rusqlite` + `petgraph`

Two layers:
1. **Persistence:** `rusqlite` tables for `concept` and `relation`
2. **In-memory traversal:** `petgraph::Graph<Concept, Relation>` materialized from
   the SQL rows on load

Verbatim from `rusqlite-0.39.0/examples/persons/main.rs` lines 23–45:

```rust
fn main() -> Result<()> {
    let conn = Connection::open_in_memory()?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS persons (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )",
        (),
    )?;

    conn.execute(
        "INSERT INTO persons (name) VALUES (?1), (?2), (?3)",
        ["Steven", "John", "Alex"].map(|n| n.to_string()),
    )?;

    let mut stmt = conn.prepare("SELECT id, name FROM persons")?;
    let rows = stmt.query_map([], |row| {
        Ok(Person {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    Ok(())
}
```

Verbatim from `petgraph-0.8.3/src/graph_impl/mod.rs` lines 343–355:

```rust
use petgraph::Graph;

let mut deps = Graph::<&str, &str>::new();
let pg   = deps.add_node("petgraph");
let fb   = deps.add_node("fixedbitset");
let qc   = deps.add_node("quickcheck");
let rand = deps.add_node("rand");
let libc = deps.add_node("libc");
deps.extend_with_edges(&[
    (pg, fb), (pg, qc),
    (qc, rand), (rand, libc), (qc, libc),
]);
```

Canonical signatures (same file, 572 / 628):

```rust
pub fn add_node(&mut self, weight: N) -> NodeIndex<Ix> { ... }
pub fn add_edge(&mut self, a: NodeIndex<Ix>, b: NodeIndex<Ix>, weight: E) -> EdgeIndex<Ix> { ... }
```

### 21.2 Schema

```sql
-- authored schema, derived from the rusqlite + petgraph patterns above
CREATE TABLE concept (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,           -- 'capability' | 'pattern' | 'vocab'
    name TEXT NOT NULL UNIQUE,
    payload BLOB NOT NULL         -- bincode or serde_json
);

CREATE TABLE relation (
    src INTEGER NOT NULL REFERENCES concept(id),
    dst INTEGER NOT NULL REFERENCES concept(id),
    kind TEXT NOT NULL,           -- 'requires' | 'cooperates_with' | 'alias_of'
    weight REAL NOT NULL DEFAULT 1.0,
    observed_count INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (src, dst, kind)
);

CREATE INDEX idx_relation_kind ON relation(kind);
```

On load, materialize rows into `petgraph::Graph<Concept, Relation>` for BFS ("who
cooperates with whom") and Dijkstra ("path of capability composition"). Persist
deltas via SQLite upsert:

```sql
INSERT INTO relation (src, dst, kind, weight, observed_count)
VALUES (?1, ?2, ?3, ?4, 1)
ON CONFLICT(src, dst, kind) DO UPDATE SET
    observed_count = observed_count + 1,
    weight = ?4;
```

### 21.3 Concrete SharedMentalModel

```rust
// cooperation/mental_model.rs
use std::collections::HashMap;
use petgraph::Graph;

pub struct SharedMentalModel {
    pub domain_knowledge: HashMap<String, DomainEntry>,
    pub capability_awareness: HashMap<AgentId, Vec<CapabilityDecl>>,
    pub cooperation_patterns: Vec<CooperationPattern>,
    pub shared_vocabulary: HashMap<String, VocabularyEntry>,
    pub conventions: Vec<Convention>,
    graph: Graph<Concept, Relation>,
}

pub struct CooperationPattern {
    pub trigger: PatternTrigger,
    pub participants: Vec<RoleInPattern>,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_used: Instant,
}

pub struct Convention {
    pub description: String,
    pub established_by: Vec<AgentId>,
    pub strength: f32, // how consistently this convention is followed
}
```

### 21.4 Why not cozo / surrealdb

- **cozo** — datalog is powerful but adds ~15MB of deps and a query language nobody
  on the team will know.
- **surrealdb embedded** — as of 2026 the embedded story is still rocky; pulls in a
  massive transitive dep tree.

SQLite + petgraph is boring, proven, and already fits Springtale's dep pins.

### 21.5 Honest gaps

`rig` (0xPlaygrounds) and `llmchain-rs` both ship memory stores for agent frameworks.
Neither was verified against source in this research pass. If you want to compare
their persistence patterns before committing, read `rig-core/src/vector_store` and
`rig-core/src/agent`.

---

## 22. Tempo & Pacing

> **PDF section:** §22. Game sources: L4D Adaptive Dramatic Pacing, Total War
> fatigue, Siege timer, Patapon BPM.

### 22.1 Mechanism

```
┌───────────────────────────────────────────────────┐
│                Pacing phase machine                │
│                                                    │
│   Preparation ──▶ Active ──▶ Peak ──▶ Recovery    │
│        ▲                                  │       │
│        └──────────────────────────────────┘       │
│                                                    │
│   + Disruption can fire at any phase (external)   │
└───────────────────────────────────────────────────┘
```

### 22.2 Honest gap upfront

**There is no published Rust crate that directly implements L4D's Adaptive Dramatic
Pacing.** Searching crates.io and GitHub patterns turned up nothing. The closest
primitive is `governor` (GCRA rate limiting) with runtime-reconfigurable quotas. The
phase machine on top is bespoke code — this section documents the bespoke code
against the verified governor primitive.

### 22.3 `governor` 0.6 — the throttling primitive

Verbatim from `governor-0.6.3/src/lib.rs` lines 15–27:

```rust
use std::num::NonZeroU32;
use nonzero_ext::*;
use governor::{Quota, RateLimiter};

let mut lim = RateLimiter::direct(Quota::per_second(nonzero!(50u32)));
assert_eq!(Ok(()), lim.check());
```

Verbatim from `governor-0.6.3/src/quota.rs` lines 32–61 — quota semantics that are
load-bearing for the peak→recovery behavior:

```rust
// Construct a quota that allows 50 cells per second (replenishing at a rate of
// one cell per 20 milliseconds), with a burst size of 50 cells
let q = Quota::per_second(nonzero!(50u32));
assert_eq!(q, Quota::per_second(nonzero!(50u32)).allow_burst(nonzero!(50u32)));
assert_eq!(q.replenish_interval(), Duration::from_millis(20));
assert_eq!(q.burst_size().get(), 50);

// 2 cells per hour, bursting up to 90:
let q = Quota::per_hour(nonzero!(2u32)).allow_burst(nonzero!(90u32));
assert_eq!(q.replenish_interval(), Duration::from_secs(30 * 60));
assert_eq!(q.burst_size().get(), 90);
```

Async wait from `src/state/direct/future.rs` line 30:

```rust
pub async fn until_ready(&self) -> MW::PositiveOutcome {
    self.until_ready_with_jitter(Jitter::NONE).await
}
```

### 22.4 Why GCRA fits the Director pattern

Governor's GCRA is a leaky bucket with explicit burst capacity. You model cumulative
intensity as bucket fullness:

| Phase | Quota | Burst |
|-------|-------|-------|
| Preparation | slow | large headroom |
| Active | medium | medium |
| Peak | high | low headroom |
| Recovery | ~1/sec | small (forces `until_ready().await` to stall) |
| Disruption | bypass | — |

### 22.5 The "mutable quota" gotcha

Governor's `Quota` is constructed up front and `RateLimiter` is not mutable in place.
The canonical workaround is to hold the limiter behind `ArcSwap<RateLimiter<...>>`
and swap atomically when the phase transitions. This is the standard pattern in
production services.

```rust
// cooperation/pacing.rs
use arc_swap::ArcSwap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use nonzero_ext::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub enum PacingPhase {
    Preparation { started: Instant },
    Active { intensity: f32, started: Instant },
    Peak { intensity: f32, fuel_rate: f32, started: Instant },
    Recovery { remaining: Duration },
    Disruption { event: DisruptionEvent },
}

pub struct PacingManager {
    pub current_phase: PacingPhase,
    pub cumulative_intensity: f32,
    pub time_since_last_recovery: Duration,
    pub disruption_count: u32,

    pub peak_duration_max: Duration,
    pub recovery_duration_min: Duration,
    pub intensity_ceiling: f32,

    limiter: ArcSwap<DefaultDirectRateLimiter>,
}

impl PacingManager {
    pub fn enter_recovery(&self) {
        let new_quota = Quota::per_second(nonzero!(1u32))
            .allow_burst(nonzero!(2u32));
        let new_limiter = RateLimiter::direct(new_quota);
        self.limiter.store(Arc::new(new_limiter));
    }

    pub fn enter_peak(&self) {
        let new_quota = Quota::per_second(nonzero!(50u32))
            .allow_burst(nonzero!(50u32));
        let new_limiter = RateLimiter::direct(new_quota);
        self.limiter.store(Arc::new(new_limiter));
    }

    pub async fn await_action_slot(&self) {
        let lim = self.limiter.load();
        lim.until_ready().await;
    }
}
```

### 22.6 What governor does not do (honest)

- Does **not** implement AIMD (additive-increase/multiplicative-decrease)
- Does **not** track cumulative intensity across phases
- Does **not** schedule phase transitions

That's your Director state machine. Write it. No shortcut crate exists for that
part.

### 22.7 AIMD option not recommended blind

Netflix's concurrency-limits port (community Rust crates exist with names like
`concurrency-limits` / similar — verify before depending) implements Little's Law-
based adaptive limits (Vegas, Gradient2), closer to true adaptive pacing than
governor. These were **not** verified against source in this research pass. If you
want automatic pressure-responsive adaptation rather than scripted phases, evaluate
them separately.

---

## 23. Specialization vs Generalization

> **PDF section:** §23. This is a design principle, not a module. Enforced through
> §3.1 composer, §16 capability binding, and §24 sacrifice decision framework.
>
> **LFCG anchor:** §23 is LFCG's **Relations between Player Actions →
> Complementarity** (Pais et al. 2024 §4.4.6), extended from Rocha et al. 2008.
> The Hunting Horn / DRG specialization design ideal this section argues for is
> precisely "Complementarity" in the LFCG vocabulary. The PDF's "don't be a
> backseat dooter" anti-pattern is LFCG's **Asymmetry → Abilities** taken to
> degenerate length. See Appendix C.5.

### 23.1 The principle

- Agents should have ALL general formation capabilities (awareness broadcasting,
  tick reporting, environment reading, handoff participation) PLUS unique
  capabilities from their connector type.
- An agent should **never** sacrifice its primary capability to perform cooperative
  functions.
- Monster Hunter Hunting Horn is the design ideal: an agent whose primary work
  (attacking) naturally produces cooperative benefits (buffs/healing) for neighbors.
- DRG is the structural ideal: shared baseline + unique specialization.
- L4D shows that in some formation types, no specialization is correct — all agents
  interchangeable, roles purely situational.

### 23.2 Enforcement points

- `role_hint` in `composer.rs` (§3.1) biases, never mandates
- `DynamicCapabilitySet` (§16) ensures agents retain general formation capabilities
  even when their specialized capability is exhausted or transformed
- Sacrifice decision framework (§24) refuses sacrifices that eliminate a unique
  capability the formation needs

No code lives in `cooperation/specialization.rs` — it would be empty.

---

## 24. Sacrifice & Covering

> **PDF section:** §24. Distinct from recovery (§18), which helps agents already in
> trouble. Sacrifice is an agent deliberately accepting cost BEFORE failure occurs,
> to benefit the formation.

### 24.1 The four decision checks

1. **Net positive check** — does formation total output improve?
2. **Recovery path check** — is there a way back?
3. **Capability preservation check** — does the sacrifice eliminate a unique
   capability the formation needs?
4. **Momentum impact check** — does the sacrifice risk breaking formation momentum?

### 24.2 Implementation — `big-brain` utility AI

`big-brain` (zkat) is the canonical Rust utility-AI crate, Bevy ecosystem. It gives
a `Measure` trait plus four built-in combinators plus composite scorers. Decisions
are per-entity (per-agent) with no central arbiter — exactly the
cooperation-not-orchestration shape.

Verbatim from `big-brain/src/measures.rs` — the complete `Measure` trait and all
four upstream combinators:

```rust
/// A Measure trait describes a way to combine scores together.
#[reflect_trait]
pub trait Measure: std::fmt::Debug + Sync + Send {
    fn calculate(&self, inputs: Vec<(&Score, f32)>) -> f32;
}

#[derive(Debug, Clone, Reflect)]
pub struct WeightedSum;
impl Measure for WeightedSum {
    fn calculate(&self, scores: Vec<(&Score, f32)>) -> f32 {
        scores.iter()
            .fold(0f32, |acc, (score, weight)| acc + score.0 * weight)
    }
}

#[derive(Debug, Clone, Reflect)]
pub struct WeightedProduct;
impl Measure for WeightedProduct {
    fn calculate(&self, scores: Vec<(&Score, f32)>) -> f32 {
        scores.iter()
            .fold(0f32, |acc, (score, weight)| acc * score.0 * weight)
    }
}

#[derive(Debug, Clone, Reflect)]
pub struct ChebyshevDistance;
impl Measure for ChebyshevDistance {
    fn calculate(&self, scores: Vec<(&Score, f32)>) -> f32 {
        scores.iter()
            .fold(0f32, |best, (score, weight)| (score.0 * weight).max(best))
    }
}

#[derive(Debug, Clone, Default, Reflect)]
pub struct WeightedMeasure;
impl Measure for WeightedMeasure {
    fn calculate(&self, scores: Vec<(&Score, f32)>) -> f32 {
        let wsum: f32 = scores.iter().map(|(_score, weight)| weight).sum();
        if wsum == 0.0 {
            0.0
        } else {
            scores.iter()
                .map(|(score, weight)| weight / wsum * score.get().powf(2.0))
                .sum::<f32>()
                .powf(1.0 / 2.0)
        }
    }
}
```

From `big-brain/src/scorers.rs`, the `ScorerBuilder` trait plus `FixedScore`:

```rust
#[reflect_trait]
pub trait ScorerBuilder: std::fmt::Debug + Sync + Send {
    fn build(&self, cmd: &mut Commands, scorer: Entity, actor: Entity);
    fn label(&self) -> Option<&str> { None }
}

#[derive(Clone, Component, Debug, Reflect)]
pub struct FixedScore(pub f32);
```

Big-brain ships composite scorers: `SumOfScorers`, `AllOrNothing`, `ProductOfScorers`,
`WinningScorer`, `MeasuredScorer`.

Source: <https://github.com/zkat/big-brain/blob/master/src/measures.rs>

### 24.3 Mapping the four checks onto big-brain

| Our check | big-brain primitive |
|-----------|---------------------|
| Net-positive | `WeightedSum` measure over `(benefit, -cost)` scorers |
| Recovery path | `AllOrNothing` — any scorer below threshold zeros out |
| Capability preservation | `ProductOfScorers` — multiplicative, one zero kills the whole |
| Momentum impact | Individual `Scorer` feeding into `MeasuredScorer` |
| Final `SacrificeType` pick | `WinningScorer` over the four variants |

`WeightedMeasure` is especially interesting: it's an L2-norm combination
(`sqrt(sum(weight_i/W * score_i^2))`) which penalizes "one great dimension
compensating for several weak ones." Good fit for "don't sacrifice yourself because
*one* metric looks good."

### 24.4 Concrete Sacrifice

```rust
// cooperation/sacrifice.rs
pub enum SacrificeType {
    /// Accept damage/cost to protect another agent.
    /// MH aggro drawing, L4D body blocking, DRG shield.
    Covering {
        sacrificer: AgentId,
        beneficiary: AgentId,
        cost_to_sacrificer: SacrificeCost,
        benefit_to_beneficiary: BenefitDescriptor,
    },

    /// Accept task degradation to assist another agent's task.
    /// Overcooked station covering, Siege entry fragging.
    TaskDiversion {
        sacrificer: AgentId,
        abandoned_task: TaskDescriptor,
        assumed_task: TaskDescriptor,
        formation_net_benefit: f32, // must be positive to justify
    },

    /// Accept individual destruction for formation benefit.
    /// Total War screening force, Helldivers self-bombing.
    Expendable {
        sacrificer: AgentId,
        expected_recovery: Option<RecoveryAction>, // Helldivers: reinforce
        formation_benefit: BenefitDescriptor,
    },

    /// Spend own resources on formation infrastructure.
    /// DRG Gunner shield, Patapon defend rhythm.
    ResourceInvestment {
        investor: AgentId,
        resource_spent: ResourceDescriptor,
        infrastructure_created: InfrastructureDescriptor,
        beneficiaries: Vec<AgentId>,
    },
}

pub struct SacrificeCost {
    pub fuel_cost: FuelBudget,
    pub capability_reduction: Vec<CapabilityDecl>,
    pub vulnerability_increase: f32,
    pub duration: Duration,
}
```

### 24.5 Voluntary, not commanded

The sacrifice decision must be **voluntary** (agent decides based on local awareness)
not **commanded** (orchestrator orders an agent to sacrifice). Orchestrated sacrifice
is micromanagement. Cooperative sacrifice is mutual aid. This is enforced by having
the decision logic run inside each agent's own `big-brain` scorer system — the
orchestrator has no API for commanding a sacrifice.

### 24.6 Unverified alternatives

- `bonsai-bt` and `behave` (behavior trees) — not pulled in this research pass. If
  you want BT-style *sequenced* checks rather than parallel utility scoring,
  evaluate these separately.
- `mcts` crate — likely overkill for local same-tick decisions; it's for multi-step
  lookahead.

---

## 25. Dependency Summary & Honest Gaps

### 25.1 Crate dependencies to add at workspace root

```toml
# New for cooperation/
statig         = "0.4"      # §7  — hierarchical state machines (verified)
chitchat       = "0.10"     # §8  — scuttlebutt gossip, primary (verified)
foca           = "1.0"      # §8  — SWIM liveness, secondary (verified)
dashmap        = "6.1"      # §10 — sharded concurrent map (verified)
crossbeam-deque = "0.8"     # §20 — work-stealing FlexibleChain (verified)
governor       = "0.6"      # §22 — GCRA rate limiting (verified)
big-brain      = "0.22"     # §24 — utility AI scoring (verified)
typetag        = "0.2"      # §14, §16 — serializable trait objects (verified)
petgraph       = "0.8"      # §21 — knowledge graph (verified)

# Likely already present, confirm pins
tokio          = { version = "1.50", features = ["rt-multi-thread","sync","time","macros"] }
tokio-util     = { version = "0.7",  features = ["rt"] }
arc-swap       = "1.9"
rusqlite       = "0.39"     # already in springtale-store
wasmtime       = "43.0"     # already per architecture docs
openraft       = "0.9"      # used at most for Vote ordering pattern (§11)

# Phase 3 only
iroh-gossip    = "0.97"     # §19.4 — p2p topic gossip
```

#### As-built crate deviations (June 2026 reconciliation)

The list above is the *initial-research* recommendation. The shipped crate chose
different vehicles for four of these, each more faithful to the spec's own
*refined* sections (and to 2026 research) than §25.1's first pass — the
cooperative *ideas* are realized, only the crate differs:

- **`statig` → hand-rolled tier FSM** (`momentum.rs`). The §7 momentum machine is
  4 flat tiers (Cold/Warming/Hot/Fever) with linear transitions; `statig`'s value
  is *deep hierarchical* state trees, so a hand-rolled enum FSM is behaviorally
  identical with less ceremony. The §7 idea (earned capability-gating) is intact.
- **`typetag` → `dyn-clone` + name registry** (`role/registry.rs`). `typetag` rides
  `inventory`/`ctor` runtime-init hooks with real static-link fragility (the spec's
  own §25.2.10 flags this; upstream issue dtolnay/typetag#15). §21's reload design
  *specifies* rebuilding a role "from the persisted name" — exactly the registry,
  which is **more faithful** to §21 and avoids the linker bug.
- **`big-brain` → local `utility/` module**. `big-brain` is a **Bevy** crate
  (requires bevy@0.16); Springtale is not Bevy. §25.2.8 itself says the local
  `Measure`/`Scorer` traits "suffice," and §25.3 offers framework-agnostic
  `bonsai-bt`. The local module avoids a Bevy dependency while realizing §24.
- **`sled` → SQLite via `springtale-store`**. CLAUDE.md mandates all cooperation
  SQL live in `springtale-store`; the atomic `UPDATE…RETURNING` claim has the same
  exactly-once / CAS semantics §13/§20 require.

`AgentId` is a UUID, not the Bitsquid packed-u64 sketch — see the E11 note in
`cadence.rs` (cross-process + Phase 3 transport make global uniqueness cheaper).

#### Gap-closure reconciliation (June 2026, second pass)

A line-by-line audit of this document against the shipped tree closed the
remaining drift. Recorded here so the as-built shape stays auditable:

- **`Tick` carries no `IntentPattern`.** §5.3's sketch put intent inside every
  tick because it assumed a bus per formation. As built, ONE `CadenceBus` per
  bot serves every formation, so the bus is a pure metronome
  (sequence/timestamp/window) and intent travels on the §6 `FormationContext`
  watch channel. Every intent write goes through one chokepoint —
  `orchestrator::intent::apply_intent` — which also feeds the §7 momentum FSM
  an `IntentChanged` event.
- **§5.5 intent-transition source 2 is wired.** Formation self-governance is a
  consensus vote on a typed `DecisionSubject::IntentChange` (Fever-gated),
  applied by the `resolve_consensus` tick step. Academic anchor: Joint
  Intention Theory (Cohen & Levesque; STEAM) — a joint persistent goal changes
  only by mutual belief, i.e. a vote.
- **§11 consensus loop is closed.** Votes carry a typed `DecisionSubject`
  (`DestructiveAction` | `IntentChange`); `ConsensusEngine::resolve_ready()`
  sweeps quorum/override/deadline resolutions once per tick and the
  `resolve_consensus` step APPLIES them — approval mints a one-shot execution
  permit, deny/timeout removes the task. A pending-vote guard
  (`Formation::awaiting_consensus`) prevents per-tick re-proposal. Timeout on a
  destructive subject is ALWAYS a denial (default-safe), even though the engine
  itself stays As-Dusk-Falls-faithful (most-popular-wins on timeout).
- **§12 `Countdown` is live.** `CommitBarrier::with_countdown(d)` holds Ready →
  Countdown → Execute on a deadline; `Duration::ZERO` (default) preserves the
  straight Ready → Execute path.
- **§22 decision #5 constants implemented.** `INTENSITY_DECAY = 30s`,
  `RELAX_THRESHOLD = 0.99`, `SUSTAIN_PEAK_MIN/MAX = 3s/5s` in
  `pacing/manager.rs` (per §A.1.1), with decay paused while engaged. Pacing
  elapsed time is now true wall-clock between processed ticks (the old code
  reused the ×4 agent commit window as elapsed — pacing clocks ran 4× fast).
- **§22 frequency modulation is real.** `PacingManager::tick_divider()` skips
  bus ticks per phase (Peak ÷1, Active ÷2, Preparation/Disruption ÷3, Recovery
  ÷6) — L4D's "amplitude is not changed, frequency is" — replacing the unwired
  `tick_interval_modifier`.
- **Decision #6 rally falloff implemented non-spatially.** WH3's 70/×1.5 linear
  aura maps onto snapshot **Age of Information**: neighbor influence in
  `morale_target()` is weighted by `aoi_weight(last_updated.elapsed())` — full
  ≤2s, linear to zero at 3s (`awareness/types.rs`). The `MAX_CONTAGION_DISTRESSED
  = 4` cap applies to the weighted sum.
- **Decision #2 `TickId` newtype implemented.** Research re-confirmed the typed
  tick (DST practice, Rust API guidelines C-NEWTYPE); `tick::TickId(u64)`
  replaced every raw `tick_sequence: u64` — and the sweep immediately caught a
  real instance of the bug class (a tick stuffed into `payload_hash`).
- **`recursive.rs` / `subagent.rs` removed early** (originally scheduled for
  Phase 3 in §17.5): both were caller-free, and pre-launch dead code is deleted
  wholesale rather than deprecated in place.
- **Crate deps:** `springtale-cooperation` depends on `springtale-core` and
  `springtale-store` (the all-SQL-in-store rule requires the latter); the
  implementation plan's "zero internal deps" line was stale. The direct
  `rusqlite` dependency was unused and removed; `lib.rs` declares 41 modules.

### 25.2 What the Rust ecosystem does NOT give you

Flagged throughout the document. Collected here so you know what to write yourself:

1. **No L4D Director crate** (§22). Governor is the throttling primitive; the phase
   machine is bespoke.
2. **No off-the-shelf AIMD adaptive limiter** was verified against source.
   Netflix concurrency-limits ports exist but evaluate before depending.
3. **No native key TTL in sled or redb** (§20.3). Build a sweeper task.
4. **No peer (non-hierarchical) supervision crate** (§15). `JoinSet` + `Semaphore` +
   `broadcast` is the idiomatic build-it-yourself pattern.
5. **No scarce-capability-token crate** verified against source (§11.4). Object-
   capability pattern from `capnp-rpc` is the right inspiration but wasn't pulled;
   simpler approach is SQLite row locks.
6. **No mature blackboard-pattern crate** (§10.4). `dashmap + arc-swap` is the
   idiomatic substitute.
7. **`automerge` operation-level `ActionNegation`** via Lamport `OpId` ordering
   exists but `src/op_tree.rs` / `src/change.rs` were not read in this pass (§13.4).
8. **`big-brain` `Thinker` / `Actor` machinery** beyond the trait surface was not
   read (§24). The `Measure` and `Scorer` traits suffice for the four sacrifice
   checks, but if you want the full runtime (scheduling, prediction), read the rest.
9. **`rig` / `llmchain-rs` memory stores** not verified (§21.5). If you want to
   compare against the SQLite + petgraph design, read them before committing.
10. **`inventory::submit!` transitive-dep linker gotcha** (§14.3, §16.4). Affects
    typetag registries. Explicit `extern crate` required.

### 25.3 Previously-unverified references — now verified

These were docs-only in the first research pass and have since been verified against real source:

- **`ractor` 0.15 supervision** — `SupervisionEvent` enum verified verbatim from `ractor/src/actor/messages.rs`. **Important correction:** there is **no `SupervisionStrategy` enum** in ractor (unlike Akka/OTP). Supervision is callback-based via `Actor::handle_supervisor_evt`. Panics in `pre_start` bypass supervision entirely. Ractor remains parent-child by design, still the wrong shape for peer-level rally.
- **`tower::retry::budget::TpsBudget`** — verified verbatim from `tower/src/retry/budget/tps_budget.rs`. `deposit()`, `withdraw()`, sliding-window ring buffer with reserve count. Exactly the rally-token pattern for RPCs.
- **`tower::retry::Retry` + `Policy` trait** — verified verbatim from `tower/src/retry/mod.rs` and `tower/src/retry/policy.rs`. Note `S: Service<Request> + Clone` requirement; wrap non-cloneable connectors in `tower::buffer::Buffer`.
- **`capnp::capability::Client` + `ClientHook`** — verified verbatim from `capnp/src/capability.rs` and `capnp/src/private/capability.rs`. Unforgeability comes from Rust's type system (no public constructor of `ClientHook` outside the RPC machinery) + `add_ref()` being the only duplication path. Drop of `Box<dyn ClientHook>` releases the capability. **Use for scarce override tokens in §11.**
- **`automerge::OpId`** — verified verbatim from `rust/automerge/src/types.rs`: `pub(crate) struct OpId(u32, u32)` where fields are `(counter, actor_index)`. **Critical caveat:** derived `Ord` is lexicographic (counter first, then actor), NOT true Lamport ordering. Upstream has a FIXME comment acknowledging this is suspect. For §13 ActionNegation detection, combine `OpId` with an explicit op-type predicate — don't rely on derived `Ord` alone.
- **`extism` host function binding** — verified verbatim from `runtime/src/plugin.rs`. Extism is literally a thin wrapper over `wasmtime::Linker::func_new` with an `unsafe` block and raw-pointer lifetime bypass. **Recommendation: use wasmtime directly**, since Springtale's `#![forbid(unsafe_code)]` policy would have to carve an exception for extism's unsafe that it wouldn't need for direct wasmtime use.
- **`bonsai-bt`** — verified verbatim from `github.com/Sollimann/bonsai` (NOT `sachaos/bonsai` which is a Go TUI). Full `Behavior<A>` enum with `Select`, `Sequence`, `If`, `WhenAll`, `Race` variants. `tick(e, blackboard, f)` signature. Framework-agnostic, deterministic, serde-serializable. **Viable alternative to big-brain for §24** if you want sequenced BT checks rather than parallel utility scoring.
- **`rig-core::vector_store::VectorStoreIndex`** — verified verbatim. `top_n` returns `(f64 score, String id, T document)` tuples. **Confirm decision to keep rusqlite+petgraph**: rig is (1) vector-first requiring embeddings (forces AI dependency, violates NoopAdapter constraint), (2) not object-safe (uses `impl Future` in return position), (3) persistence backends pull in heavy service deps. Rig could be an optional semantic-recall adapter behind the same trait the sqlite+petgraph store implements, but not a replacement.
- **`tokio_util::sync::CancellationToken`** — verified verbatim. Tree structure: `child_token()` creates a child cancelled when parent is cancelled, child can also cancel independently. `cancelled()` future for `select!` integration. Safe to clone cheaply.
- **AIMD concurrency limits in Rust** — partial verification. `rate_limiter_aimd` (TwistingTwists, extracted from vector.dev) exists but is single-contributor v0.1.1. `tower-resilience` / `tower-resilience-ratelimiter` is actively maintained and supports both AIMD and Vegas as pluggable limit algorithms. **Recommendation: use `tower-resilience` for §22 if you want adaptive (not scripted) pacing.** **Confirmed not findable: Gradient2Limit in Rust** — only Java original (Netflix/concurrency-limits) and Go port (platinummonkey/go-concurrency-limits) exist as maintained implementations.

### 25.4 Still unverified (cited from docs only)

- `bevy_ecs` `FixedUpdate` / `bevy_time::Fixed` (§5.4) — low priority, tokio alternative is primary.
- `async-nats` 0.47.0 (§19.4) — not recommended anyway.

### 25.5 Module file layout (final)

```
crates/springtale-bot/src/cooperation/
├── mod.rs              # pub mod declarations + re-exports (no code)
├── cadence.rs          # §5  — CadenceBus, Tick, TickReport
├── formation.rs        # §6  — Formation, FormationMember, FormationContext
├── momentum.rs         # §7  — Momentum state machine (statig)
├── awareness.rs        # §8  — LocalAwareness, NeighborSnapshot (chitchat bridge)
├── attention.rs        # §9  — AttentionEconomy, AttentionBroker (ArcSwap)
├── environment.rs      # §10 — SharedEnvironment, Surface
├── consensus.rs        # §11 — ConsensusVote, VoteResolution
├── commit.rs           # §12 — two_phase_commit, CommitPhase
├── interference.rs     # §13 — detect_interference, InterferenceEvent
├── transformation.rs   # §14 — DynamicRole, RoleTransformation
├── rally.rs            # §15 — FormationRally (JoinSet + Semaphore)
├── capability.rs       # §16 — DynamicCapabilitySet, bind_linker
├── recovery.rs         # §18 — DistressSignal, RecoveryAction
├── comms.rs            # §19 — FormationBus, CommChannel
├── handoff.rs          # §20 — HandoffType, HandoffPayload
├── mental_model.rs     # §21 — SharedMentalModel, rusqlite+petgraph
├── pacing.rs           # §22 — PacingManager, PacingPhase (governor+ArcSwap)
├── sacrifice.rs        # §24 — SacrificeType, big-brain Measures
└── error.rs            # CooperationError (thiserror)
```

Every file has one focused concern. `mod.rs` is re-exports only. `lib.rs` stays a
table of contents.

---

## Appendix A — Game source verification

The PDF cites 14 games as design references. The Rust mappings above only land if those games actually implement what the PDF claims. This appendix walks each game and reports what is actually documented in primary sources (GDC talks, developer interviews, datamine repositories, reverse-engineering projects), what numeric values and data structures are publicly known, and what is **not** documented and therefore should not be cited as fact.

Every code block in this appendix is verbatim from the source listed. Every quoted phrase has a URL. Every "honest gap" is a thing the spec must not invent.

### A.1 Left 4 Dead (Valve, 2008) — used in §5, §15, §18, §19, §22

**Primary source:** Michael Booth, *"The AI Systems of Left 4 Dead,"* Valve, GDC 2009 / Stanford AIIDE-09. Slide deck at <https://steamcdn-a.akamaihd.net/apps/valve/2009/ai_systems_of_l4d_mike_booth.pdf> — read page-by-page during research.

**The four foundational data structures, named on the slides:**

- **Navigation Mesh** — walkable space, answers "Has area A been seen by actor B?", "Where is a spot near the Survivors not visible to any of them?", "How far have we traveled to reach this area?"
- **Flow Distance** — "Travel distance from the starting safe room to each area in the navigation mesh." Following an increasing flow gradient always leads to the exit. The shortest-path flow from start safe room to exit is the **Escape Route**.
- **Potentially Visible Areas** — set of nav areas potentially visible to any Survivor.
- **Active Area Set (AAS)** — "The set of Navigation Areas surrounding the Survivor team. The AI Director creates/destroys population as the AAS moves through the environment. Allows for hundreds of enemies using a small set of reused entities."

**The five population classes** (verbatim from "Structured Unpredictability in Left 4 Dead" slide):

- **Wanderers** (high freq) — "Common Infected that wander around in a daze until alerted by a Survivor"
- **Mobs** (medium freq) — "A large group (20–30) of enraged Common Infected that periodically rush the Survivors"
- **Special Infected** (medium freq) — individual Infected with special abilities
- **Bosses** (low freq) — Tank, Witch
- **Weapon Caches / Scavenge Items** — pipe bombs, molotovs, pain pills, extra pistols

**Wanderer spawn rules** (verbatim):
- "Stored as a simple count (N) in each area."
- N is randomly determined at map (re)start based on Escape Route length and desired density.
- When an area enters the AAS → create N Infected (if possible).
- When an area leaves the AAS, or a pending Mob needs more members → wanderers in the area are deleted and N is increased accordingly.
- Wanderer count N is **zeroed** when an area becomes visible to any Survivor OR when the Director is in Relax mode.

**Mob spawn timing:** "90–180 seconds on Normal difficulty."

**Where mobs come from:** "75% of Mobs come from behind, since wanderers and Special/Boss Infected are usually engaged ahead of the team."

**Boss placement:** "Positioned every N units along 'escape route' +/- random amount at map (re)start." "Three Boss events are shuffled and dealt out: Tank, Witch, and Nothing." "Successive repeats are not allowed (ie: Tank, then Tank again)."

**The Adaptive Dramatic Pacing algorithm** (verbatim from the slide titled "Adaptive Dramatic Pacing algorithm"):

> - Estimate the "emotional intensity" of each Survivor
> - Track the max intensity of all 4 Survivors
> - If intensity is too high, remove major threats for awhile
> - Otherwise, create an interesting population of threats

**How "emotional intensity" is computed** (verbatim from "Estimating the 'emotional intensity' of each Survivor"):

> - Represent Survivor Intensity as a value
> - Increase Survivor Intensity:
>   - When injured by the Infected, **proportional to damage taken**
>   - When the player becomes **incapacitated**
>   - When player is **pulled/pushed off of a ledge by the Infected**
>   - When **nearby Infected dies, inversely proportional to distance**
> - Decay Survivor Intensity towards zero over time
> - Do NOT decay Survivor Intensity if there are Infected actively engaging the Survivor

That is the complete formal specification. Booth closes the section with: *"Survivor Intensity estimation is crude, yet the resulting pacing works."* No exact decay constant, no exact damage-to-intensity coefficient, no exact peak threshold is published. **Anyone writing pseudocode more precise than the bullets above is inventing it.**

**The four-state pacing FSM** (drawn as a ring on the slide, exact names):

- **Build Up** — "Create full threat population until Survivor Intensity crosses peak threshold"
- **Sustain Peak** — "Continue full threat population for **3–5 seconds** after Survivor Intensity has peaked. Ensures minimum 'build up' duration."
- **Peak Fade** — "Switch to minimal threat population ('Relax period') and monitor Survivor Intensity until it decays out of peak range. This state is needed so current combat engagement can play out without using up entire Relax period. Peak Fade won't allow the Relax period to start until a natural break in the action occurs."
- **Relax** — "Maintain minimal threat population for **30–45 seconds**, or until Survivors have traveled far enough toward the next safe room, then resume Build Up."

**Boss encounters are NOT affected by adaptive pacing:** "Overall pacing affected too much if they are missing. Boss encounters are intended to change up the pacing anyhow."

Booth's closing line: *"Algorithm adjusts pacing, not difficulty. Amplitude (difficulty) is not changed, frequency (pacing) is."*

**Public extension surface — `DirectorOptions` field list from decompiled mutations.** L4D2 ships VScript (Squirrel) hooks via `.nut` files. The full tunable list lives in per-map scripts, not in `director_base.nut` (which only contains delegation hooks like `finaleStageList`, `OnChangeFinaleMusic`, `OnChangeFinaleStage`, and `MaxSpecials = 2`).

Real fields extracted from decompiled mutation scripts (`Stabbath/L4D2-Decompiled`, `sedol1339/l4d2`, `RimunAce/vscripts`):

| Field | Type | Example value and source |
|-------|------|--------------------------|
| `cm_CommonLimit` | int | `0` (Mutation 1/5: zero common infected). Overrides global default 30. |
| `cm_MaxSpecials` | int | `2` (Mutation 1), `8` (Hard Eight / Mutation 4/5). |
| `cm_BaseSpecialLimit` | int | Default per-class SI cap (sets all class limits). |
| `cm_DominatorLimit` | int | `1` (Mutation 1), `8` (Mutation 5). Dominator = Smoker/Hunter/Jockey/Charger. |
| `cm_SpecialRespawnInterval` | float sec | `60` (Mutation 1), `15` (Mutation 4/5). |
| `cm_NoSurvivorBots` | int bool | `1` disables bot survivors. |
| `cm_AutoReviveFromSpecialIncap` | int bool | `1` auto-revives from SI incap. |
| `cm_AllowPillConversion` | int bool | `0` blocks pills→adrenaline behavior. |
| `cm_ProhibitBosses` | int bool | Blocks Tank/Witch spawns. |
| `BoomerLimit` / `HunterLimit` / `SmokerLimit` / `SpitterLimit` / `JockeyLimit` / `ChargerLimit` | int | Per-class SI cap (`cm_` prefix optional inside DirectorOptions). |
| `DominatorLimit` | int | Non-`cm_` equivalent. |
| `SpecialInitialSpawnDelayMin` / `Max` | float sec | Initial SI spawn delay window (e.g. `5` / `30`). |
| `MobMaxPending` / `MobMaxSize` / `MobMinSize` | int | Horde sizing. `NoMobSpawns = true` also valid. |
| `FarAcquireTime` / `NearAcquireTime` | float sec | SI AI acquire timers. |
| `NearAcquireRange` | float | SI near-acquire detection radius. |
| `SurvivorMaxIncapacitatedCount` | int | Incaps before death. |
| `AlwaysAllowWanderers` | bool | Allows wandering infected regardless of horde state. |
| `TankHitDamageModifierCoop` | float | Tank melee damage multiplier (`0.5` in Mutation 1). |
| `ActiveChallenge` | int | Flags script as challenge mutation. |

**Scope precedence** (`jeremyvillanuevar/vscripts_custom`): Mode (4) → Local (3) → Map (2). The `cm_` prefix forces mutation scope so map scripts can't override. Squirrel requires `<-` for new slots, `=` only after slot exists.

Mirrors: [Stabbath/L4D2-Decompiled](https://github.com/Stabbath/L4D2-Decompiled), [sedol1339/l4d2](https://github.com/sedol1339/l4d2), [RimunAce/vscripts](https://github.com/RimunAce/vscripts), [jeremyvillanuevar/vscripts_custom](https://github.com/jeremyvillanuevar/vscripts_custom).

### A.1.1 Director tuning constants — recovered from cvar dumps + production VScripts

**Correction: Booth's "crude" language was about the *model*, not the *implementation*.** The actual L4D2 binary ships with every Director constant as a Source engine cvar or a VScript `DirectorOptions` field. They're not hidden — they're just not in Booth's GDC deck. Sources:

- [`gamerconfig.eu` L4D2 cvar dump](https://www.gamerconfig.eu/commands/left-4-dead-2/) — complete list of engine cvars with descriptions and default values
- [`LuckyServ/l4d2-luckylock-server-files`](https://github.com/LuckyServ/l4d2-luckylock-server-files/blob/master/scripts/vscripts/l4d2_diescraper3_mid_34_minifinale_promod.nut) — production VScripts showing `DirectorOptions` overrides
- [AlliedModders forum thread](https://forums.alliedmods.net/showthread.php?p=1551603) — community analysis

**Verbatim cvar names and default values:**

```
intensity_decay_time             = 30     # Seconds to decay full intensity to zero
intensity_averaged_following_decay = 20   # Seconds for time-averaged intensity to meet baseline
intensity_factor                 = 0      # How quickly intensity increases
intensity_lock                   = -1     # Locks intensity at a value (-1 = unlocked)
director_intensity_threshold     = 0      # Engine-level peak threshold (scripts override)
director_intensity_relax_threshold = 0    # Engine-level relax threshold (scripts override)
director_afk_timeout             = 45     # Seconds of AFK before intervention
```

**VScript-level overrides (from production `DirectorOptions`):**

```
IntensityRelaxThreshold   = 0.99     # All survivors must drop below 99% intensity
                                      # before PEAK FADE transitions to RELAX
RelaxMinInterval          = 30       # seconds
RelaxMaxInterval          = 45       # seconds — matches Booth's 30–45s
SustainPeakMinTime        = 3        # seconds — matches Booth's 3–5s
SustainPeakMaxTime        = 5        # seconds
RelaxMaxFlowTravel        = 50       # units — spatial distance for relax→build transition
MobSpawnMinTime           = 1        # seconds (default; scripts override to 6)
MobSpawnMaxTime           = 1        # seconds (default; scripts override to 12)
MobMinSize                = 12       # (default 10 per Booth, scripts set 12)
MobMaxSize                = 17       # (default 20–30 per Booth, scripts set 17)
MobMaxPending             = 20
ZombieSpawnRange          = 1500     # units
z_mega_mob_size           = 50
z_mob_spawn_max_interval  = 180      # seconds (Normal/Hard/Expert; Easy = 240)
```

**This closes the "crude" gap:**

| Booth said | Actual value |
|------------|--------------|
| "decay towards zero over time" | `intensity_decay_time = 30` seconds |
| "full threat population until Survivor Intensity crosses peak threshold" | `IntensityRelaxThreshold = 0.99` (intensity is on 0.0–1.0 scale) |
| "3–5 seconds after Survivor Intensity has peaked" | `SustainPeakMinTime=3, SustainPeakMaxTime=5` |
| "30–45 seconds" | `RelaxMinInterval=30, RelaxMaxInterval=45` |
| "randomized intervals between 90 and 180 seconds" for mobs | `z_mob_spawn_max_interval = 180` (Normal/Hard/Expert) |

**Intensity is a 0.0–1.0 float.** Peak fires at 1.0. Relax transition only happens when all survivors drop below 0.99. Full decay from 1.0 to 0.0 takes 30 seconds.

**Implication for Springtale §22 pacing:** adopt the same 0.0–1.0 intensity scale and the `intensity_decay_time = 30s` default. Use `IntensityRelaxThreshold = 0.99` for the peak→fade transition to prevent chatter near the peak. Three to five seconds sustain at peak before fade can begin. Relax for 30–45 seconds before rebuild.

**Still unresolved:** exact damage-to-intensity scaling coefficient. This is an input gain, not a time constant, and it's not exposed as a cvar. Likely a hardcoded magic number in the compiled binary. Booth's "proportional to damage taken" is all the published source has.

---

### A.2 Helldivers 2 (Arrowhead, 2024) — used in §15, §19, §22, §24

**Engine context (load-bearing for everything else):** Helldivers 2 runs on **Bitsquid / Autodesk Stingray**, an engine Autodesk discontinued in January 2018. There is no public SDK, no public source, and no community reverse-engineering project at the engine level. *All* the patrol-timing numbers below are community-derived from black-box empirical measurement, not Arrowhead-published.

Sources: <https://www.pcgamer.com/helldivers-2-engine-bitsquid-autodesk-stingray/>, <https://www.gamedeveloper.com/production/arrowhead-ceo-confirms-helldivers-2-was-built-on-a-dead-engine>, <https://en.wikipedia.org/wiki/Bitsquid>.

**Friendly fire — design intent only.** Pilestedt, March 2024 ([Kotaku](https://kotaku.com/helldivers-2-friendly-fire-always-on-arrowhead-ps5-pc-1851329269)):

> "The most important thing when we make games is believability […] things should be consistent in the game world and therefore, we must have friendly fire. If your bullets can kill enemies, and the enemies can kill you, then logic dictates that your bullets must also be able to kill your friends."

**There is no published hit-registration model.** No team-damage multiplier. No public information on whether self-damage and team-damage have different coefficients. Anyone claiming otherwise is speculating.

**Stratagem input system — what's public.** Konami-code-style directional sequence (cardinal directions over D-pad/WASD), a wrong input clears the buffer, then a physical "stratagem beacon" grenade marks the drop point and the orbital/Eagle payload deploys after a per-stratagem fuse. Source: [Helldivers Wiki: Stratagems](https://helldivers.wiki.gg/wiki/Stratagems), [VideoGamer on the April 2024 input glitch](https://www.videogamer.com/news/nasty-helldivers-2-stratagem-input-glitch-fix/). Buffer size, debounce timing, parser internals: not published.

**Mission pacing — community-derived from `helldivers.wiki.gg/wiki/Spawn_Mechanics`.** A "Battlefield Constant" governs patrol spawn timing. Baseline period in seconds between patrols (solo):

| Difficulty | Automaton | Terminid |
|---|---|---|
| 4 | 245 | 174 |
| 5 | 215 | 155 |
| 6 | 200 | 136 |
| 7 | 180 | 125 |
| 8 | 160 | 113 |
| 9 | 110 | 99 |

**Team multipliers (post-April 2024 patch, CLOSED):** the full linear scaling is now confirmed via Arrowhead's direct statement quoted in [PC Gamer](https://www.pcgamer.com/games/third-person-shooter/helldivers-2-devs-explain-why-solo-missions-have-more-patrols-now-the-intention-is-that-one-player-has-14th-of-the-patrols-they-had-16th/):

> "the intention is that 1 player has 1/4th of the patrols compared to 4 players, but it used to be that they had 1/6th"

Final table (relative to 4-player baseline):

| Players | Patrol multiplier |
|---------|------------------|
| 1 | 0.25 |
| 2 | 0.50 |
| 3 | 0.75 |
| 4 | 1.00 (baseline) |

Scaling is now linear. The pre-patch non-linear ×0.8333/×0.75 interval multipliers on the wiki describe the solo→party *interval* reduction, not the patrol-count ratio — both views are internally consistent now that the patrol ratio is linear.

**Area of Influence heat falloff:** full heat within 50 m of objective icon, linear falloff 50–150 m at "1% per meter."

**Heat modifiers:** standard objectives +50% in AoI; Detector Tower / Stratagem Jammer fabricators +50% per fabricator + 10% per tower (stack multiplicatively); extraction post-objective: up to **5.4× faster patrol spawns** at the pad. Primary objective complete → patrol threshold ×0.75 (~33% more frequent).

**Spatial constraints:** ~85 m "safety bubble" — patrols cannot spawn inside this radius. Intended spawn distance 125 m. Observed range 90–140 m. Players >75 m apart trigger independent spawn buckets.

**Factors with NO impact on patrols:** time spent in mission, combat engagement, stratagem usage, breaches/drops, biome, terminal interaction.

The closest official confirmation is Arrowhead's design director quoted in [PC Gamer April 2024](https://www.pcgamer.com/games/third-person-shooter/helldivers-2-devs-explain-why-solo-missions-have-more-patrols-now-the-intention-is-that-one-player-has-14th-of-the-patrols-they-had-16th/) confirming the *intended* ratio is 1/4 patrols solo vs 4-player, with non-linear scaling getting it to 1/6 until the patch. **That single quote is the only primary-source confirmation of the scaling formula.**

### A.2.1 Bitsquid/Stingray engine internals — CLOSED via author's own public materials

Although Autodesk discontinued Stingray in January 2018, **Niklas Frykholm (original Bitsquid creator) keeps his technical walkthroughs online** at [`github.com/niklasfrykholm/stingray-engine-code-walkthrough`](https://github.com/niklasfrykholm/stingray-engine-code-walkthrough) and [`bitsquid.blogspot.com`](http://bitsquid.blogspot.com/2014/08/building-data-oriented-entity-system.html). These describe the exact engine architecture HD2 inherits. The files include `10-threading.md`, `18-entities.md`, `24-multiplayer.md`, `7-data-compiling.md`, and the canonical KTH-hosted [*"Bitsquid: Behind The Scenes"*](https://www.kth.se/social/upload/5289cb3ff276542440dd668c/bitsquid-behind-the-scenes.pdf) PDF.

**Threading model** (from `10-threading.md` verbatim): *"We spawn as many [job threads] as we need to get one thread/core."* **Two pipeline threads**: main + render, with latency ≈ 2× frame time. `JobManager` is explicitly described as over-designed; Frykholm recommends *"a 1024 byte opaque blob"* for job data.

**Entity system** (from `18-entities.md` + the blog post, verbatim constants):

```
ENTITY_INDEX_BITS      = 22    // ~4M addressable entities
ENTITY_GENERATION_BITS = 8     // 256 generations before wraparound
MINIMUM_FREE_INDICES   = 1024  // free queue threshold before recycling
```

**Entity struct is a single `unsigned id` containing (22 index bits, 8 generation bits).** SoA component storage in contiguous buffers. Each `ComponentManager` owns its own storage strategy. Reuse is guaranteed spaced by ≥1024 entity churn cycles.

**This is directly portable to Springtale.** §6 Formation should consider this exact packed-ID approach for agent handles: 22+8 bits in a single `u32`, with a generation counter to detect stale references. It's the canonical fix for the "dangling handle to recycled slot" problem.

### A.2.2 Stingray network defaults — CLOSED via Autodesk-published Lua API

Autodesk's public Stingray Lua API reference at [`help.autodesk.com/cloudhelp/ENU/Stingray-Help/lua_ref/`](https://help.autodesk.com/cloudhelp/ENU/Stingray-Help/lua_ref/ns_stingray_Network.html) is still online. **Exact default values for the networking stack Helldivers 2 inherits:**

```
Network.set_max_transmit_rate              default = 0.03 s   # ≈33 Hz replication cap
Network.set_ping_send_time                 default = 1.0 s
Network.set_ping_resend_time               default = 0.5 s
Network.set_pong_timeout                   default = 60.0 s
Network.set_resend_time                    default = 0.2 s
GameSession.set_interpolation_lag_compensation  default = 0.015 s
GameSession.set_perfhud_pie_update_interval    default = 1.0 s
```

QoS API exposes `Network.enable_qos(min_peer_kbps, initial_peer_kbps, max_total_kbps)` but without published defaults. Network object/RPC types live in a `.network_config` resource referenced from `boot.package`. Client-server vs P2P is a schema flag.

**Implication for §19 Communication Protocols:** **33 Hz replication, 0.2 s resend window, 0.015 s lag compensation** are sensible defaults for agent-to-agent messaging in Springtale. Use these as initial values rather than inventing them.

### A.2.3 Why the "HD2 engine internals are unknowable" framing was wrong

Earlier research claimed HD2's engine was a closed black box. That was **wrong for the layers Frykholm himself documented before Autodesk killed Stingray**. The engine Arrowhead built on is not fully opaque — the core entity system, threading model, and network replication defaults are all published by the engine's creator. What IS closed: Arrowhead's specific modifications to the engine post-Autodesk-shutdown (how they replaced missing Autodesk support, their specific friendly-fire damage model, patrol spawn algorithm).

Sources: [Niklas Frykholm walkthrough repo](https://github.com/niklasfrykholm/stingray-engine-code-walkthrough), [Bitsquid blog: Building a Data-Oriented Entity System](http://bitsquid.blogspot.com/2014/08/building-data-oriented-entity-system.html), [Autodesk Stingray Lua API Network namespace](https://help.autodesk.com/cloudhelp/ENU/Stingray-Help/lua_ref/ns_stingray_Network.html), [Autodesk Stingray GameSession API](https://help.autodesk.com/cloudhelp/ENU/Stingray-Help/lua_ref/obj_stingray_GameSession.html), [AutodeskGames/stingray-docs](https://github.com/AutodeskGames/stingray-docs).

**Honest remaining gaps:** No published FSM equivalent to L4D's Build Up / Sustain / Fade / Relax for HD2 specifically. HD2's pacing remains spatial (AoI geometry + objective state), not emotional. Arrowhead's post-Autodesk engine modifications are not public. Bitsquid internal header constants (`game_session_config.h`, `packing.h`) aren't in any legal public repo.

---

### A.3 Army of Two (EA Montreal, 2008) — used in §9, §15, §24

**Honest preamble:** Army of Two is the most poorly documented of the games in the PDF. **No GDC talk, no published developer postmortem on the aggro system specifically, no leaked source, no reverse-engineering project.** The two Game Developer (Gamasutra) interviews with Chris Ferriera and Alex Hutchinson contain zero implementation detail about aggro. Best behavioural source: [P3anut Reviews](https://p3anut.wordpress.com/2008/03/05/army-of-two-review/).

**What's confirmed from observable behaviour:**

- The Aggrometer is a **single shared scalar** displayed as a centred bar between the two players.
- Aggro inputs: firing weapons (primary), killing enemies, weapon stats: "The higher the Aggro stat is, the faster you generate Aggro when firing." Suppressors decrease aggro; gold/chrome/bigger barrel increase it. Pistols low-base, RPGs/MGs high-base. **No exact numbers are published anywhere.**
- Visual: "You glow red as you accrue Aggro, and you fade away as your partner builds Aggro" (P3anut).

**Strongly inferred (NOT confirmed by primary source):** the simplest model consistent with observation is `A ∈ [-1, +1]` where each shot adds `weapon.aggro_rate * sign_of_shooter`, clamped, with decay toward 0 over time. **This is reconstruction, not fact. Flag any pseudocode using this model as inference.**

**Overkill mode:** activates when the bar is fully pegged. Two variants: Power Overkill ("double damage and unlimited ammunition") and Stealth Overkill ("invisible, and able to run a lot faster"). Duration **15 seconds**, then aggro resets to neutral. Source: [Army of Two Fandom: Aggrometer](https://armyoftwo.fandom.com/wiki/Aggrometer), [GameFAQs](https://gamefaqs.gamespot.com/boards/932860-army-of-two/46818797).

**Honest gaps:** exact weapon aggro values, decay function, target-selection algorithm — none published. The "single shared scalar" representation is strongly implied by the UI but unconfirmed by any developer statement.

---

### A.4 Total War (Creative Assembly, 2000–present) — used in §3, §6, §7, §8, §15, §22

**The three-layer AI is real and confirmed.** Tommy Thompson, *Evolution of War | The AI of Total War (Part 2)*, [gamedeveloper.com](https://www.gamedeveloper.com/design/evolution-of-war-the-ai-of-total-war-part-2-):

> "the unit AI that controls individual troops and keeps them in formation and on point, the combat AI that groups and sets formations to units and the campaign/diplomacy AI that conducts the turn-based strategy."

Per Thompson's series:
- **Unit AI** — artificial neural networks for reactive low-level movement.
- **Combat AI** — *Empire: Total War* adopted **GOAP** (Goal-Oriented Action Planning, F.E.A.R.-style).
- **Campaign AI** — uses a **BDI** (Belief-Desire-Intention) framework. *Rome II* switched to **Monte Carlo Tree Search** as anytime algorithm over ~800,000 hex-map deployment points. Sources: [Part 1](https://www.gamedeveloper.com/programming/the-road-to-war-the-ai-of-total-war-part-1-), [Part 3](https://www.gamedeveloper.com/design/revolutionary-warfare-the-ai-of-total-war-part-3-), [Part 4](https://www.gamedeveloper.com/design/make-peace-not-war-the-ai-of-total-war-part-4-).

**Real morale data structure — from `Frodo45127/rpfm-schemas/schema_wh3.ron`** (community-extracted Warhammer 3 game database schema, used by Rusted PackFile Manager):

The `_kv_morale_tables` table is the **Key-Value table driving land-combat morale simulation**. The full lookup-key list enumerates every tunable parameter the engine reads. The eight discrete morale bands (this is the real state machine, not designer prose):

```
ums_impetuous_threshold_lower
ums_eager_threshold_lower / ums_eager_threshold_upper
ums_confident_threshold_lower / ums_confident_threshold_upper
ums_steady_threshold_lower / ums_steady_threshold_upper
ums_wavering_threshold_lower / ums_wavering_threshold_upper
ums_shaken_threshold_lower / ums_shaken_threshold_upper
ums_broken_threshold_lower / ums_broken_threshold_upper
```

**State machine:** Impetuous → Eager → Confident → Steady → Wavering → Shaken → Broken → Shattered. Morale is an integer that starts from `morale_base`. Ships have parallel `sms_*` fields.

**Routing / rally / shatter machinery:**

```
broken_finish_base_timeout
broken_finish_timer_experience_bonus
waver_base_timeout
post_rally_no_rout_timer
shatter_after_rout_count
shatter_after_first_rout_if_casulties_higher_than
shatter_after_second_rout_if_casulties_higher_than
morale_shock_rout_timer_long / _short
morale_shock_terror_morale_threshold_long / _short
morale_shock_rout_immunity_timer
```

**Rally is a continuous influence field, not a discrete ability:**

```
general_aura_radius
inspiration_radius_max_effect_range_modifier  -- linear falloff out to aura * THIS
general_inspire_effect_amount_min / _max       -- scaled by command stars
commanding_general_alive_effect_amount_min / _max
unit_inspire_effect_amount                     -- non-commanders apply flat value
```

Full effect within `general_aura_radius`, linear falloff to `general_aura_radius * inspiration_radius_max_effect_range_modifier`.

**Tick loop:**

```
morale_base                           -- start point
minimium_increment_update_per_tick    -- floor on per-tick change
percent_update_per_tick               -- ideal % toward target per tick
```

Per tick, the engine sums all active `ume_*` (Unit Morale Effect) modifiers into a target, then lerps current morale toward target by `percent_update_per_tick`, clamped to at least `minimium_increment_update_per_tick`.

**The modifier catalogue (`ume_*` = Unit Morale Effect; `sme_*` = Ship Morale Effect)** has roughly **~90 distinct fields**. Categories with documented descriptions in the schema:

- **Casualties** (recent ≤4s, extended ≤60s, total): `recent_casualties_penalty_{6,10,15,33,50}`, `extended_casualties_penalty_{10,15,33,50,80}`, `total_casualties_penalty_{10..90}`.
- **Kill bonuses:** `blood_bonus_5/_7/_12` ("unit killed 5/7.5/12% of enemy in last 4 sec").
- **Combat state:** `losing_combat`, `losing_combat_significantly`, `winning_combat`, `winning_combat_slightly`, `winning_combat_significantly`.
- **Flanks:** `was_attacked_in_front/_flank/_rear`, `ume_concerned_flanks_exposed_single/_multiple`, `ume_encouraged_flanks_secure`, `ume_encouraged_on_the_hill`.
- **Leadership:** `ume_concerned_general_dead`, `_general_died_recently`, `_general_fled_recently`, `_captain_died_recently`.
- **Psychology:** `ume_concerned_panic`, `_surprised`, `_unit_frightened`, `_elephants_frightened`, `_horses_frightened`, `fear_effect_range`, `terror_effect_range`.
- **Contagion:** `routing_friends_effect_weighting`, `routing_enemies_effect_weighting`, `max_routing_friends_to_consider`, `max_routing_enemies_to_consider`, `routing_unit_effect_distance_front/_flank`.

**Routing cascade is explicit:** `UME_CONCERNED_FRIENDS_ROUTING = routing_friends_effect_weighting * (enemies_routing − friends_routing)` clamped to the `max_*_to_consider` limits. A broken unit routs; nearby friendlies accrue morale penalty; cascade can be capped (a single routing unit cannot cascade unbounded).

**Source:** <https://github.com/Frodo45127/rpfm-schemas> — `schema_wh3.ron`, `_kv_morale_tables`, `main_units_tables`, `_kv_naval_morale_tables`, `campaign_group_morale_effects_tables`. Editor: <https://github.com/Frodo45127/rpfm>.

**The "220 rules from Sun Tzu" claim is NOT supported by primary sources.** Dave Mark's [*Sun Tzu as an AI Design Guide?*](https://www.gamedeveloper.com/design/sun-tzu-as-an-ai-design-guide-) debunks the **framing**, not a specific rule count. Mark's actual verbatim critique:

> "The problem I have here is that this seems to be more of a marketing gimmick than anything."

> "After all, most of what Sun Tzu wrote should, in various forms, already be in game AI anyway."

> "To say Sun Tzu's ideas are unique to him and would never have been considered without his wisdom is similar to saying that no one thought that killing was a bad idea until Moses wandered down the hill."

The "220" figure does not appear in Mark's article — it comes from CA's own marketing. **The technical substance visible in `_kv_morale_tables` is the ~90 tunable morale modifiers above, NOT an explicit Sun Tzu ruleset.** Reframe any "220 rules" reference as: "CA marketed Total War's battle AI as influenced by Sun Tzu; the actual implementation in the game's database tables is a ~90-field weighted modifier system on an 8-state morale FSM."

**Mike Simpson direct architecture quote** — [GameWatcher, *Rome: Total War PC Interview*](https://www.gamewatcher.com/interviews/rome-total-war-interview/11546):

> "The AI works on several different levels and will look at more than just the things you mentioned above (such as terrain, battle objectives, weather, the visible contents of the players army etc) in order to decide on the best strategy to use at any given time. It will also respond to the player's tactics during the battle."

Simpson says "several different levels" but does not himself enumerate them. The explicit **three-layer naming** is Tommy Thompson's: *"the unit AI that controls individual troops and keeps them in formation and on point, the combat AI that groups and sets formations to units and the campaign/diplomacy AI that conducts the turn-based strategy"* (Evolution of War Part 2).

### A.4.1 Numeric defaults — complete dump recovered

**[`Shazbot/WH3-Dump`](https://github.com/Shazbot/WH3-Dump/tree/master/db/_kv_morale_tables)** mirrors the vanilla Warhammer 3 database tables as TSV. The complete `_kv_morale_tables/data__.tsv` has been fetched and is stored at `docs/intended-arch/research-sources/`. Full dump:

**State machine thresholds (the 8 morale bands)**:
```
ums_impetuous_threshold_lower  = 1.1
ums_eager_threshold_lower      = 0.9
ums_eager_threshold_upper      = 1.1
ums_confident_threshold_lower  = 0.65
ums_confident_threshold_upper  = 1.0
ums_steady_threshold_lower     = 30.0
ums_steady_threshold_upper     = 0.8     # note: unit mismatch suggests these are
                                          #  multiple different value types
ums_wavering_threshold_lower   = 0.0
ums_wavering_threshold_upper   = 16.0
ums_shaken_threshold_lower     = 12.0
ums_shaken_threshold_upper     = 32.0
ums_broken_threshold_lower     = -50.0
ums_broken_threshold_upper     = 0.0
```

The mixed units (some ratios, some absolute values) suggest CA uses these thresholds as a fuzzy overlay; don't try to rebuild an exact state machine from these alone — the game likely has clamping and normalization code around them.

**Tick loop constants** (the lerp-to-target core):
```
morale_base                       = 0.0
minimium_increment_update_per_tick = 1.0   # floor on per-tick change
percent_update_per_tick            = 0.15  # 15% toward target per tick
```

So morale moves toward its target at 15% per tick, with a minimum change of 1.0 morale units per tick.

**Routing and rally machinery (seconds)**:
```
broken_finish_base_timeout              = 180.0    # 3 minutes to rally a broken unit
broken_finish_timer_experience_bonus    = 10.0     # per experience level
waver_base_timeout                      = 25.0     # 25s before wavering → broken
post_rally_no_rout_timer                = 10.0     # 10s invulnerability after rally
shatter_after_rout_count                = 3.0      # 3 routs → terminal shatter
shatter_after_first_rout_if_casulties_higher_than  = 0.05   # 5% casualty threshold
shatter_after_second_rout_if_casulties_higher_than = 0.10   # 10% casualty threshold
morale_shock_rout_timer_long            = 14.0     # seconds
morale_shock_rout_timer_short           = 14.0
morale_shock_terror_morale_threshold_long  = 13.0
morale_shock_terror_morale_threshold_short = 13.0
morale_shock_rout_immunity_timer        = 85.0
```

**Rally is a continuous influence field** (the real numbers):
```
general_aura_radius                         = 70.0    # base rally radius (game units)
inspiration_radius_max_effect_range_modifier = 1.5    # linear falloff out to 70 × 1.5 = 105
general_inspire_effect_amount_min           = 4.0
general_inspire_effect_amount_max           = 4.0
unit_inspire_effect_amount                  = 4.0     # non-commanders
commanding_general_alive_effect_amount_min  = 0.0     # base is zero for player generals
commanding_general_alive_effect_amount_max  = 0.0
```

So **rally radius = 70 game units at full strength, falling to zero at 105 units** (70 × 1.5). The "aura" is a flat +4 morale to nearby allies when the general is alive.

**Routing contagion (the cascade coefficients)**:
```
routing_enemies_effect_weighting   = 2.5   # morale gain from enemies routing
routing_friends_effect_weighting   = 3.0   # morale loss from friends routing
max_routing_enemies_to_consider    = 5.0   # cap
max_routing_friends_to_consider    = 4.0   # cap
routing_unit_effect_distance_flank = 100.0
routing_unit_effect_distance_front = 100.0
```

Morale delta from friends routing = `3.0 × min(friends_routing, 4)` penalty; morale gain from enemies routing = `2.5 × min(enemies_routing, 5)` bonus. **The caps prevent a single routing unit from cascading the entire army.** Critical design lesson for Springtale's §15 Rally: bound the cascade contagion explicitly.

**The `ume_*` modifier catalogue — verbatim numeric values**:

Casualty penalties (recent = last 4 sec):
```
recent_casualties_penalty_6      = -6     # 6% casualties in last 4 sec
recent_casualties_penalty_10     = -12
recent_casualties_penalty_15     = -20
recent_casualties_penalty_33     = -44
recent_casualties_penalty_50     = -80
recent_casualties_shock_threshold = 25    # threshold for "shock" state
```

Casualty penalties (extended = last 60 sec):
```
extended_casualties_penalty_10  = -4
extended_casualties_penalty_15  = -6
extended_casualties_penalty_33  = -14
extended_casualties_penalty_50  = -32
extended_casualties_penalty_80  = -60
```

Total casualty penalties (cumulative over battle):
```
total_casualties_penalty_10 = -2    total_casualties_penalty_20 = -4
total_casualties_penalty_30 = -7    total_casualties_penalty_40 = -11
total_casualties_penalty_50 = -16   total_casualties_penalty_60 = -22
total_casualties_penalty_70 = -32   total_casualties_penalty_80 = -47
total_casualties_penalty_90 = -74
```

Combat state:
```
losing_combat                = -3
losing_combat_significantly  = -8
winning_combat_slightly      =  3
winning_combat               =  6
winning_combat_significantly =  8
```

Flanking/positioning:
```
was_attacked_in_front                  =  0    # no penalty for frontal attacks
was_attacked_in_flank                  = -6
was_attacked_in_rear                   = -14
ume_concerned_flanks_exposed_single    = -3
ume_concerned_flanks_exposed_multiple  = -6
ume_encouraged_flanks_secure           =  5
ume_encouraged_on_the_hill             = 10
```

Leadership:
```
ume_concerned_general_dead              = -10
ume_concerned_general_dead_ai           = -10
ume_concerned_general_died_recently     = -16
ume_concerned_general_died_recently_ai  = -16
ume_concerned_general_fled_recently     = -16
ume_concerned_general_fled_recently_ai  = -16
ume_concerned_captain_died_recently     = -2
```

Psychology:
```
ume_concerned_panic               = -50
ume_concerned_surprised           = -30
ume_concerned_unit_frightened     = -8
ume_concerned_horses_frightened   = -20
ume_concerned_elephants_frightened = -30
fear_effect_range                 = 20.0
terror_effect_range               = 5.0
```

Fatigue and situational:
```
ume_concerned_exhausted           = -6
ume_concerned_very_tired          = -2
ume_concerned_tired               =  0    # zero — tired alone has no direct effect
ume_concerned_night_battle_unprepared = -5
ume_concerned_under_friendly_fire = 0     # zero — friendly fire has no direct morale effect
ume_concerned_army_destruction    = -120
```

Charge bonus:
```
charge_bonus   = 15   # bonus morale during a successful charge
charge_timeout = 60   # seconds the bonus lasts
```

Difficulty modifiers (player-side):
```
difficulty_modifier_player_easy      =  4.0
difficulty_modifier_player_normal    =  0.0
difficulty_modifier_player_hard      = -2.0
difficulty_modifier_player_very_hard = -4.0
difficulty_modifier_ai_extra_multiplier_high = 0.8
difficulty_modifier_ai_extra_multiplier_low  = 0.4
```

Enemy morale penalty range:
```
enemy_morale_penalty_combat_power_min = 4.0
enemy_morale_penalty_combat_power_max = 32.0
enemy_morale_penalty_value_min        = -3.0
enemy_morale_penalty_value_max        = -24.0
```

Other fields:
```
cavalry_effect_range          = 20.0
enemy_effect_range            = 70.0
neighbour_effect_range        = 120.0
open_flanks_effect_range      = 120.0
warcry_effect_range           = 225.0
surprise_timeout              = 200.0
use_hitpoints_instead_of_casualties_prop_in_morale_calculation = 1.0
```

### A.4.2 Implications for Springtale's §6, §8, §15

- **Bound cascade contagion.** WH3 caps friend-routing effects at 4 and enemy-routing at 5 — a single routing unit cannot cascade an army. Springtale's rally supervisor in §15 should apply similar caps on peer-failure contagion.
- **Lerp-to-target tick update at 15% per tick** with a 1.0 floor is simple and works. Matches §7 momentum transition rate.
- **Rally is a continuous field with linear falloff**, not a binary radius. 70-unit aura, fade to zero at 105 units. Springtale's rally tokens in §15 should likewise be distance-weighted.
- **The 3-rout terminal shatter rule** with casualty gating (5% first-rout, 10% second-rout) is an elegant way to prevent infinite rallying of a doomed unit. Maps directly to §14 role transformation + §15 rally budget.
- **Friendly fire has zero direct morale penalty.** This is surprising and worth thinking about — CA decided that agent interference shouldn't directly demoralize. Springtale's §13 interference detection should consider whether agent-on-agent interference degrades formation momentum or is handled separately.

### A.4.3 The Sun Tzu claim, final verdict

Dave Mark's debunk stands. The actual morale system has **roughly 40 numeric constants** in the `ume_*`/`ums_*`/radius/timer space (full dump above). Not 220. Not organized as "rules" at all — it's a weighted-modifier summation on an 8-state FSM with caps on cascade contagion. Any "Sun Tzu" language CA used in marketing was aesthetic framing, not engineering substance.

**Honest remaining gaps on Total War:** Mike Simpson direct quote naming three layers — still only Thompson's attribution, Simpson himself used "several different levels" in the GameWatcher Rome interview. The ums_* threshold values have mixed units (some ratios, some absolutes) suggesting a non-trivial normalization layer we don't have source for.

---

### A.5 Patapon (Pyramid / Japan Studio, 2007) — used in §3, §5, §11, §19, §24

**Core tempo and command structure** — primary source: [PlayStation Blog 2017 interview with Kotani and Adachi](https://blog.playstation.com/2017/08/01/marching-to-the-beat-of-their-own-drum-an-interview-with-the-creators-of-patapon/).

- **Marching tempo = 120 BPM.** Adachi: *"Marching tempo is 120BPM."*
- **Every track in the game shares that tempo.** Adachi: *"Every track has the same tempo… Rhythm games usually try to vary the tempo to create variety. Introducing variations at the same tempo, well, it was tricky to say the least."*
- **Four-button-as-drum.** Kotani: *"Thinking about 'pata' and 'pon' as drumming noises, I envisioned drumming out 'pata, pata, pata, pon,' and the creatures would start marching."*

The four drum syllables map to PSP face buttons (□=Pata, △=Chaka, ○=Pon, ×=Don). Each command is a **4-beat measure**: drum four inputs over four beats; on the next measure the Patapons respond. **120 BPM = 500 ms/beat = 2000 ms/measure.** Measures are the unit the engine schedules on.

**Fever Mode activation** — from [Patapon Fandom Wiki: Fever_Mode](https://patapon.fandom.com/wiki/Fever_Mode):

> "Fever Mode is a mechanic which boosts offensive and defensive statistics of Patapons."
> "To reach Fever, player needs to hit 10 consecutive commands (depending on accuracy) or 3 perfect commands."

Algorithm: `fever = (combo >= 10) || (perfect_in_a_row >= 3)`. A missed command resets the combo and drops Fever.

**Class structure** (per [Patapon Fandom Wiki: Patapon_Units](https://patapon.fandom.com/wiki/Patapon_Units)): Yaripon (spear), Tatepon (shield/melee), Yumipon (bow). Squads cap at 6 each. Army = up to 18 units + Hero. Each class is a separate action module bound to the same command set — Pon-Pon-Pata-Pon ("attack") is interpreted by Yaripon as throwing spears, by Yumipon as loosing arrows, by Tatepon as charging melee.

**The rhythm is the shared-context protocol.** A single command broadcasts one intent; each class interprets it through its own action module. This is the inspiration for §3.2 IntentPattern.

**Honest gaps:**
- **No primary source for the exact input window in milliseconds.** Community guides imply "on the beat with tolerance" but no developer disclosure.
- No PSP decompilation project found. The PSP RE scene is active ([uofw](https://github.com/uofw/uofw), [PSP-RE HQ](https://psp-re.github.io/)) but Patapon-specific reverse engineering does not exist publicly. Retro Reversing's catalog of decompiled retail games does not list Patapon.
- Unit stats (HP, attack, range) are not publicly documented at field-name level.
- Fever activation differs slightly between Patapon 1, 2, 3 — no single developer-authoritative source.

---

### A.6 Crypt of the NecroDancer (Brace Yourself Games, 2015) — used in §5, §11, §12, §22

**This is the cleanest reference architecture in the entire game catalog.** Two primary sources:

**Source 1 — Ryan Clark's design rationale.** [*Game Design Deep Dive: Finding the beat in Crypt of the NecroDancer*, gamedeveloper.com 2014-09-17](https://www.gamedeveloper.com/audio/game-design-deep-dive-finding-the-beat-in-i-crypt-of-the-necrodancer-i-).

The famous 100% leeway discovery, verbatim:

> "I tried increasing this leeway value and was surprised to discover that it felt best at 100%!"

Initial leeway was 20% (at 120 BPM, ±100 ms). Final is 100% — an entire beat. The design insight Clark foregrounds:

> "the times when you are least accurate are the times when you are most stressed!"

**Source 2 — `Grimy/ChoregraphAI`**, a pure-C bug-for-bug reimplementation with Brace Yourself Games' explicit blessing. The README:

> "ChoregraphAI uses its own heavily optimized, pure C re-implementation of (the relevant parts of) NecroDancer. It can simulate millions of beats per second, with bug-for-bug accuracy."
> "Since NecroDancer isn't open-source, this was achieved by black-box reverse-engineering: set up edge-case scenarios in the game, see what happens, then find patterns."

Source: <https://github.com/Grimy/ChoregraphAI>

**The actual GameState struct, verbatim from `chore.h` lines 201–232:**

```c
struct alignas(2048) GameState {
    Tile board[32][32];          // 32x32 grid
    Monster monsters[72];         // entity array
    Trap traps[32];

    u32 seed;                     // PRNG state
    char input[32];               // Last 32 player inputs
    Coords stairs;
    u8 locking_enemies;
    u8 current_beat;              // Number of beats spent in the level
    u8 nightmare;
    u8 monkeyed;                  // ID of enemy grabbing player
    u8 mommy_spawn;
    u8 sarco_spawn;
    u8 last_monster;
    bool game_over;

    u8 bombs;
    Item shovel, weapon, body, head, feet, ring, usable, torch, none;
    CharId character;
    bool player_moved;
    bool sliding_on_ice;
    bool boots_on;
    u8 iframes;                   // Beat # where invincibility expires
};
```

**Critical observations:**

- **`u8 current_beat`** — global tick is an 8-bit beat counter, incremented exactly once per resolved beat. This is the external clock.
- **`char input[32]`** — 32-slot ring buffer of player inputs, indexed by `current_beat & 31`.
- **`u8 iframes`** — invincibility expressed as an absolute beat number, not a frame count. The sim is beat-scheduled, not ms-scheduled.
- **`alignas(2048)`** — struct deliberately aligned to 2048 bytes so snapshotting and copying is cache-friendly.

**The beat loop, `do_beat()` from `main.c`** (verbatim):

```c
bool do_beat(char input) {
    // Player's turn
    g.input[g.current_beat++ & 31] = input;
    if (setjmp(player_died)) return true;
    player_turn(input);
    if (TILE(player.pos).type == STAIRS && g.locking_enemies == 0) return true;
    update_fov();
    before_and_after();

    // Build a priority queue with all active enemies
    Monster *queue[64] = {0};
    u64 queue_length = 0;
    bool bomb_exploded = false;
    for (Monster *m = ...; m > &player; --m) {
        if (!monster_ai[m->type] || !m->hp) continue;
        m->knocked = false;
        if (!m->aggro && !check_aggro(m, player.pos - m->pos, bomb_exploded)) continue;
        if (m->type == BOMB || m->type == BOMB_STATUE) bomb_exploded = true;
        priority_insert(queue, queue_length++, m);
    }

    // Enemies' turns in decreasing priority
    for (u64 i = 0; i < queue_length; ++i) { ... }
}
```

Comment in the file:

> // Runs one full beat of the game.
> // During each beat, the player acts first, enemies second and traps last.
> // Enemies act in decreasing priority order. Traps have an arbitrary order.

**Rollback — the actual implementation, from `play.c main()`** (verbatim):

```c
GameState timeline[32] = {[0 ... 31] = g};
for (;;) {
    timeline[g.current_beat & 31] = g;
    display_all();
    i32 c = getchar();
    if (c == 't') execv(*argv, argv);
    else if (c == 'u') g = timeline[(g.current_beat - 1) & 31];     // <-- ROLLBACK
    else if (c == '\033' && scanf("[M%*c%c%c", &cursor.x, &cursor.y))
        cursor += {-33, -33};
    else if (c == EOF || c == 4 || c == 'q') break;
    else if (!g.game_over)
        g.game_over = do_beat((char) c);
}
```

**This is the cleanest rollback implementation in any rhythm-turn game:**

- Before every beat, the entire `GameState` is memcpy'd into a 32-slot ring buffer indexed by `current_beat & 31`.
- Pressing 'u' (undo) replaces `g` with `timeline[(g.current_beat - 1) & 31]` — a plain struct assignment, because `GameState` is POD with no pointers.
- Depth: 32 beats.
- Cost: one copy of `sizeof(GameState)` per beat. With `alignas(2048)`, ~2 KB per snapshot → 64 KB ring buffer total.
- This works because the sim is fully deterministic given `(seed, inputs)` — which is also why SYNCHRONY's rollback netcode is feasible.

**This is the single most load-bearing reference for Springtale's cooperation module:** *agent state as POD + beat counter + ring-buffer snapshots → rollback is one struct copy.* §5 (cadence), §12 (synchronized commit), and §15 (rally) all reduce to this pattern.

**SYNCHRONY rollback netcode** — Marukyu (Vortex Buffer), [mod.io interview](https://blog.mod.io/crypt-of-the-necrodancer-dlc-developer-interview-f9a0706d5cf7):

Marukyu describes himself as:

> "engine developer with a passion for live-code-reloading and rollback networking."

On the rewrite:

> "a live-reloadable mod system and a rollback-based networking architecture that made online sessions feel like single-player."

> The project rewrote the game "with an architecture supporting modding and multiplayer from the start."

**BPM is per-track, NOT a single game-wide default.** Community measurement via Torcularis's BPM chart on Steam Community (<https://steamcommunity.com/app/247080/discussions/4/412449431007225544/>) confirms each zone has its own BPM:

| Track | Zone | BPM |
|-------|------|-----|
| Tombtorial | Tutorial | 100 |
| Disco Descent | 1-1 | 115 |
| Crypteque | 1-2 | 130 |
| Mausoleum Mash | 1-3 | 140 |
| Fungal Funk | 2-1 | 130 |
| Grave Throbbing | 2-2 | 140 |
| Portabellohead | 2-3 | 150 |
| Stone Cold | 3-1 | 135 |
| Dance of the Decorous | 3-2 | 145 |
| A Cold Sweat | 3-3 | 155 |
| Styx and Stones | 4-1 | 130 |
| Heart of the Crypt | 4-2 | 145 |
| Wight to Remain | 4-3 | 160 |
| Metalmancy (Death Metal boss) | boss | 175 |
| Last Dance (NecroDancer P2) | boss | 160 |

Range: 100–175 BPM. Margin ~5 BPM (community measurement). **The "120 BPM default" commonly cited is only the BPM of the `Watch Your Step` training and `Absolutetion` (Golden Lute final) tracks — not a game-wide constant.** Your cooperation module must not hardcode a single BPM; it must support per-formation tempo.

**SYNCHRONY rollback API surface** — `vortexbuffer.com/synchrony/docs/` (Marukyu's own documentation) exposes these module names: `necro.client.Rollback`, `necro.client.NetClock`, `necro.game.system.Snapshot`, and cycle modules `necro.cycles.Frame`, `necro.cycles.Tick`, `necro.cycles.Turn`. The presence of `Turn` alongside `Frame`/`Tick` confirms rollback is **beat-granular (by Turn), not frame-granular** — which matches the design of the underlying step-locked game. Numeric parameters (rollback depth, input delay, snapshot byte format) remain undisclosed.

**Precise sub-beat timing model (CLOSED via Playwright fetch of the Fandom wiki).** Full Sub-beat Mechanics page verbatim at `docs/intended-arch/research-sources/ncd-subbeat2.txt`. Extracted facts:

**1. Timing window is exactly half a beat in each direction.** Verbatim:

> "The player may be up to half a beat early or half a beat late on any input."

This is Ryan Clark's 100% leeway from §A.6 in precise form: the *acceptance window* is a full beat (half before + half after), centered on each beat. Two consecutive valid inputs can therefore be made with up to a full beat between them — one made "as late as possible" and the next "as early as possible."

**2. Engine runs at 60 fps** (derived from the floor-transition constant). Verbatim:

> "Transition to the next floor in approximately 27 frames (~0.45 seconds)"

$27 \text{ frames} / 0.45 \text{ seconds} = 60 \text{ fps}$. Confirmed.

**3. Minimum input debounce = 3 frames.** Verbatim:

> "Two valid inputs can be made with only three frames (0.05 seconds) in between."

At 60 fps, 3 frames = 50 ms. This is the irreducible input spacing.

**4. Triple-input rule:** three valid inputs require at least **one full beat plus two frames** between the first and last.

**5. Per-trigger frame windows:**
- Trapdoor animation: 14 frames (233 ms)
- Travel rune: 14 frames before secret-shop loads
- Shopkeeper-aggro throw window: 14 frames

**6. BPM thresholds derived from frame math:** the wiki gives precise BPM values at which tricks become possible without sub-beat inputs. Reverse-engineering the math:

| BPM | Frames per beat (60/f·BPM·60) | Why it matters |
|-----|-----|----|
| 100 | 36 | Tutorial tempo |
| 115 | 31.3 | Disco Descent (zone 1-1) |
| 130 | 27.7 | Crypteque (zone 1-2) |
| 134 | 26.9 | **Heal-spell-on-stairs trick becomes free** (fewer than 27 frames between beats) |
| 144 | 25.0 | Double-heal trick needs half-beat frame-precision |
| 157 | 22.9 | Double-heal without frame precision |
| 160 | 22.5 | Wight to Remain (max normal tempo) |
| 258 | 14.0 | **Any trapdoor sub-beat trick becomes free** (14 frames ≥ trapdoor window) |
| 267 | 13.5 | Double-heal becomes free |
| 288 | 12.5 | **Triple-heal with Bolt becomes free** |
| 300 | 12.0 | Quarter-beat input threshold |
| 314 | 11.5 | Triple-heal without frame precision |
| 360 | 10.0 | Four-input-in-14-frames threshold |
| 403 | 8.9 | Only with custom music |

**7. Quarter-beat inputs exist.** Verbatim:

> "Two frame-perfect quarter-beat inputs (i.e. halfway between two of Bolt's beats)"

Sub-beats are a fractional-beat concept with both half-beat and quarter-beat levels possible depending on character and tempo.

**8. Damage invincibility lasts ~26–29 frames with a ~20-frame grace period** where valid inputs can be made early. Verbatim:

> "When damage to the player is blocked, the player will suffer a 'lag' lasting somewhere between ~26 and ~29 frames, during which any valid inputs made are delayed or buffered until the end of the lag. However, there also appears to be some kind of 'grace period' after somewhere around ~20 frames, where valid inputs can be made without having to wait through the last few remaining frames of lag."

**9. Character modifications to the timing model:**
- **Bolt:** doubles effective BPM (plays on half-beats)
- **Coda:** plays on half-beats (different from Bolt)
- **Bard:** has no invalid input timing — can input any time
- **Nocturna:** bat form has special rules at end-of-floor

**This gives Springtale a complete reference timing model for §5 Cadence:**
- 60 fps tick rate
- Full-beat acceptance window centered on tick (generous, matches NecroDancer insight)
- 3-frame minimum debounce between agent actions
- Per-action frame windows (rather than per-beat) for specific operations

**SYNCHRONY rollback API surface** — previously confirmed. Module names `necro.client.Rollback`, `necro.client.NetClock`, `necro.game.system.Snapshot`, `necro.cycles.Frame/Tick/Turn`. Numeric rollback depth, input delay, snapshot format still not public (would need GameMaker `.win` decomp via UndertaleModTool).

**Honest remaining gaps:** rollback depth in beats and input delay in frames — still only in SYNCHRONY binary.

---

### A.7 Monster Hunter (Capcom, 2004–present) — used in §6, §13, §18, §23, §24

**Per-part HP and stagger thresholds — real datamined values.** Source: [Kiranico](https://mhworld.kiranico.com/), the standard community datamine treated as canonical by the MH community.

> "A monster is staggered and/or flinched when sufficient damage is dealt to a monster part... While breaking a monster's horn or tail always causes a flinch, breaking a monster's legs or arms may only sometimes cause a flinch."
> "Each monster part has its own HP and depleting one bar results in a 'flinch', 'topple' or 'dunk'."
> — <https://mhworld.kiranico.com/en/guide/understanding-monster>

**Real per-part data, MH Wilds Rathalos** ([mhwilds.kiranico.com/data/monsters/rathalos](https://mhwilds.kiranico.com/data/monsters/rathalos)):

| Part | Part HP | Wound count | Stagger sequence (initial → reduction → regen) |
|------|---------|-------------|------------------------------------------------|
| Head | 500 | ×2 | 250 → 30 → 100 |
| Neck | 500 | ×0 | 250 → 30 → 100 |
| Torso | 300 | ×3 | 150 → 30 → 100 |
| Left Wing | 520 | ×3 | 260 → 30 → 100 |
| Right Wing | 520 | ×3 | 260 → 30 → 100 |
| Left Leg | 300 | ×1 | 150 → 30 → 100 |
| Right Leg | 300 | ×1 | 150 → 30 → 100 |
| Tail | 520 | ×3 | 260 → 30 → 100 |

Base monster HP: **4,500**. HRP value: **850**. The 3-number sequence is `initial threshold → minimum reduction → regeneration`. These are real datamined fields, not estimates.

**Wide-Range skill** ([Fextralife](https://monsterhunterworld.wiki.fextralife.com/Wide-Range)):

| Level | Range | Item efficacy to allies |
|-------|-------|-------------------------|
| 1 | Standard | 33% |
| 2 | Wider | 33% |
| 3 | Wider | 66% |
| 4 | Much wider | 66% |
| 5 | Much wider | 100% |

**Capcom has never published the exact Wide-Range radius in world units.** Recovery Up and Item Prolonger are NOT propagated by Wide-Range — each ally's bonus stats apply only to that ally.

**Hunting Horn melody propagation** — the only confirmable quantitative datum is Attack Up (Large) duration: base **120 seconds**, Encore extends by **+90 seconds** with the Maestro skill (giving the rolling refresh model). Source: [MHW Fandom: Horn Melodies](https://monsterhunter.fandom.com/wiki/MHW:_Horn_Melodies). Fextralife's Hunting Horn page confirms only the *mechanism* — "You can tell if a party member is within range of your Performance effects by looking at the icon next to their name" — no meter radius, no tick rate.

**Yuya Tokuda, GDC 2018 (verbatim, via Siliconera/Inven Global):**

> "This is how we were able to realize Monster Hunter: World's most ambitious change: to make it possible to use the environment."
> — [Siliconera 2018-03-22](https://www.siliconera.com/monster-hunter-world-developers-show-off-prototype-lagiacrus-gdc-2018/)

> "Our goal was to bring players into a living, breathing ecosystem by creating seamless open environments teeming with life."
> "Monsters now leave tracks and traces behind, and following them will lead players to their targets."
> — [Inven Global 2018-10-22](https://www.invenglobal.com/articles/6549/creating-a-dense-open-world-a-lecture-from-yuya-tokuda-the-director-for-monster-hunter-world)

GDC Vault session: ["Monster Hunter: World Postmortem"](https://www.gdcvault.com/play/1024981/-Monster-Hunter-World-Postmortem) — paywalled.

**Honest gaps:** Hunting Horn radius is not a published number — every community source describes it as "within range" with a UI indicator. Melody tick rate undocumented. GDC Vault talk content paywalled. No first-party Capcom source has ever been released.

---

### A.8 Deep Rock Galactic (Ghost Ship Games, 2020) — used in §6, §15, §18, §20, §23

**Primary source — Mikhail Akopyan's GDC 2023 talk:** ["Independent Games Summit: Developing a Live Game That Never Truly Left Early Access"](https://www.gdcvault.com/play/1028756/Independent-Games-Summit-Developing-a). Game Developer recap: <https://www.gamedeveloper.com/marketing/developing-a-live-game-that-never-truly-left-early-access-with-deep-rock-galactic>. Akopyan called DRG "a mixture of Minecraft and Left 4 Dead" and the Experimental Branch users "more passionate and knowledgeable about the game than the developers themselves."

**Real numeric values from `deeprockgalactic.wiki.gg`** (community wiki maintained from blueprint inspection via UAssetGUI/FModel):

**Per-enemy Difficulty Point costs** ([Swarm](https://deeprockgalactic.wiki.gg/wiki/Swarm), [Encounter](https://deeprockgalactic.wiki.gg/wiki/Encounter)):

| Enemy | DP cost |
|-------|---------|
| Grunt (Slasher/Guard) | 10 |
| Swarmer / Naedocyte Shocker | 6 |
| Exploder / Web Spitter | 25 |
| Mactera Spawn | 45 |
| Tri-Jaw / Q'ronar Youngling | 50 |
| Acid Spitter / Septic Spreader | 55 |
| Nayaka Trawler / Stingtail | 75 |
| Praetorian / Oppressor / Warden / Menace / Stalker / Grabber / Goo Bomber | 90–95 |
| Bulk Detonator | 120 |
| Shellback | 145 |

**Swarm timeline** (verbatim from wiki):

- t=3.7s: "Mission Control announces a swarm. Swarm music begins."
- t=20s: "3 spawn locations are chosen within 20m of players; this is called the Location Set. **270 base Difficulty Points** of enemies are spawned."
- Subsequent waves at 220, 120, 100, 90 base points with location resets every 10–20s.

**Caps:** max 60 active swarmers, max 60 active standard enemies, aggro limits 32 ground + 32 flying per player.

**Encounter spawn budgets** are weighted distributions bucketed by hazard:
- Hazard 1–3: 12.5% at 50–150 pts, 75% at 250–350, 12.5% at 400–500.
- Hazard 4–5: shifted to 100–500 range.
- Hazard 5.5: 33.33% at 100–200, 66.67% at 250–350.

**Resupply pod** ([wiki](https://deeprockgalactic.wiki.gg/wiki/Resupply_Pod)): costs **80 Nitra**, spawns a pod with **4 racks**; each rack restores **50% of a player's ammo (rounded up) and 50% health**, taking **4 seconds** per interaction (interruptible). The pod itself deals **1000 typeless damage in a 1.5m radius** on landing. **The "4 charges for 4 players" property is modeled as four discrete rack state objects on one pod actor; there is no per-player attribution — any dwarf can take any rack.**

**Class perks** ([Field Medic](https://deeprockgalactic.wiki.gg/wiki/Field_Medic), [Perks](https://deeprockgalactic.wiki.gg/wiki/Perks)):
- Field Medic: +15% revive speed at T1, scaling +5%/tier to +30% at T4, plus 1 instant revive per mission / 3 per Deep Dive.
- Iron Will: temporary movement/damage/slow-resist boost, once per mission.
- Resupplier max rank: 50% faster resupply, +25% extra health.

**Engine and modding:**
- DRG runs on **Unreal Engine 4.27** ([DRG Modding Handbook](https://drg-modding.github.io/docs/tools/basic-tools)).
- [`mint`](https://github.com/trumank/mint) — third-party DRG mod loader; `hook/` and `hook_resolvers/` indicate runtime patching.
- [`ArcticEcho/DRG-Mods`](https://github.com/ArcticEcho/DRG-Mods) — gameplay-parameter mods, demonstrating which values are blueprint-overridable.
- The **Nexus "Support Pods" mod** (3 shared charges, 5-min refresh, host-only) implies the base game's 4-rack model is state on a single pod actor, not a global charge pool.

**Full Enemy Count Modifier matrix (CLOSED)** — published on [`deeprockgalactic.wiki.gg/wiki/Difficulty_Scaling`](https://deeprockgalactic.wiki.gg/wiki/Difficulty_Scaling):

Normal hazards:

| Hazard | Solo | 2P | 3P | 4P |
|--------|------|----|----|----|
| 1 — Low Risk | 0.15 | 0.25 | 0.45 | 0.65 |
| 2 — Challenging | 0.25 | 0.35 | 0.65 | 0.80 |
| 3 — Dangerous | 0.50 | 0.50 | 0.80 | 1.10 |
| 4 — Extreme | 0.75 | 0.75 | 1.15 | 1.35 |
| 5 — Lethal | 0.85 | 0.85 | 1.25 | 1.50 |

Deep Dive hazards:

| Hazard | Solo | 2P | 3P | 4P |
|--------|------|----|----|----|
| 3.5 DD | 0.60 | 0.60 | 0.90 | 1.20 |
| 4.5 DD | 0.80 | 0.80 | 1.20 | 1.40 |
| 5.5 DD | 0.85 | 0.85 | 1.25 | 1.50 |

The small solo→2P delta compensates for loss of Bosco at 2 players. Enemy resistance categories: normal, large, extra large, extra large 2, extra large 3, elite — each with its own scaling curve.

**Machine-readable schema**: [`trumank/drg-custom-difficulties`](https://github.com/trumank/drg-custom-difficulties) exposes `cd.schema.json` and `DATA.md` — this is literally the JSON-friendly version of what Springtale would build for its own formation-scaling tables. **Read this repo for reference data layout before designing §7 momentum tiers.**

**Honest remaining gaps:** Akopyan's GDC talk full video content paywalled. Encounter and stationary spawn weight sub-tables (per-wave, per-biome) referenced by the wiki but not reproduced in HTML — recoverable by reading raw `.cd.json` files in the trumank repo.

---

### A.9 Overcooked / Overcooked 2 (Ghost Town Games, 2016/2018) — used in §6, §15, §20, §22

**Primary source — Phil Duncan's [Game Design Deep Dive](https://www.gamedeveloper.com/design/game-design-deep-dive-building-truly-cooperative-play-in-i-overcooked-i-).** Verbatim quotes:

- *"With Overcooked we wanted to make a game where cooperation was a central pillar."*
- *"A game which was much more focused on how a team works together rather than simply adding more players."*
- *"Kitchens have always struck me as a perfect analogy for a cooperative game: an occupation where teamwork, time management, spatial awareness and shouting are all vitally important."*
- *"Many hands make light work."*
- *"We decided to keep all players on a level playing field."* — **no station affinity; any chef can use any station.**
- *"There were generally always more actions to perform than players available."*
- *"We experimented with adding disruptions: events which would change the rules midway through a level forcing players to rethink their strategy."*
- *"Suddenly players were having to really concentrate on what every member of the team was doing and stay in constant communication."*

**Oli De-Vine, [NYU Game Center 2017 IGF interview](https://gamecenter.nyu.edu/2017-igf-interviews-overcooked/):**

- *"We wanted a game where everyone had something to do simultaneously."*
- *"Communication is a skill you need, more than any other, to succeed at Overcooked."*

**Ghost Town Games, [Push Square 2018-08-06](https://www.pushsquare.com/news/2018/08/interview_chewing_the_fat_with_overcooked_2_developer_ghost_town_games):**

- *"We've also seen people communicating in ways we weren't expecting — spinning on the spot, or throwing uncooked chickens at one another for example!"*
- *"The difficulty comes much more from a team's ability to communicate than it does with how seasoned a gamer you are."*

**Open-source clones — these are the closest thing to a reference implementation:**

- [`KitchenLib`](https://github.com/KitchenMods/KitchenLib) — open-source modding library for **PlateUp!** (commercial Overcooked-like on Unity DOTS ECS). 137 releases. Working reference for how a cooperative kitchen state is modeled as ECS components and systems. [Wiki](https://wiki.plateupgame.com/en/Modding/GettingStarted).
- [`Farfi55/CookedUp`](https://github.com/Farfi55/CookedUp) — Unity Overcooked-clone with an Answer Set Programming (ASP) bot using clingo/DLV via ThinkEngine. Its **9-state AI FSM** is explicit: *"Pick up Plate, Place Plate, Get Ingredients, Ingredient Burning, Ingredient Left on CuttingCounter, Drop Ingredient, Pick up completed Plate, Deliver, Recipe failed."* This is the closest public reference to a formal cooperative-kitchen agent model.
- [`plateupplanner`](https://github.com/plateupplanner/plateupplanner.github.io) — kitchen-layout planner exposing station adjacency and pipeline topology.

**Honest gaps:** No datamined numeric constants for Overcooked itself. Recipe timers, burn thresholds, order decay rates — none published or datamined. PlateUp's ECS code is the closest analog. Disruption system described qualitatively only; no published trigger grammar.

---

### A.10 Divinity: Original Sin 2 (Larian, 2017) — used in §10, §13, §20

**Surface enum is real, has 74 entries, lives as a compile-time engine enum.** Source: [Divinity Engine Wiki: Scripting surface types](https://docs.larian.game/Scripting_surface_types). Reported values:

```
SurfaceNone = -1
SurfaceFire = 0
SurfaceWater = 4
SurfaceBlood = 16
SurfacePoison = 28
SurfaceOil = 32
SurfaceLava = 36
SurfaceSource = 37
SurfaceWeb = 38
SurfaceDeepwater = 42
```

Variants encode modifiers via enum position: Blessed / Cursed / Purified / Frozen / Electrified. A separate cloud range (43–73) covers fire/water/blood/poison/smoke/explosion/frost/deathfog clouds.

The wiki explicitly warns: *"It is advised to use the enumeration value instead of the integer representation because this list is subject to change"* — these are compile-time enum offsets, not a data-driven registry. Confirmed by [Larian forum thread](https://forums.larian.com/ubbthreads.php?ubb=showflat&Number=626820) where a modder fails to add a new surface type because "new surface types aren't being added to the enum/array of available surface types" and `CreateSurface` throws.

**Spatial representation — the AI Grid.** [Divinity Engine Wiki: AI grid panel](https://docs.larian.game/AI_grid_panel). Surfaces are NOT a separate data structure — they live as one layer of the multi-layer AI Grid that also stores movement, projectile, and dynamics data. Per-tile state with "moveable"/"blocked" flags. "Surface combinations that are not possible for your current selection are automatically greyed out" — the combination rules are data-table-checked at paint time.

**Osiris API surface** — [`CreateSurface`](https://docs.larian.game/Osiris/API/CreateSurface):

```
CreateSurface(GUIDSTRING _Source, STRING _SurfaceType, REAL _Radius, REAL _Lifetime)
```

**Surfaces are server-authoritative, circular-brush stamped, radius-based (in metres), with lifetime in seconds (-1 = permanent).** Sister calls: `GetSurfaceSize`, `GetSurfaceTypeIndex`, `TransformSurface`, `ChangeSurfaceOnPath`.

**Reverse-engineering — `Norbyte/ositools`**, the most detail-rich primary source: <https://github.com/Norbyte/ositools/blob/master/Docs/LuaAPIDocs.md>. Surfaces are server objects with handles and NetIDs (no GUIDs — distinguishing them from entities). Engine-boundary surface actions: `ChangeSurfaceOnPath`, `CreatePuddle`, `ExtinguishFire`, `RectangleSurface`, `PolygonSurface`, `SwapSurface`, `Zone`. Note `PolygonSurface` exists — while the Osiris scripting API is circular, the engine itself supports polygonal regions. Surfaces support Damage Lists with `Add()/Clear()/Multiply()/Merge()/ConvertDamageType()/ToTable()` — **tick damage is a composable list, not a single scalar.**

**Nick Pechenin, [Fextralife interview](https://fextralife.com/divinity-original-sin-2-interview-armour-changes-mechanics-skills-types-and-more/), verbatim:**

> "We had half a dozen proposals for a new surface system (One of them contained a combo that would spawn a Shitstorm. Yes, really)."

> "The problem we kept running into was that many natural surfaces are very inert and do little besides hinder character movement. That's why we decided to add a second dimension to the existing system and make it a bit more dense, rather than wide."

> "4AP was chosen in particular because it lets us give every combat participant two significant actions or one double-action, or one-and-a-half action with a half-action per turn."

The "second dimension" quote is the closest to a design rationale for the Blessed/Cursed modifier axis.

**Surface lifetimes (CLOSED)** — from in-game testing documented at [Jarvz's Damage Interaction FAQ](https://steamcommunity.com/sharedfiles/filedetails/?id=1171813230):

| Surface | Lifetime |
|---------|----------|
| Fire | 2 turns |
| Ice | 2 turns |
| Poison | Permanent (until cleared) |
| Oil | Permanent |
| Water | Permanent |
| Blood | Permanent |

**Walk-through damage rule (verbatim):** *"Walking through a damage surface (fire/poison) causes 1/2 the tick value of the status effect per ~2 meters. Damage surface reactions also seem to do 1/2 the tick value on contact."* Example: fire tick 140 → oil-to-fire ignition reaction applies 70.

**CRITICAL — surfaces don't carry their own damage.** The real implementation is: surfaces **apply tickable statuses** (`Burning`, `Poisoned`, `Electrified`, `Frozen`) and those statuses' per-tick damage lives in `Data/Public/Shared/Stats/Generated/Data/Data.txt` (skill/status stats). Surface templates point at these statuses; they don't duplicate the damage. Verified via [LaughingLeader's Data.txt gist](https://gist.github.com/LaughingLeader/4bb72f2a44093cea0e3fee74b404fe2f) and the [Sinitar modding guide](https://www.sinitargaming.com/dos2.html).

**Recommended phrasing for the spec:** *"Surfaces apply tickable statuses (Burning, Poisoned, Electrified, Frozen) from `Stats/Data.txt`; walk-through causes ~50% tick per step; lifetimes are Fire/Ice 2 turns, others persistent."*

### A.10.1 Tick rate — CLOSED

**The Divinity 4.0 engine runs game logic at ~30 Hz, tick interval ≈33 ms.** Source: [`Norbyte/ositools` ReleaseNotesv56.md](https://github.com/Norbyte/ositools/blob/master/Docs/ReleaseNotesv56.md) — verbatim quotes: *"Normally game logic runs at ~30hz"* and *"this event is thrown roughly every 33ms."* This is the canonical tick rate of the engine Divinity Original Sin 2 ships on and that Baldur's Gate 3 inherits.

**Implications for Springtale §5 Cadence:**
- 30 Hz is a reasonable baseline for agent cooperation; not every formation needs 60+ Hz tick rates.
- At 30 Hz, with the half-beat tolerance window from NecroDancer (§A.6), one tick = ~33 ms = ~16 ms of slack each side. Generous enough for agents to commit the right action.

**SurfaceTemplate field schema** (verified via ositools Lua API): `FX`, `InstanceVisual`, `IntroFX` (visual data arrays), `CurrentLifeTime`, `LifeTime` (numbers, seconds), `Flags`, `NetID`, `OwnerHandle`, `StatsMultiplier`, `StatusSourceHandle`, `StatusType`. Surface Status fields: `BringIntoCombat`, `Channeled`, `CleansedByHandle`, `DamageSourceType`, `EsvStatusFlags0/1/2`, `ForceFailStatus`, `ForceStatus`, `Influence`, `InitiateCombat`, `IsFromItem`, `IsHostileAct`, `IsInvulnerable`, `IsLifeTimeSet`, `IsOnSourceSurface`, `IsResistingDeath`. Surface-specific statuses subclass: `EsvStatusHit`, `EsvStatusConsumeBase`, `EsvStatusHeal`, `EsvStatusAoO`, `EsvStatusClimbing`, `EsvStatusInSurface`.

### A.10.2 Data.txt full dump (`Stats/Generated/Data/Data.txt`)

**Complete dump** recovered from [LaughingLeader's gist](https://gist.github.com/LaughingLeader/4bb72f2a44093cea0e3fee74b404fe2f). Stored at `docs/intended-arch/research-sources/`. Key surface/status/elemental-related values:

```
Freeze Contact Status Duration        = 1
Burn Contact Status Duration          = 1
Stun Contact Status Duration          = 1
Poison Contact Status Duration        = 1
Chill Contact Status Duration         = 3
EntangledContactStatusDuration        = 1
SurfaceDurationFromHitFloorReaction   = 18
SurfaceDurationFireIgniteOverride     = 12
SurfaceDurationFromCharacterBleeding  = -1   # permanent
SurfaceDurationBlessedCursed          = -1
SurfaceDurationAfterDecay             = -1
SmokeDurationAfterDecay               = 6
Surface Distance Evaluation           = 2
Surface Clear Owner Time              = 1
ChanceToSetStatusOnContact            = 100
Painted surface status chance         = 100
Oiled Chance to Burn Boost            = 30
Haste Speed Modifier                  = 1.5
Slow Speed Modifier                   = 0.8
SurfaceAbsorbBoostPerTilesCount       = 10
StatusDefaultDistancePerDamage        = 0.75
SkillCombustionRadius                 = 3
Infectious Disease Depth              = 5
Infectious Disease Radius             = 5
```

**The elemental interaction table at the status-duration level is now CLOSED.** Contact durations are mostly 1 turn (fire/burn/poison/stun/freeze/entangled), except Chill = 3 turns. `SurfaceDurationFromHitFloorReaction = 18` turns for floor-reaction surfaces; `SurfaceDurationFireIgniteOverride = 12` for oil→fire ignition. Character-bleeding blood and blessed/cursed surfaces are permanent (`-1`). Walk-through damage is 50% of the tick value per Jarvz's community testing (documented in earlier pass).

### A.10.3 Pechenin additional quotes (new in this pass)

From the [Fextralife interview](https://fextralife.com/divinity-original-sin-2-interview-armour-changes-mechanics-skills-types-and-more/):

> **On the 4-AP economy rationale:** *"4AP was chosen in particular because it lets us give every combat participant two significant actions or one double-action, or one-and-a-half action with a half-action per turn."*

> **On balance philosophy:** *"We design systems to be inherently exploitable, and we don't generally nerf elements that are above the curve."*

> **On AI + crowd-control:** *"Our new advanced AI, free from the shackles of custom scripts, was also very keen on using crowd control whenever it had the chance."*

The 4-AP rationale is directly relevant to Springtale §16 Dynamic Capability Binding — Larian's design rule is "budget for two significant actions or one double-action per turn." Good default for agent tick budgets.

**Honest remaining gaps:** exact per-status tick damage values in the compiled `.pak` `StatusData.txt`. The `StatsMultiplier` field on surfaces means damage scales with caster level/attribute — there isn't a single flat number to report. The propagation algorithm (how water spreads on uneven floors, how fire jumps tiles) is compiled into engine code and not exposed in docs or ositools Lua API. The Vincke GDC 2019 talk is production/iteration postmortem, not surface internals (confirmed by RPGCodex write-up at <https://rpgcodex.net/article.php?id=11133>).

**If you own DOS2:** the Divinity Engine 2 mod kit is free and official. `Data\Public\Shared\Stats\Generated\Data\Data.txt` is plaintext — open it in any editor and the per-status tick values are right there. This is the single highest-ROI gap closure in the entire spec if you own the game.

---

### A.11 Rainbow Six Siege (Ubisoft Montréal, 2015–) — used in §10, §13, §16, §18, §19

**The destruction engine is RealBlast on AnvilNext 2.0**, NOT Snowdrop (common confusion — Snowdrop is the Massive engine used by The Division). Confirmed: [Game Developer GDC 2016 coverage](https://www.gamedeveloper.com/design/video-why-real-time-destruction-is-the-core-of-i-rainbow-six-siege-i-) and [GDC Vault entry for Julien L'Heureux's "The Art of Destruction in Rainbow Six: Siege"](https://www.gdcvault.com/play/1023003/The-Art-of-Destruction-in) (GDC 2016). L'Heureux's abstract:

> "Introducing a game-changing technology in a AAA game comes with its own set of challenges. It's not enough to develop a new technology, you need to make it play nicely with other systems in the game."

**Alexandre Remy's [Develop post-mortem](https://mcvuk.com/development-news/the-develop-post-mortem-rainbow-six-siege/), verbatim:**

> "When we rebooted in January 2013, we had just made the breakthrough of the destruction engine."
> "With the material-based destruction engine, it procedurally breaks everything down."
> "It's based on materials, which need to react logically and consistently to different stimuli."

Team was 25 devs at the January 2013 reboot.

**Two-layer destruction model** — best public reverse-engineering at <https://forums.unrealengine.com/t/procedural-destruction-rainbow-six-siege-style/81885> (community observation, not Ubisoft-authored — flag as third-party analysis):

1. **Surface layer:** small bullet holes, melee holes, stackable per-impact. Procedural texture/mesh deformation at hit point.
2. **Structural layer:** when cumulative damage crosses a threshold, **Voronoi-style fracture** produces realistic chunk separation.

This two-stage model is consistent with what L'Heureux shows in the GDC 2016 deck. **Update: the full deck has been retrieved and parsed** — `docs/intended-arch/research-sources/lheureux-gdc2016-realblast.txt`. The actual technical details follow.

#### A.11.1 Destruction model — Hierarchical Decomposition + Connection-Based Leaf Graph

Verbatim from the slides:

> "Objects are separated into different parts based on their physical material."
> "Hierarchical decomposition based on fragmentation."
> "Connection-based leaf graph — Game interacts with connections; leaf graph manages state."
> "Leaf fragments can be flagged as procedural, depending on topology. Visual and collision can change; can create new child fragments; create connections from parent's."

**This is the real data structure:** a tree of fragments where leaves are connected via a graph that manages destruction state. Gameplay interacts with the connections (which break), not with the fragments directly.

#### A.11.2 Surface procedural destruction algorithm (verbatim)

> "Developed exclusively for Rainbow 6: Siege"
> "Use arbitrary cutting polygons to cut a planar surface"
> "General 2D polygonal technique — Robust, fast, simple"

**Pipeline:**
1. 3D → 2D projection of the surface
2. Generate a cut pattern (shape depends on impact position in local space + combination of inputs and material parameters)
3. **Polygon intersection via Weiler-Atherton polygon clipping algorithm**
4. **Triangulation via Ear Clipping** ("robust, can handle multiple holes")
5. Extruded 3D mesh from the 2D surface

**Cutter classes:**
- Perimeter-only cutters: random ellipse, spline
- Inner-fragment cutters: **Voronoi**
- Both: glass, texture (continuous tileable motif mapped in UV space, then transformed to 2D surface space — artists use a tool to generate vector coordinates)

This is NOT pure Voronoi — Voronoi is one cutter class among several. The community "Voronoi-style" reverse engineering was partially right but incomplete.

#### A.11.3 Real performance budgets (verbatim from the "Destruction Budgets" slide)

| Budget | Value |
|--------|-------|
| **CPU per wall** | ~6 ms (2 procedural layers + pre-fragmented) |
| **GPU memory** | 25 MB |
| **RAM (data + engine)** | 200 MB + 150 MB |
| **Target frame rate** | 60 FPS (~16.67 ms frame budget) |

**Benchmarks table (verbatim — format is "first hit / subsequent hit" per platform):**

| Event | PC | PS4 | XB1 |
|-------|-----|------|------|
| Single bullet hole | 0.33 / 0.36 ms | 1.1 / 1.4 ms | 1.1 / 1.5 ms |
| Single explosion (drywall layer) | 1.4 / 1.9 ms | 2.8 / 4.0 ms | 3.6 / 4.9 ms |
| Single explosion (2 drywall + 2 wood layers) | 8.1 / 10.3 ms | 19.5 / 23.5 ms | 19 / 23 ms |

L'Heureux's caveat: *"To take with a heap of salt."*

**This is concrete guidance for Springtale's §10 Shared Environment memory/CPU budgeting.** A formation's shared workspace should be sized in the low tens of MB, with per-tick operations in the low single-digit ms range.

#### A.11.4 Concurrency model (verbatim)

> "Multithreading at the object-basis is trivial — Each independent sub state is MT in the simulation"
> "MT procedural destruction — Didn't go to MT algorithms, but might be a next step"
> "Asynchronicity made destruction a manageable risk. Little impact on framerate and game feel. Introduces delay in game perception vs. actual destruction state. Creates the need for an event forwarding mechanism."
> "Enables Pre-destruction — Perform destruction in advance; synchronize with end of animation"

**Time-slicing C++ macro pattern** (verbatim from the slide):

```cpp
#define START_STEP_FUNCTION(stateVar, state) \
    bool fellThrough = false; \
    switch(stateVar) { \
    case state: {

#define STEP_FUNCTION(state) \
    }; break; \
    case state: {

#define END_STEP_FUNCTION(exitVar) \
    }; break; \
    default: fellThrough = true; } \
    if(!fellThrough) return exitVar;

bool StepFunction(int& state) {
    START_STEP_FUNCTION(state, 0)
        A();
        ++state;
    STEP_FUNCTION(1)
        B();
        ++state;
    STEP_FUNCTION(2)
        C();
        ++state;
    END_STEP_FUNCTION(false)
    return true;
}
```

L'Heureux notes: *"Somewhat intrusive macros, but can easily be disabled. Multi-threaded, time-sliced code is very hard to follow and debug. Make sure you can disable it easily. If possible, make it single-threaded as well."* **This is a hard-won lesson directly relevant to §12 synchronized commit and §13 interference detection** — never build a time-sliced concurrent system you can't switch to single-threaded for debugging.

#### A.11.5 Physics and debris

> "Havok FX" for physics.
> "No procedurally cut dynamic fragments" — performance choice. "Well-placed replacements, instanced, recycled aggressively."
> "Dynamic fragments don't collide together. Vaporize fragments on explosion. Simple collision primitives (always boxes)."

Collision after destruction uses a collection of 2D convex shapes from a simplified version of the surface (remove small holes, reduce tessellation). The actual surface geometry is used for hi-resolution collision (shooting) but simplified shapes for character movement.

#### A.11.6 Determinism and rollback (THE key lesson for Springtale)

> "Gameplay feature → deterministic and replicated. Minimize bandwidth usage over CPU usage — Events (messages), States (meshes)."
> "Contract between game and destruction: We expect to be provided: The exact same inputs, In the same order."

**Same-inputs requirement:**
> "Not trivial: Race conditions between gameplay states, Network data compression even locally, **Need symmetrical compression**"

Verbatim formal statement of symmetrical compression from the slide:
$$\forall v, C(v) = c^{-1}(c(v)) \text{ where } c \text{ is compression} \Rightarrow C(v) = C(C(v))$$

**Same-order requirement:**
> "On R6:S, guaranteed by the network layer — Which is definitely the easiest solution by far. Still had to make the code not too sensitive to 'same frame' vs 'different frames' events."

**Randomness via seeded RNG on TLS (thread-local storage):**
> "Seed a RNG based on some input value. On R6:S: based on impact position. Assumes perfect replication of inputs. Store the RNG on TLS for ease-of-use. Caveat: time-slicing."

**Rollback explicitly considered and rejected:**

> "The rollback: Each client keeps track of locally applied events, reverts and re-applies when receiving other events from the host. Pros/Cons: Super robust and deterministic. Stack of events to revert is not really bounded (susceptible to latency). Each revert step is memory-intensive (full surface backup)."

**This directly informs Springtale's §12 synchronized commit design.** Siege rejected full event-log rollback because (1) the stack is unbounded under latency and (2) each revert is a full state snapshot. Springtale's cooperation module should learn from this: if you're going to ship rollback, bound the log explicitly (NecroDancer SYNCHRONY's 32-slot ring buffer is the model per §A.6) and keep snapshot state POD-minimal.

#### A.11.7 Ecosystem integration (verbatim from "The R6:S Game Ecosystem" slide)

- **Destruction Event System** — "Hint: some care needed to have asynchronous listeners"
- **AI Navigation: Navlink Update** — trapdoors and breachable walls update the navmesh dynamically
- **AI Visibility / Sound Propagation** — *"Destruction changes the acoustic of the environment drastically. Sound (& propagation) is an important feature of R6:S. AI visibility through partially broken walls."*
- **Gameplay Elements** — *"Need to have 'state-like' behavior on top of procedural destruction. Need to know when an object is broken. Ambiguous concept. Can be managed through properties (static vs dynamic) or state (triggers)."*

#### A.11.8 Official tick rate — CLOSED

Launch: **64 Hz server tick rate, but player position updates only 30 Hz.** Post Patch 2.4.1 (February 2016): **position updates bumped to 60 Hz across all platforms**. Sources:

- [Game Informer, 2016-02-10: *"Ubisoft Talks Rainbow Six: Siege Netcode"*](https://www.gameinformer.com/b/features/archive/2016/02/10/ubisoft-talks-rainbow-six-siege-netcode-clan-support-and-most-popular-operators.aspx)
- [WCCFTech: *"Rainbow Six Siege Patch 1.3 Server Tick Rate Improvement Coming To Consoles Today"*](https://wccftech.com/rainbow-siege-patch-13-detailed-server-tick-rate-improvement-coming-consoles-today/)

**Springtale implication:** a mismatch between **simulation tick rate** and **replication rate** is both normal and necessary for adversarial real-time systems. Don't pin both to the same value in §5 Cadence — allow the replication rate to be a separate configuration from the internal tick rate.

**Honest remaining gaps (greatly reduced):** exact `NvBlastBond` / `NvBlastChunk` equivalent struct layouts in RealBlast (different engine — Ubisoft's, not Nvidia's). Exact pre-fragmentation seed resolution. Propagation radius per material. Operator gadget base class hierarchy — Siege's AI has zero public tech-talk documentation.

**GDC 2018 "Intelligent Game Design on Rainbow Six Siege"** — Leroy **Athanassof** (game director, one 'f') and Geoffroy **Mouret** (game intelligence analyst). [GDC Vault](https://www.gdcvault.com/play/1025322/Intelligent-Game-Design-on-Rainbow). Abstract describes a metrics-engineered process: *"how key metrics are carefully engineered to fit the issues at hand and provide the most relevant information for each problem."* **This is a metrics/telemetry talk, NOT an operator-gadget-framework talk.** The publicly visible description does not reveal a data-driven gadget slotting framework.

**Y5S1 explosion system (CLOSED via dev blog)** — [Ubisoft Y5S1 "Explosions & Shrapnel" devblog](https://www.ubisoft.com/en-us/game/rainbow-six/siege/news-updates/1QkezaGoRkDWqcQ6duGvtk/dev-blog-explosions-shrapnel-in-y5s1):

> "Each type of explosion is defined by multiple data points, and this determines the shape of the explosion and the radius of its effects."

Frag grenades are radial; claymores are oblong.

> "These raycasts are exploratory lines that travel outward from the epicenter towards any entities and their query points within in the blast radius."

**Operators have multiple query points across their physics capsule and bounding volume** — not a single hit point. Damage uses raycast-through-destructibles:

> "Using the results of the raycasts, final damage output is determined by interpolating the damage curve with respect to your distance from the explosion."

**Critical split (verbatim):** *"Explosions commonly have two effects — destruction and damage. Damage deals damage to players and destruction is what causes any environmental destruction."* Pre-Y5S1, destructible objects capped damage radius (the "C4 outdoors > C4 indoors" bug). The Y5S1 fix: damage is reduced per destructible encountered; visual shrapnel holes indicate origin direction.

**Gadget framework (partially closed via Designer's Notes):** Ubisoft's seasonal [Y9S2](https://www.ubisoft.com/en-us/game/rainbow-six/siege/news-updates/2L0bFvjA80swkS4EvawujB/y9s2-designers-notes), [Y9S3](https://www.ubisoft.com/en-us/game/rainbow-six/siege/news-updates/1P21T5Rllq7X72E0zSGpEG/y9s3-designers-notes), [Y10S4](https://www.ubisoft.com/en-us/game/rainbow-six/siege/news-updates/1XzWbYPWo59u7NgZVDjaIm/y10s4-designers-notes) Designer's Notes reveal:

1. **Two-tier framework (Y9S2 hard rule):** *"No secondary gadget should surpass an operator's unique ability, so specialized operators remain the most efficient choice."* → **Unique Ability** (operator-defining gadget) vs **Secondary Gadgets** (shared pool: frag, smoke, claymore, impact, breach charge, stun).
2. **Signal in/out capability flags (Y10S4):** Mute's jammer was reworked from "permanent EMP around jammers" to *"affect gadgets with signals coming in or out of the radius"* — confirming gadgets have `ReceivesSignals` / `EmitsSignals` capability bits consumed by jammer logic.
3. **Spatial detection volumes (Y9S3):** *"The SPEC-IO detection area was tweaked so only the central area detects gadgets"* — gadgets register sub-regioned trigger geometry with a spatial detection subsystem.
4. **Per-operator resource pools (Y10S4):** *"Hibana's gadget allows spending resources for the right situation"* — gadgets have charge/resource pools per operator.

**Gadget function taxonomy** (wiki + Designer's Notes):

- **Breach** (hard: Hibana X-Kairos, Thermite exothermic, Maverick blowtorch, Ace SELMA; soft: frag, breach charge, impact)
- **Intel** (Twitch drone, IQ Electronic Sensor, Valkyrie Black Eye, Lion EE-ONE-D, Dokkaebi Logic Bomb)
- **Denial/Trap** (Kapkan EDD, Frost Welcome Mat, Lesion GU Mine, Ela Grzmot, Thorn Razorbloom)
- **Anti-gadget** (Thatcher EMP, Mute Signal Disruptor, Twitch Shock Drone, Jäger ADS, Wamai Mag-NET)
- **Area denial** (Smoke Gas, Capitão Asphyxiating Bolt, Goyo Vulcan)
- **Utility** (Bandit CED-1, Kaid Rtila)

**Recommended phrasing for the spec:** *"Siege gadgets follow a two-tier model (Unique Ability + Secondary pool) with per-operator resource pools, signal-in/signal-out capability flags consumed by jammer/EMP subsystems, and spatial detection volumes registered with a trigger subsystem."* Deeper architecture (gadget base class, virtual function set, capability-bit layout) remains closed.

**Honest remaining gaps:** Numerical constants in RealBlast (shard counts per wall, voxel resolution, physics step rate, chunk mass, pre-fragmentation seed resolution, propagation radius per material) exist only in the GDC 2016 image-only PDF at `media.gdcvault.com/gdc2016/Presentations/LHeureux_Julien_Art_Of_Destruction.pdf` — requires OCR or the Vault video. Gadget base class source not public. See **Appendix B.6 (Nvidia Blast + Unreal Chaos + Teardown)** for open-source destruction alternatives with real code.

---

### A.12 Splinter Cell (Ubisoft Montréal, 2005–2013) — used in §3, §12, §15, §18, §20

**Patrick Redding GDC 2011 talk is real and on GDC Vault.** [Game Developer write-up](https://www.gamedeveloper.com/design/video-how-to-encourage-cooperative-behavior-during-co-op-play). [Co-Optimus summary](https://www.co-optimus.com/article/5783/splinter-cell-conviction-s-co-op-design-core-principles-to-encourage-co-op-behavior.html). The talk distinguishes **"player cooperation" vs "systemic cooperation"** and covers:

- **Gating** — "locking confined areas until they're cleared by both players"
- **Difficulty scaling when separated** — "overwhelming players when they're caught alone"
- **Buddy revival**
- **Mark-and-execute as assistance mechanic** — "one player's actions make another player more powerful"

### A.12.1 Redding slide deck RECOVERED via Wayback Machine — CLOSED

**Source:** [Wayback Machine mirror](https://web.archive.org/web/2014/https://holesinteeth.typepad.com/blogginess/files/Patrick_Redding_Design_KeepItTogether.pdf) of Redding's dead Typepad blog. 45 MB PDF (978 lines extracted) stored at `docs/intended-arch/research-sources/redding-gdc2011-keep-it-together.txt`.

**Redding defines "player cooperation" explicitly on slide 003:** title is *"Player cooperation"* with subtitle *"(as opposed to systemic cooperation)"*. He then argues for player cooperation through five principles (each its own slide):

1. *"Negotiated actions reinforce social interaction"*
2. *"Players become invested in the success of collaborative partners"*
3. *"Players respond to collective agency at work in the game space"*
4. *"shared intentionality"*
5. *"Players will work together to optimize system output"*

**The Detection-Model example slide** illustrates: *"Two-player stealth is fragile"* and *"2x the players ≠ 2x the chances for detection."* The insight is that shared intentionality lets the AI treat the team as one entity.

**Self-expression hierarchy (slide):** High-level = Develop strategies · Mid-level = Create plans · Low-level = Make risky choices · Mastery = Explore optional paths.

**"Meaningful cooperation" definition (slide, two parts both required):**

1. *"Serious, important or useful to the player's success in the game"*
2. *"Has a recognizable function in the logic of the game systems"*

### A.12.2 Redding's 7 design tools — the whole framework

Slide "Cooperative dynamics" lists seven tools on a **prescriptive → voluntary spectrum**:

| Tool | Position | Verbatim description |
|------|----------|----------------------|
| **Gating / tethering** | Very prescriptive | *"No player proceeds until all players proceed"* — bump/snap main loop |
| **Exotic challenges** | Moderately coercive | *"Altered camera/controls for some of the players"*, *"Risks associated with playing separately grow sharply over time"*, *"More than one player needed to avert trouble"* |
| **Punitive systems** | Moderate | *"One player is trapped"*, *"Requires rescue by another player to survive"*, *"Negative feedback for everyone"*, *"Can be avoided"* |
| **Buffing systems** | Voluntary | *"One player makes another mechanically more powerful"*, *"Could be passive or intentional"*, *"Benefits are conditional, temporary"*, *"Players can choose whether or not to opt in"* |
| **Asymmetric abilities** | Voluntary | *"Players have different sets of game actions"*, *"Might be orthogonal classes or customization system"*, *"Players can't max out"* |
| **Combined actions** | Voluntary | (covered later in deck) |
| **Survival / attrition** | Voluntary | (covered later in deck) |

**Redding explicitly anchors the talk in MDA.** Verbatim:

> *"Dynamics in the MDA sense: The run-time behavior of the mechanics acting on player inputs and each other's outputs over time."*
> — (2004, Hunicke, LeBlanc and Zubek, 8kindsoffun.com)

### A.12.3 "Bump/snap" loop — the key idiom

Redding draws each tool as a small state-machine main loop with a drift/reunite rhythm. Gating/tethering's diagram:

```
                         Players drift apart
              main loop                           bump/snap
                         Players reunite
```

**Exotic challenges:**
```
                  Modified camera/controls
             main loop                    threatened
                  Protected by teammate
```

**Punitive systems:**
```
                  Player(s) isolated from team
             main loop                        trap
                  Rescued by teammate
```

**Buffing systems:**
```
                  Player buffs teammate
             main loop                 overpowered
                            Times out
```

**Asymmetric abilities:**
```
                  New challenge type
             overpowered           underpowered
                       Players reorient
```

**The "bump/snap" idiom is worth stealing wholesale.** Elastic period where players can drift, then hard snap-back when they exceed the tether. Springtale §6 Formation should use this as formal semantics for formation membership: members can drift in local decision-making but hit a snap-back when they exceed intent/momentum bounds.

### A.12.4 1:1 mapping from Redding's framework to Springtale modules

| Redding tool | Springtale analog |
|--------------|-------------------|
| Gating/tethering | §6 Formation membership snap-back + §22 Pacing drift thresholds |
| Exotic challenges | §14 Role transformation (modified controls = new role) + §20 Handoff dependency |
| Punitive systems | §15 Rally & Cascade Recovery (trap → rescue) |
| Buffing systems | §9 Attention Economy (one agent makes another more powerful, times out) |
| Asymmetric abilities | §23 Specialization + §16 Dynamic Capability Binding |
| Combined actions | §12 Synchronized Commit |
| Survival/attrition | §7 Momentum (fuel depletion) + §18 Recovery |

**This is a 1:1 structural mapping.** Redding's entire 2011 GDC framework fits onto Springtale's module layout with no axis slack. The deck has been sitting behind a dead blog for a decade and is now recovered.

### A.12.5 Chaos Theory engine correction confirmed

**Mark-and-Execute in co-op:** [Wikipedia: Conviction](https://en.wikipedia.org/wiki/Tom_Clancy%27s_Splinter_Cell:_Conviction) and the Splinter Cell Wiki confirm Archer and Kestrel **share each other's marks**. Either player can Execute the other player's marked enemies. Marks are a per-player token pool earned via stealth melee kills. **Dual Execute** requires both players to have marked targets; when one Executes, time slows and both resolve simultaneously. **Shared selection set with owner-tagged entity handles.**

**Revive window in Conviction co-op: 60 seconds** ([Co-Optimus review](https://www.co-optimus.com/review/454/page/3/splinter-cell-conviction-co-op-review.html)). **Not 30.** During bleed-out the downed player can either stay still or "sit up with their pistol armed."

**Chaos Theory co-op level design** — [Vincent Barrières's portfolio page](https://vbarrieres.com/?page_id=199) (principal level designer on Chaos Theory) describes his "Tri-Path" map, verbatim:

> "There are holes in the walls of corridors so the spies can fire in the corridor of another player spy to help him by deactivating an alarm for example."

> "one or two spies will have to distract the mercenary while another spy pirates the computer."

He confirms the level was built on **Unreal Engine 2.5**.

**CRITICAL CORRECTION — Kismet does NOT exist in UE2.5.** Kismet is a UE3 feature introduced in 2006 (Gears of War, UT3). Chaos Theory shipped in **March 2005** on UE 2.5 using **UnrealScript states** on trigger actors, not visual scripting. Any earlier claim that co-op triggers were "Kismet-scripted" is wrong. The Chaos Theory editor + 3DSMax plugin is on ModDB (<https://www.moddb.com/games/tom-clancys-splinter-cell-chaos-theory/downloads/sc-chaos-theory-map-editor>) — asset extraction via UModel is the only path to the actual UnrealScript source for co-op sequences.

**Boost / hangover / stand-on-shoulders implementation — honest gap.** No Ubisoft-authored primary source states whether boost is IK, scripted animation on both characters, or physics-attached. Given UE2.5 + UnrealScript, **the strong inference** (mark as inference, not fact) is that boost is a **paired scripted animation triggered by a hand-placed trigger volume**, with each Pawn snapped to anchor transforms via a UnrealScript `state` block. There is no IK-blending published. **Do not write pseudocode claiming IK or physics without a source.**

**Open-source alternative for §15 / §20 (paired animations):** See Appendix B.12. **Naughty Dog publicly moved *away* from strict paired animations** in "Unsynced: The Last of Us Melee System" (Anthony Newman, GDC 2014) because paired animations broke whenever geometry/physics disagreed. The UE5 **Contextual Animation System** (tutorial: <https://vorixo.github.io/devtricks/contextual-anim/>) is the only production-quality open-source reference. Consider dropping the Splinter Cell reference entirely in favor of the "unsynced" model.

The Reid Schneider / Richard Carrillo Unreal Engine blog interview did not surface in research. If it exists, do not cite without verification.

---

### A.13 It Takes Two (Hazelight Studios, 2021) — used in §14, §16, §23

**Engine: Unreal Engine 4.** Released 26 Mar 2021 across Windows/PS4/PS5/XB1/XSX; Switch port 4 Nov 2022. Source: [Wikipedia](https://en.wikipedia.org/wiki/It_Takes_Two_(video_game)).

**Critical technical fact: It Takes Two is scripted in AngelScript via Hazelight's open-source UE plugin.** From <https://angelscript.hazelight.se/>:

> "Angelscript performs significantly better than blueprint for game scripting, and approaches native C++ performance when using transpiled scripts in a shipping build."

**Plugin capabilities:** hot-reload scripts without editor restart, non-structural code changes reloadable mid-Play-In-Editor, VS Code LSP with autocompletion/diagnostics/debugging, breakpoint debugging.

**This is the load-bearing fact for §14 and §16:** the "new tools each chapter" capability swap is implemented as **AngelScript hot-reloadable game scripts** in a UE4 shell, not blueprint reparenting and not C++ recompilation. The plugin is **open source** and used by additional studios beyond Hazelight.

**Active mod community:**
- [`Lemuura/It-Takes-Two-Mods`](https://github.com/Lemuura/It-Takes-Two-Mods) — "Various individual angelscript mods for It Takes Two"
- [`Lemuura/ITTAS-Installer`](https://github.com/Lemuura/ITTAS-Installer/releases)
- [Nexus Mods category](https://www.nexusmods.com/games/ittakestwo/mods) including a Performance Fix mod.

**Josef Fares verbatim** ([VGC retrospective](https://www.videogameschronicle.com/features/interviews/josef-fares-revisits-it-takes-two-i-still-think-to-myself-this-is-a-fing-good-game/)):

> "everything has to be rendered twice in a split-screen"

> "We don't have many great co-op games...it's literally A Way Out that started doing that"

[Xbox Wire 2025-03-05](https://news.xbox.com/en-us/2025/03/05/split-fiction-josef-fares-interview/):

> "It's about taking this to the next level, then the next level – what can we do to keep the players on their toes."

> "I do feel sometimes that cool moments like that wouldn't have been as cool if we just reused them all the time."

**Audio architecture:** split-screen audio mix is also split — left/center/right speakers used as base with front-over-rear preference. [Audiokinetic Q&A](https://www.audiokinetic.com/en/blog/behind-the-sound-of-it-takes-two-a-qa-with-the-hazelight-audio-team/).

**Sales:** 27M+ units ([VGChartz](https://www.vgchartz.com/article/466750/it-takes-two-sales-top-27-million-units-a-way-out-tops-12-million-units/)).

**A Way Out (2018)** is the same studio's prior cooperation-only game. EA Originals deal. ~30–35 devs. ([Wikipedia](https://en.wikipedia.org/wiki/A_Way_Out_(video_game)), [EA news](https://www.ea.com/news/coop)). No formal Gamasutra postmortem found.

**Per-chapter capability inventory (CLOSED)** — from [It Takes Two Fandom: Power and Abilities/Weapons](https://ittakestwo.fandom.com/wiki/Power_and_Abilities/Weapons):

| Chapter | Cody | May |
|---------|------|-----|
| **1. The Shed** | Nails (throw + whistle back, 1→3 active) | Hammerhead (smash obstacles, swing from nails, trigger switches) |
| **2. The Tree** | Sap cannon "Tree Sap Habschaiki 57" (explosive sap, weight, boat motor) | "DrillBazzer X200" matchstick gun (detonates Cody's sap) |
| **3a. Rose's Room — Spaced Out** | Cosmic-inflation belt (large/normal/small) | Space boots (walk on walls/ceiling) |
| **3b. Rose's Room — Hopscotch** | Fidget spinner (glide) | Fidget spinner (glide) |
| **3c. Rose's Room — Dungeon Crawler** | Wizard — ice blasts, short teleport | Knight — sword, flaming dashes, fire bursts |
| **4. The Cuckoo Clock** | Stopwatch (control time direction on objects) | Wristwatch (holographic clones of self) |
| **5. The Snow Globe** | Red magnet half (pull opposite / push same; self-launch) | Blue magnet half (inverse polarity) |
| **6. The Garden** | Hair grappling hook + plant merges: Dandelion (glide), Cactus (needles), Flower (stretch, leaf platforms), Moss (quiet movement), Mushroom (bounce), Tomato/Potato/Lime (roll attack) | Sickle + water hose (water plants, dry soil, kill infection) |
| **7. The Attic** | Cymbal (shield + thrown projectile) | Harmonic singing voice (pacify mic snakes, shatter glass, move obstacles via microphones) |

This is the complete set of chapter capability swaps — **the PDF's "It Takes Two dynamic capability reassignment" claim is fully documented for the first time**, and each swap maps to one `DynamicCapabilitySet::transformed_capabilities` state change in §16.

**Organizational structure — Hazelight works in "pods".** From 80.lv and GameRant Fares interviews: Hazelight organizes development around **pods of 8 people** — two designers, two programmers, two artists, two animators — with *"each pod owning a few levels with fairly little outside help."* This matches what a "chapter" means technically: **each pod owns a cluster of chapters and writes its own AngelScript level content independently.** There's no central "chapter manager" framework — chapters are content-owned by pods, not described as a shared data structure.

**AngelScript dev scale:** Hazelight's own status page states both It Takes Two and Split Fiction shipped *"with the majority of their gameplay written in AngelScript"* and is used daily by *"30+ developers."* Split Fiction specifically has *"over 1,700,000 lines of AngelScript across 16,000+ script files."* It Takes Two's scale is not published but can be extrapolated: ~60% of Split Fiction's would be ~1M lines / ~10K files.

**Audio architecture:** the only Hazelight GDC-adjacent technical talk is the Audiokinetic Wwise case study. For It Takes Two, the biggest Wwise extension was *"changing how spatialized signals were combined when handled by multiple listeners; they created custom 'Spatial Panning' by modifying Wwise's low-level systems."* Source: <https://www.audiokinetic.com/en/blog/behind-the-sound-of-it-takes-two-a-qa-with-the-hazelight-audio-team/>. This is the ONLY It Takes Two technical disclosure Hazelight has published beyond the AngelScript plugin itself.

**Honest remaining gaps:** Hazelight has not published the actual AngelScript source for chapter transitions. The open-source AngelScript UE plugin (<https://angelscript.hazelight.se/>) means **if you own the game**, unpacking the `.pak` and reading the `.as` files directly is trivial — every chapter's capability swap is literally a named AngelScript file. Start with [`Lemuura/It-Takes-Two-Mods`](https://github.com/Lemuura/It-Takes-Two-Mods) for the unpack pipeline. This is the second-highest-ROI RE target in the spec.

---

### A.14 As Dusk Falls (Interior Night, 2022) — used in §11

**Voting system primary source:** [Xbox Wire 2022-07-11](https://news.xbox.com/en-us/2022/07/11/as-dusk-falls-multiplayer-and-accessibility-features/), verbatim:

> "Up to 8 players who own the game can cooperatively (or antagonistically) play through the story."

> "the choice with the most votes wins, deciding the outcome from moment to moment."

> Tied votes → "the game randomly selects the winning option"

> "Players can also use special overrides when they disagree with the group."

> "If a player overrides a decision, the decision they voted for is automatically chosen."

**Override timing:** **10 seconds standard, 20 seconds with accessibility setting**. All votes are weighted equally. Counter-overrides: "other players may counter-override if they disagree" — multiple overrides can stack on the same choice.

**Override token count:** **default 3 per player, customizable 0–9** ([TheGamer](https://www.thegamer.com/as-dusk-falls-multiplayer-guide/)).

**Replenishment — source conflict:** TheGamer says "per game"; Marchal/Desodt in [DualShockers interview](https://www.dualshockers.com/as-dusk-falls-interview-caroline-marchal/) say *"When you run out of overrides, you can't use them again until the next chapter"*. Developer-side source favors per-chapter replenishment.

**Player count:** 4 controllers max on-screen + up to 8 total with companion phones. Companion app: iOS 12.0+ / Android 4.4+, same Wi-Fi/LAN, join by code.

**The companion app is an INPUT DEVICE, not a display.** "Change Input Device" in-game reveals the pairing code.

**Broadcast Mode:** Twitch chat votes via hashtag (#3, #4, etc.); host handles all QTEs; works with local co-op only.

**48 possible endings across 13 major/secondary characters; 8 characters can live or die** ([GameSpot](https://www.gamespot.com/articles/as-dusk-falls-choices-and-endings-all-character-deaths-and-best-ending/1100-6505859/)).

**Decision points labeled "Crossroads"** in-game. Post-chapter visualization: decision/outcome tree with community percentages. Each Book ends with a **Values / Traits / Play Style** summary computed from accumulated flags.

**Caroline Marchal verbatim** ([The Loadout interview](https://www.theloadout.com/as-dusk-falls/interview)):

> "We've got 1,200 pages of scripts. It's like the equivalent of ten to 12 films."

**Marchal on branch pruning** ([DualShockers](https://www.dualshockers.com/as-dusk-falls-interview-caroline-marchal/)):

> "There are some moments where we're like 'there are too many variables now, so [character] was sacrificed.'"

This implies an internal flag system governing branch pruning, but the data structure is not disclosed.

**Interior Night lineage:** Founded November 2017 by Caroline Marchal. Marchal was **lead game designer on Heavy Rain and Beyond: Two Souls** at Quantic Dream. [Wikipedia](https://en.wikipedia.org/wiki/Interior_Night), [NME Boss Level feature](https://www.nme.com/features/gaming-features/boss-level-2022-caroline-marchal-as-dusk-falls-interior-night-3254807).

**Distinction from Quantic Dream's mocap pipeline** — Marchal verbatim ([DualShockers](https://www.dualshockers.com/as-dusk-falls-interview-caroline-marchal/)):

> "We don't have really animators, we have more 2D artists who paint over each frame... everything grayscale - then we go and shoot those shots with real actors in live action."

> "Actors are not models and that's not how you get the best performance... their work was quite similar to what they do for traditional live-action."

**Engine: Unity + Timelines (confirmed).** Secret 6 case study (<https://secret6.com/case-studies/as-dusk-falls>): *"The game was built in Unity with Timelines being used to assemble the content of fragments."* `UnityEngine.Timeline` is a Unity-native construct — this is a real technical disclosure, not marketing. The branching narrative is assembled at runtime from Timeline tracks stitched together per chapter. Art pipeline: 3D environments in Unity + "meticulously hand-painted over 12,000 frames of still images" with live-action rotoscoped references.

**GDC 2023 talk on the narrative graph:** *"A Narrative Multiverse: The Branching Structure of As Dusk Falls"* by **Brad Kane** (not Marchal), GDC Vault session 1028903. Abstract frames it as a *"complex story tree made of meaningful and elegant narrative structures"* and uses a "Christmas tree" metaphor — the branching story is *"more like a Christmas tree, with the base wider than the tip where you start the story, but with points where things come back."* The full video is paywalled on Vault; abstract is public.

**Override replenishment CORRECTION — it's per-game, not per-chapter.** Re-reading both Xbox Wire and The Loadout: both sources consistently say *"available to each player per game"*. The earlier "source conflict" between Xbox Wire and Marchal was not real — no Marchal quote contradicts the per-game reading. **3 default overrides, host-configurable 0–9, one pool for the entire playthrough.**

**Marchal on branch pruning** ([DualShockers](https://www.dualshockers.com/as-dusk-falls-interview-caroline-marchal/), verbatim): *"There are some moments where we're like 'there are too many variables now, so [character] was sacrificed.'"* Confirms internal flag system governs branch pruning, but data structure is not disclosed.

**Honest remaining gaps:** narrative graph schema, flag count, per-choice consequence representation — none public. No As Dusk Falls modding scene exists. For the open-source equivalent of As Dusk Falls's override/vote/consequence model, see **Appendix B.10 (Ink `VariablesState.cs`)** — it is a direct semantic match with open, auditable code.

---

### A.15 Academic papers cited by the PDF

**Pais et al., CHI 2024 — Living Framework for Cooperative Games (LFCG)** — verified.

Full citation: Pedro Pais, David Gonçalves, Daniel Reis, João Cadete Nunes Godinho, João Filipe Morais, Manuel Piçarra, Pedro Trindade, Dmitry Alexandrovsky, Kathrin Gerling, João Guerreiro, André Rodrigues. "A Living Framework for Understanding Cooperative Games." *Proceedings of the 2024 CHI Conference on Human Factors in Computing Systems (CHI '24)*, May 11–16 2024, Honolulu. Paper 220. DOI **10.1145/3613904.3641953**. <https://dl.acm.org/doi/10.1145/3613904.3641953>

**Abstract verbatim** (via search snippet):

> "Playing cooperative games is recognised as a positive social activity. Yet, we have limited means to rigorously define or communicate the structures that govern these experiences, hindering attempts at consolidating knowledge and limiting the potential of design efforts. In this work, we introduce the Living Framework for Cooperative Games (LFCG), a framework derived from a multi-step systematic analysis of 129 cooperative games with contributions of eleven researchers."

**Three top-level taxonomy divisions:**
1. **Play Structure** — progression, group formation, goal types.
2. **Player Context** — identity, relationships, world view, viewpoint.
3. **Forms of Cooperation** — arrangement, synchronicity, communication patterns.

**Seven Cooperation Design Pattern categories** (from <https://www.lfcooperativegames.com/>):

1. Dependencies — Task, Grouping, Spatial, Temporal, Fixed/Scaling Difficulty.
2. Affecting Others — Assistive Actions, Manipulating Entities, Piggy-Backing.
3. Resource Sharing — Consumables, Unlockables, Interactables, Playable Characters, Space.
4. Asymmetry — Information, Abilities, Usefulness.
5. Relations Between Player Actions — Synergies, Complementarity.
6. Communication by Design — Agnostic, Limited, Required/Incentivised.
7. Means of Communication — Voice Chat, Text Chat, Pings, Pins, Drawings, Voice Lines, Emotes, In-Game Actions.

**Corpus:** 129 cooperative games. Framework published as interactive web app.

**Toups, Kerne, Hamilton — SIGGRAPH Sandbox 2009** — verified. Phoebe O. Toups, Andruid Kerne, William Hamilton. "Game design principles for engaging cooperative play." *Proceedings of the 2009 ACM SIGGRAPH Symposium on Video Games (Sandbox '09)*, pp. 71–78. DOI **10.1145/1581073.1581085**. Affiliation: **Interface Ecology Lab, Texas A&M University**. PDF: <https://ecologylab.net/research/publications/SIGGRAPH-GAMES-cooperativePlayToups.pdf>.

**Key principles:** information distribution, modulating visibility, providing the right information at the right time, predictable and understandable representations **for shared mental models**.

**Cannon-Bowers et al. 1993** — verified.

Full citation: Cannon-Bowers, J. A., Salas, E., & Converse, S. A. (1993). *Shared mental models in expert team decision making.* In N. J. Castellan Jr. (Ed.), **Individual and Group Decision Making: Current Issues** (pp. 221–246). Hillsdale, NJ: Lawrence Erlbaum Associates. APA PsycNet record: <https://psycnet.apa.org/record/1993-98047-012>.

**Core claim:** a team is most effective when members share **four mental models**:
1. Equipment Model
2. Task Model
3. Team Interaction Model
4. Team Model

This four-model taxonomy is the canonical construct subsequent team-cognition literature cites. The Toups 2009 "shared mental models" language descends from this lineage.

**§21 of this document maps SharedMentalModel onto these four models.**

---

## Appendix B — Open-source reference implementations

The PDF and Appendix A lean heavily on closed-source games. Many of those games implement mechanics that **other games, engines, and academic projects have built openly** with published code and formulas. This appendix lists the open alternatives per mechanism. Use them when you want readable code to base an implementation on, and when you want the spec to cite something auditable rather than a paywalled GDC talk.

**Rule of thumb:** for every mechanism, Appendix A gives you the *design origin* (what the closed game was trying to accomplish) and Appendix B gives you the *implementation reference* (open code you can read and port).

### B.1 Adaptive Director state machine — L4D Booth PDFs + Payday 2

**Primary open reference (same team as L4D):** Mike Booth's GDC 2009 slide decks are **public**, not paywalled:

- [`ai_systems_of_l4d_mike_booth.pdf`](https://steamcdn-a.akamaihd.net/apps/valve/2009/ai_systems_of_l4d_mike_booth.pdf) on steamcdn
- [`GDC2009_ReplayableCooperativeGameDesign_Left4Dead.pdf`](https://cdn.akamai.steamstatic.com/apps/valve/2009/GDC2009_ReplayableCooperativeGameDesign_Left4Dead.pdf) on Akamai Steam CDN

These are the **primary source** for the four-state model. Use them directly; don't paraphrase.

**Secondary open reference — Payday 2 `GroupAIStateBesiege.lua`.** Overkill's assault director is a state machine with **verbatim state names**: `control` (stealth) → `anticipation` (shortly before wave) → `build` (assault ramp-up) → `sustain` (assault ongoing) → `fade` (wind-down). The canonical file is unpacked from PAYDAY 2's Lua scripts; mirrors:

- [`JamesWilko/Payday-2-BLT-Lua`](https://github.com/JamesWilko/Payday-2-BLT-Lua) — BLT Lua loader base
- [`GABRlEL/payday2.pw-Mods`](https://github.com/GABRlEL/payday2.pw-Mods) — mod source collection
- [Assault States mod on modworkshop](https://modworkshop.net/mod/19391) — documents the five-state machine

Grep for `_upd_assault_task` and `_assault_phase` in the unpacked scripts. **This is the cleanest open-source implementation of an L4D-style director with readable state transitions.**

**Academic reference — PaceMaker DSL.** [Geheeb et al., *PaceMaker: A Practical Tool for Pacing Video Games*, arXiv:2408.15001 (2024)](https://arxiv.org/pdf/2408.15001). Platform-independent state-diagram tool for authoring pacing, formalizing the L4D Director pattern as a reusable state-machine DSL. **Directly relevant to §22** — if you design a bot rule DSL for pacing, read this paper first.

**Supporting papers (real citations):**

- Yannakakis & Togelius, *Experience-Driven Procedural Content Generation*, IEEE TAAC 2011. [PDF](https://yannakakis.net/wp-content/uploads/2015/11/PID3821875.pdf). The EDPCG framework: affective/cognitive modeling → content adjustment in real time.
- Pedersen, Togelius & Yannakakis, *Modeling Player Experience for Dynamic Difficulty Adjustment*, IEEE CIG 2009.
- Zohaib, *Dynamic Difficulty Adjustment (DDA) in Computer Games: A Review*, Advances in HCI 2018. DOI [10.1155/2018/5681652](https://onlinelibrary.wiley.com/doi/10.1155/2018/5681652).
- Shaker, Yannakakis & Togelius, *Towards automatic personalized content generation for platform games*, AIIDE 2010.

### B.2 Swarm / horde spawn budgeting — DRG + KF2 + CoD Zombies + Minecraft

**Primary — Deep Rock Galactic Difficulty Points.** Already detailed in Appendix A.8. The community wiki at [`deeprockgalactic.wiki.gg/wiki/Swarm`](https://deeprockgalactic.wiki.gg/wiki/Swarm) + [`/Encounter`](https://deeprockgalactic.wiki.gg/wiki/Encounter) + [`/Difficulty_Scaling`](https://deeprockgalactic.wiki.gg/wiki/Difficulty_Scaling) publishes the full per-enemy point costs, wave budgets, and enemy count modifier matrix. **Machine-readable schema:** [`trumank/drg-custom-difficulties`](https://github.com/trumank/drg-custom-difficulties) with `cd.schema.json` + `DATA.md`.

**Killing Floor 2 Controlled Difficulty — real configurable knobs.** [`notblackout/kf2-controlled-difficulty`](https://github.com/notblackout/kf2-controlled-difficulty) exposes the internal director as configurable fields:

```
MaxMonsters     — concurrent ZED cap, default 16 solo / 32 multiplayer
CohortSize      — max simultaneous spawn per cycle
SpawnMod        — 0.0–1.0, 1.0 = player-friendly, 0.75 = hostile, 0 = continuous
SpawnPoll       — update interval, vanilla = 1s
WaveSizeFakes   — artificial player count (increases both budget and wave count)
ZTSpawnMode     — "unmodded" or "clockwork" (clockwork applies slowdown during zed time)
```

Wave size formula: `ZEDs = BaseAmount × WaveSizeModifier × WaveLengthModifier`. Wave length by player count: 1p=1.0, 2p=2.0, 3p=2.75, 4p=3.5, 5p=4.0, 6p=4.5, 7+ linear up to **10.0 cap**.

**CoD Zombies round formula** (community reverse-engineered, zombacus.com / callofdutyzombies.com forum 143744):

```
Solo:  zombies(R) = 0.0842·R² + 0.1954·R + 22.05
2p:    zombies(R) = 0.1793·R² + 0.0405·R + 23.187
```

**Concurrent cap:** 24 solo, +6 per additional player (30/36/42 for 2/3/4p). Spawning pauses when cap is reached.

**Minecraft raid system — open source since Mojang released mappings.** [`minecraft.wiki/w/Raid`](https://minecraft.wiki/w/Raid) publishes:

- **Waves:** Easy=3, Normal=5, Hard=7 (+1 with Bad/Raid Omen II+)
- **Cooldown:** 300 ticks between waves
- **Spawn attempts:** **8 attempts every 5 ticks** = 480 total checks per cooldown window
- **Spawn radius phases:** phase 1 = 64 blocks, phase 2 = 32 blocks, phase 3 = 0 blocks from raid center; 20 attempts per phase
- Wave composition tables per difficulty verbatim on the wiki

Code lives in `net.minecraft.world.entity.raid.Raid` (decompile via `hube12/DecompilerMC` using Mojang's released proguard mappings).

**DayZ `cfgeventspawns.xml`** — the cleanest open data-driven spawner schema. Official [`BohemiaInteractive/DayZ-Central-Economy`](https://github.com/BohemiaInteractive/DayZ-Central-Economy). Each event declares:

```xml
<event name="...">
    <nominal>N</nominal>          <!-- target live count -->
    <min>M</min>                  <!-- minimum before respawn -->
    <max>X</max>                  <!-- hard cap -->
    <saferadius>S</saferadius>    <!-- no spawn within S of players -->
    <distanceradius>D</distanceradius>  <!-- spawn distance from player -->
    <cleanupradius>C</cleanupradius>    <!-- auto-despawn beyond C -->
</event>
```

**Directly portable to Rust TOML.** Use this as the template for §22 event/pacing spawner configuration.

**Warframe level scaling — fully published math.** [`wiki.warframe.com/w/Enemy_Level_Scaling`](https://wiki.warframe.com/w/Enemy_Level_Scaling) publishes the exact smoothstep formulas:

```
Current = Base × (1 + Coeff × (Level − BaseLevel)^Exponent)
```

Split by smoothstep interpolation at Δ ∈ [70,80]:

| Stat | f1 (below Δ=70) | f2 (above Δ=80) |
|------|-----------------|-----------------|
| Health Grineer/Scaldra | `1 + 0.015·Δ^2.12` | `1 + 10.7332·Δ^0.72` |
| Health Corpus | `1 + 0.015·Δ^2.12` | `1 + 13.4165·Δ^0.55` |
| Health Infested | `1 + 0.0225·Δ^2.12` | `1 + 16.1·Δ^0.72` |
| Shields Corpus | `1 + 0.02·Δ^1.76` | `1 + 2·Δ^0.76` |
| Armor (all) | `1 + 0.005·Δ^1.75` | `1 + 0.4·Δ^0.75` |
| Damage (most) | `Damage Mult = 1 + 0.015·Δ^1.55` | (same) |

**Cleanest reference-grade math for §7 momentum tier scaling.**

### B.3 Morale / routing / rally — 0AD-Morale-System + Spring/BAR

**Primary — 0AD-Morale-System JS mod.** [`github.com/azayrahmad/0AD-morale-system`](https://github.com/azayrahmad/0AD-morale-system). Real JS code implementing Total War-style morale on top of 0 A.D.'s open simulation engine. Verbatim from `simulation/components/Morale.js`:

```js
Morale.prototype.OnHealthChanged = function(msg) {
    let cmpHealth = QueryMiragedInterface(this.entity, IID_Health);
    let maxHp = cmpHealth.GetMaxHitpoints();
    let diff = this.GetMaxMorale() * (msg.to - msg.from) / maxHp;
    if (diff > 0) this.IncreaseMorale(diff); else this.ReduceMorale(-diff);
};

Morale.prototype.OnAttacked = function(msg) {
    if (msg.attacker && this.GetMoraleLevel() === 1) {
        let cmpUnitAI = Engine.QueryInterface(this.entity, IID_UnitAI);
        if (cmpUnitAI && !cmpUnitAI.IsFleeing())
            cmpUnitAI.PushOrderFront("Flee", {"target": msg.attacker, "force": true});
    }
};
```

Constants: 5-level scale, `moraleRegenTime = 1000ms`, `desertTime = 30000ms`, `penaltyRateWorker = 0.7`, `bonusRateAttack = 0.8`. Morale delta is proportional to HP delta — this is the lerp-to-target pattern Total War's `_kv_morale_tables` hides behind binary data.

**Spring RTS / Beyond All Reason — XP and cohesion formulas** ([`springrts.com/wiki/Modrules.lua`](https://springrts.com/wiki/Modrules.lua)):

```
XP gain for damage: 0.1 * experienceMult * damage / target_HP * target_power / attacker_power
XP gain for kill:    0.1 * experienceMult * target_power / attacker_power
Reload multiplier:   reloadScale * (1 + xp / (xp + 1))   // asymptotic, never 2x
```

Higher-XP units become preferred targets (aggro scales with power), creating organic routing pressure. Repo: [`beyond-all-reason/Beyond-All-Reason`](https://github.com/beyond-all-reason/Beyond-All-Reason).

**OpenRA veterancy** — two traits `GainsExperience` + `GivesExperience`. XP thresholds in units-of-cost: a $300 unit needs 600XP (= killing $600 of enemies) for rank 1. Defaults in per-mod `defaults.yaml`.

**Wesnoth Zone of Control** — turn-based, uses ZoC instead of morale: enemy units cannot pass adjacent hexes regardless of remaining movement. Good reference for "cohesion = movement restriction" rather than "cohesion = lerp-morale." Repo: [`wesnoth/wesnoth`](https://github.com/wesnoth/wesnoth). Architecture overview: <https://aosabook.org/en/v1/wesnoth.html>.

### B.4 Elemental surfaces / reaction chains — Noita + CDDA + Powder Toy + DCSS

**Primary — Noita `<Reaction/>` XML schema** ([`noita.wiki.gg/wiki/Documentation:_Reaction`](https://noita.wiki.gg/wiki/Documentation:_Reaction)). Datamined from `data/materials.xml`. **This is the cleanest declarative spec for elemental interaction in any game.**

```xml
<Reaction input_cell1="water" input_cell2="lava"
          output_cell1="stone"  output_cell2="steam"
          fast_reaction="0" probability="100"/>
```

Fields: `input_cell1/2/3`, `output_cell1/2/3`, `probability` (0–100), `req_lifetime`, `blob_radius1/2`, `blob_restrict_to_input1/2`, `convert_all`, `direction` (top/bottom/left/right/none), `ExplosionConfig`, `fast_reaction`. Tags like `[water]`, `[corrodible]`, `[flammable]` target material families — same abstraction as DOS2's "surface flags." **Directly portable to a Rust data structure.**

**Noita GDC 2019 talk** — [Petri Purho, *Exploring the Tech and Design of Noita*, GDC Vault 1025695](https://www.gdcvault.com/play/1025695/). Free on the official GDC YouTube channel. Scaling falling-sand to continuous worlds + integrating rigid bodies.

**Cataclysm: Dark Days Ahead `field_type.json`** — declarative, moddable. [`github.com/CleverRaven/Cataclysm-DDA`](https://github.com/CleverRaven/Cataclysm-DDA). Real values:

```json
"fd_fire":  { "half_life": "30 minutes", "intensity_levels": 3, "light": [20, 60, 160] }
"fd_smoke": { "half_life": "2 minutes",  "phase": "gas", "intensity_levels": 3 }
"fd_acid":  { "half_life": "2 minutes",  "phase": "liquid", "priority": 2 }
```

Field implementation: `src/field.cpp` (`field::add_field`, `field::remove_field`, `field_entry::do_decay`). Spread: `src/map_field.cpp`. **This is the tick-based half-life decay model Springtale wants for §10 environment surfaces.**

**Powder Toy `FIRE.cpp`** — [`github.com/The-Powder-Toy/The-Powder-Toy`](https://github.com/The-Powder-Toy/The-Powder-Toy), `src/simulation/elements/FIRE.cpp`. Verbatim constants:

```
HeatConduct = 88, Advection = 0.9, AirLoss = 0.97, Gravity = -0.1
Life: 120–169 frames
Default temp: R_TEMP + 400 + 273.15 K
Ignition probability: elements[rt].Flammable + (sim->pv * 10.0f)   // pressure-boosted
State transitions: <625K → SMOKE; >=2773K → PLASMA
On contact with PT_WATR|PT_DSTW|PT_SLTW → delete self (extinguished)
```

**258 elements as of Feb 2025, each a separate C++ file.** GPL-licensed. Reference for per-cell update loops.

**Dungeon Crawl Stone Soup `cloud.cc`** ([`github.com/crawl/crawl`](https://github.com/crawl/crawl), `crawl-ref/source/cloud.cc`):

```cpp
static int _spread_cloud(const cloud_struct &cloud) {
    const int spreadch = cloud.decay > 30 ? 80 :
                         cloud.decay > 20 ? 50 : 30;
```

Cloud damage: `NORMAL_CLOUD_DAM = { 6, 16, true }` → 6 + random2avg(16, 2) monster, 10 + random2avg(23, 2) player. Steam generated when fire magic crosses water tiles; rules in `spl-clouds.cc`.

### B.5 Procedural destruction — Nvidia Blast + Unreal Chaos + Teardown

**Primary — Nvidia Blast SDK (BSD-3).** [`github.com/NVIDIAGameWorks/Blast`](https://github.com/NVIDIAGameWorks/Blast). Open replacement for Nvidia APEX. Three layers:

- `NvBlast` — low-level chunk hierarchy + support graph
- `NvBlastTk` — toolkit wrapper
- `NvBlastExt` — extensions (`ExtAuthoring` for offline Voronoi fracture, `ExtStress` for stress solver, `ExtPhysX` for physics integration)

**Core model:** `NvBlastChunk` forms a hierarchy (parent + child indices). Fracturing a parent instantiates its children. Support chunks from different hierarchical depths can co-exist in one support structure. `NvBlastBond` connects support chunks in a **support graph**; breaking enough bonds on a subgraph triggers island detection → subgraph separates as a new rigid body. `ExtAuthoring` pre-computes the hierarchy via Voronoi + plane slicing; runtime is pure graph traversal. Docs: [nvidia-omniverse.github.io/PhysX/blast/](https://nvidia-omniverse.github.io/PhysX/blast/).

**Unreal Chaos Destruction** — [dev.epicgames.com/documentation/en-us/unreal-engine/chaos-destruction-overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/chaos-destruction-overview). Same abstraction under different names: **Geometry Collection** = chunk set; **Connection Graph** = support graph; `UClusterUnionComponent` auto-connects particles from overlapping collections. Public docs, fully readable.

**Teardown / Tuxedo Labs — Dennis Gustafsson's blog** [`blog.voxagon.se`](https://blog.voxagon.se/):

- *"Cracking Destruction"* (2014-05-13) — half-edge data structure with 16-bit indices, breaks at impact point (five bounding planes randomized around hit), trigger `impulse > threshold`. **Explicitly rejects Voronoi** in favor of impulse-threshold slicing at hit point. Sacrifices physical accuracy for visual appeal.
- *"The Spraycan"* (2020-12-03) — 8-bit voxel material palettes, 255 materials per voxel. **One byte per voxel for material ID** — directly relevant to §10 memory budgeting.
- *"Smashing Tech"* (2014-04-03) — early Smash Hit post.

**Voro++ 3D Voronoi library** — [`github.com/chr1shr/voro`](https://github.com/chr1shr/voro), LBNL, C++. The open building block Blast's ExtAuthoring uses conceptually. Paper: Rycroft, *"VORO++: A three-dimensional Voronoi cell library in C++"*, Chaos 19, 041111 (2009).

**Red Faction Guerrilla "stress" algorithm** (documented via secondary coverage only, no primary slides): every brick has `force` (weight) and `strength` (mass it can hold). Layers summed top-down; when `sum(strength) < sum(force_above)` a layer fails and everything above detaches. **Top-down vertical stack reduction**, simpler than Blast but enough for buildings. Coverage: [gmtk.substack.com *"How Games Do Destruction"*](https://gmtk.substack.com) and [redfactionwiki.com](https://redfactionwiki.com).

**Honest gap — no mature Rust-native fracture crate.** Bevy ecosystem has `bevy_voxel_world`, `voxelis`, `bevy_meshem` but none ship destruction physics. For Rust you'd FFI-wrap voro++ or Jolt.

### B.6 Per-part HP / stagger / posture — Kiranico + Souls datamines + OpenMW

**Monster Hunter — Kiranico (primary open reference).** [`mhworld.kiranico.com/en/guide/understanding-monster`](https://mhworld.kiranico.com/en/guide/understanding-monster) and [`/en/guide/damage`](https://mhworld.kiranico.com/en/guide/damage). Publicly browsable per-monster per-part tables:

- Every monster has hidden per-part HP bars (head, tail, wings, legs)
- Flinch value = damage to flinch/break (hidden per weapon, independent of visible attack stat)
- Effective flinch = `flinch_value × Quest.PartBreakability × part_multiplier`
- Hitzone values (HZV) modulate damage: `final_damage = raw × HZV_phys/100 + element × HZV_elem/100`
- Separate HZVs for sever / blunt / shot / elemental (fire/water/thunder/ice/dragon)

**Open-source tools that read MH data directly:**
- [`SmartHunter` overlay (MIT)](https://www.nexusmods.com/monsterhunterworld/mods/793) — reads decrypted chunk files, exposes the `Part` struct
- [`MHWMasterDataUtils` (GitHub: TanukiSharp)](https://github.com/TanukiSharp/MHWMasterDataUtils) — direct parser; best open reference for the data model

**Data model** (reverse-engineered via SmartHunter): `Part { hp, flinch_value, breakable_flag, hzv_physical, hzv_elemental }`. **This is the cleanest concrete specification of Monster Hunter's per-part HP system in any public source.**

**Sekiro posture (datamined)** — from the Nexus Sekiro Calculator mod #2255 and fextralife. Second HP bar that regenerates; when full, character staggered (deathblow available). **HP-gated regen table:**

| HP% | Player regen | Enemy regen |
|-----|-------------|-------------|
| 100–75 / 100–80 | 100% | 100% |
| 75–50 / 80–60 | 75% | 60% |
| 50–25 / 60–40 | 50% | 30% |
| 25–0 / 40–0 | 25% | 10% |

Corrupted Monk boss override: 100/37.5/25/4.1%. Regen delay: ~1 second after last posture damage. Max posture base +30 per Prayer Bead (40 beads → 420 max).

**Dark Souls 3 poise — stack formula:**

```
α + β − (α·β / 100)     // diminishing returns per piece
Stagger Point = PoiseHealth − (PoiseDamage × Poise/100)
```

Poise is a percent reduction on incoming poise damage. Sources: [darksouls3.wiki.fextralife.com](https://darksouls3.wiki.fextralife.com/), [darksouls.fandom.com](https://darksouls.fandom.com/). Community spreadsheet: [poise-through calculator](https://docs.google.com/spreadsheets/d/1q4nzo42YTASrhROFVgad7uGEC94lstCdc0-83P49zBc).

**Elden Ring poise:**

```
Toughness = Poise / 10  // for players
Stagger occurs when incoming poise damage ≥ Toughness
```

Regen: players fully reset after **30s no-hit**; NPCs reset after 6–15s scaling linearly on poise (80 poise → 6s, 200 poise → 15s). Hard breakpoints: 51 poise resists normal attacks; ≤50 behaves like 0. PvE poise damage values: 20 (arrows), 50 (light melee), 100 (medium), 150/200/250/300 (heavy/ultra).

**Smithbox param editor** — [`github.com/vawser/Smithbox`](https://github.com/vawser/Smithbox) — live regulation-file browser for ER, ER Nightreign, AC6, Sekiro, DS1/2/3, Bloodborne, Demon's. ToughnessParam and related tables editable/viewable. Soulsmodding wiki (<https://soulsmodding.com>) documents `AtkParam` attack poise damage fields.

**OpenMW `combat.cpp` — actual open-source knockdown/stagger code.** [`apps/openmw/mwmechanics/combat.cpp`](https://github.com/OpenMW/openmw/blob/master/apps/openmw/mwmechanics/combat.cpp) on `github.com/OpenMW/openmw`. Wiki: [`wiki.openmw.org/index.php?title=Research:Combat`](https://wiki.openmw.org/index.php?title=Research:Combat). Key rules:

- **Hitstun** state prevents movement / new attacks / casting; allows turning, crouching, jumping, and completing in-progress attacks.
- **Knockdown rule:** `if (agility × fKnockDownMult) ≤ incoming_damage AND random_roll vs (agility × iKnockDownOddsMult × 0.01 + iKnockDownOddsBase) → knockdown`
- Creatures mid-attack/cast/lockpick are immune to hitstun; NPCs/players always take it.

**This is the cleanest open-source reference implementation of §14 role transformation on damage.** Real C++ you can paste.

### B.7 Beat clock + hit windows — osu! + StepMania + FNF

**osu! (C#, MIT) `OsuHitWindows.cs`** — [`osu.Game.Rulesets.Osu/Scoring/OsuHitWindows.cs`](https://github.com/ppy/osu/blob/master/osu.Game.Rulesets.Osu/Scoring/OsuHitWindows.cs). The judgement model is `DifficultyRange(min, average, max)` tuples that scale with Overall Difficulty (OD):

```
GREAT_WINDOW_RANGE = DifficultyRange(80, 50, 20)    // ±80ms at OD0, ±20ms at OD10
OK_WINDOW_RANGE    = DifficultyRange(140, 100, 60)
MEH_WINDOW_RANGE   = DifficultyRange(200, 150, 100)
MISS_WINDOW        = 400   // fixed, OD-independent
```

`SetDifficulty(double)` lerps each range and subtracts 0.5 ms. `ResultFor(timeOffset)` walks windows tightest-to-loosest and returns the first `HitResult` where `|offset| <= window`. **Clean, minimal reference for "input acceptance window with difficulty tier."**

**StepMania `src/Player.cpp`** — default `TimingWindowSecondsInit`:

| Window | seconds | ms |
|--------|---------|-----|
| TW_W1 (Marvelous) | 0.0225 | 22.5 |
| TW_W2 (Perfect) | 0.045 | 45 |
| TW_W3 (Great) | 0.090 | 90 |
| TW_W4 (Good) | 0.135 | 135 |
| TW_W5 (Boo) | 0.180 | 180 |
| TW_Hold | 0.250 | 250 |
| TW_Roll | 0.500 | 500 |

Scaled at runtime by `m_fTimingWindowScale`.

**Friday Night Funkin' `Constants.hx` + `Scoring.hx` PBOT1 thresholds:**

```
HIT_WINDOW_MS = 160.0   // absolute cutoff
PBOT1_SICK_THRESHOLD = 45.0
PBOT1_GOOD_THRESHOLD = 90.0
PBOT1_BAD_THRESHOLD  = 135.0
PBOT1_SHIT_THRESHOLD = 160.0
```

**BMS / LR2 / beatoraja — asymmetric BAD windows** ([`iidx.org/misc/iidx_lr2_beatoraja_diff`](https://iidx.org/misc/iidx_lr2_beatoraja_diff)). Beatoraja's BAD is `+165/-210` — not symmetric. **Relevant for §12 synchronized commit: if you want "generous on the late side but strict on the early side," this is your reference.**

**Arcade IIDX baseline:** PGREAT ±16.67 ms · GREAT ±33.33 ms · GOOD ±116.67 ms · BAD ±250 ms · POOR at −250.

**Honest gap — networked beat clock.** No open rhythm game ships co-op clock synchronization. For that, the reference pattern is GGPO rollback (not a rhythm game) — combine osu!'s window model with NecroDancer SYNCHRONY's beat-granular rollback approach (Appendix A.6).

### B.8 Threat / aggro tables — TrinityCore ThreatManager

**Primary — TrinityCore `ThreatManager`** ([`src/server/game/Combat/ThreatManager.{h,cpp}`](https://github.com/TrinityCore/TrinityCore/blob/master/src/server/game/Combat/ThreatManager.cpp)). The canonical open implementation of every aggro concept the PDF attributes to Army of Two.

**Update tick (verbatim):**

```cpp
static const uint32 THREAT_UPDATE_INTERVAL = 1000u;  // ms

void ThreatManager::Update(uint32 tdiff) {
    if (!CanHaveThreatList()) return;
    if (_updateTimer <= tdiff) {
        if (_needThreatClearUpdate) { SendClearAllThreatToClients(); _needThreatClearUpdate = false; }
        if (!IsThreatListEmpty(true)) UpdateVictim();
        _updateTimer = THREAT_UPDATE_INTERVAL;
    } else {
        _updateTimer -= tdiff;
    }
}
```

**Threat is re-evaluated once per second, not continuously.** No time-based decay in TC — threat is monotonic until explicitly wiped.

**Addition:** `void ThreatManager::AddThreat(Unit* target, float amount, SpellInfo const* spell, bool ignoreModifiers, bool ignoreRedirects)`. Flow: check `SPELL_ATTR1_NO_THREAT` / `SPELL_ATTR2_NO_INITIAL_THREAT`, redirect to vehicle/owner, run `CalculateModifiedThreat(threat, victim, spell)` (school + spell + player-mod multipliers), distribute to redirects, update `ThreatReference`. `ThreatReference::GetThreat() = std::max(_baseAmount + _tempModifier, 0.0f)`.

**Target selection — `ReselectVictim`:**
- Fixate wins unconditionally
- **110% threshold**: new target must have ≥1.10× current victim threat to switch (in melee range)
- **130% threshold**: automatic switch regardless of range
- Fibonacci heap of sorted threat references (O(log n) reselect)

**Aggro modifiers:**
- `ResetThreat(Unit*)` — full wipe (Feign Death, Vanish)
- `ScaleThreat(Unit*, float factor)` — multiplicative reduction
- `ModifyThreatByPercent(Unit*, int32 percent)` — e.g. Salvation −30%
- `MatchUnitThreatToHighestThreat(target)` — taunt
- `FixateTarget` / `TauntUpdate()` — aura-driven override

**Direct mapping to §9 Attention Economy:**
- Every weapon action → `AddThreat` call with per-action multiplier
- `Update()` on 1s tick
- Reselection uses TC's 110%/130% hysteresis (prevents target thrash)
- Aggro wipe = `ResetThreat`
- **Extension beyond TC:** add exponential decay in `Update()` — `_baseAmount *= exp(-dt/tau)` with τ ≈ 30s. Cite TC as the unmodified reference and this as your Springtale extension.

**Secondary — AzerothCore** ([`github.com/azerothcore/azerothcore-wotlk`](https://github.com/azerothcore/azerothcore-wotlk), fork of TC, actively maintained). Issue #5985 "Rewrite combat and threat system to be mutual from TC" documents the design tradeoffs.

**Minimal baseline — rAthena** ([`github.com/rathena/rathena`](https://github.com/rathena/rathena), `src/map/mob.cpp`, `src/map/mob.hpp`). `mob_data.attacked_count` (uint8), `aggressive` flag, `rudeattacked` condition. Much simpler than TC — no heap, just "who hit me last N times."

### B.9 Narrative branching with override tokens — Ink + Yarn Spinner + ChoiceScript

**Primary — Ink `VariablesState.cs`** ([`github.com/inkle/ink/blob/master/ink-engine-runtime/VariablesState.cs`](https://github.com/inkle/ink/blob/master/ink-engine-runtime/VariablesState.cs)).

**Storage model:**

```csharp
Dictionary<string, Object> _globalVariables;
Dictionary<string, Object> _defaultGlobalVariables;
StatePatch _patch;   // background-save overlay
```

**Persistence API:**
- `WriteJson()` — serializes, skipping variables that still match defaults (`dontSaveDefaultValues`)
- `SetJsonToken(token)` — loads state; unspecified vars fall back to defaults
- `Assign(variablePointer, value)` routes global vs temp
- `ApplyPatch()` / `StartVariableObservation` — transactional saves

**Override-token model in Ink script:**

```ink
VAR knows_about_wager = false
* { not seen_clue } [Accuse Jefferson]           // one-shot: once seen, vanishes
+ { visit_paris }  [Return to Paris] -> visit_paris  // sticky, respects flag
* -> Default content when no other choices remain
```

**This is exactly the As Dusk Falls override-token model with open, readable code.** A `*` choice gated by a visit-count condition consumes itself on first use. `{ knot_name > 3 }` tests visit thresholds directly, letting you accumulate weighted consequences.

**Mapping to §11 Consensus override:**
- Token = global `VAR` counter
- One-shot = `* { not used_token }` (disappears after selection)
- Cross-chapter persistence = free from `WriteJson`/`SetJsonToken`
- Consequence accumulation = arithmetic on globals, tested via `{ guilt > 3: ... }`

**A Rust reimplementation is ~200 lines.** Cite Ink as primary.

**Yarn Spinner** — [`github.com/YarnSpinnerTool/YarnSpinner`](https://github.com/YarnSpinnerTool/YarnSpinner). `VariableStorageBehaviour` abstract class, 8 methods. `SaveStateToPersistentStorage` / `LoadStateFromPersistentStorage` serialize all variables to JSON. Sample custom storage: [`YarnSpinnerTool/CustomVariableStorage`](https://github.com/YarnSpinnerTool/CustomVariableStorage).

**ChoiceScript** — [`github.com/dfabulich/choicescript`](https://github.com/dfabulich/choicescript). Two variable scopes (`*temp` scene-local, permanent), `*set var expr`, `*if`, **fairmath** (`%+`, `%-`) for soft accumulation — directly models "consequence dials" without hard thresholds. Engine: `web/scene.js`.

**SugarCube (Twine)** — `$var` = story variable (persistent, written to history), `_var` = temp. History-based persistence with rewind. Over-powerful for Springtale but useful as the "maximum-persistence" extreme reference.

### B.10 Motion input parsers — OpenBOR + fighting game community

**Primary — OpenBOR (C, BSD-ish) `check_combo`** — [`github.com/DCurrent/openbor`](https://github.com/DCurrent/openbor), `engine/openbor.h` + `engine/openbor.c`. Defines `MAX_SPECIAL_INPUTS = 27`. Special-move table is a 2D array; last slots are metadata:

```
[MAX_SPECIAL_INPUTS-1]  reserved flag
[MAX_SPECIAL_INPUTS-2]  animation index
[MAX_SPECIAL_INPUTS-3]  reserved
[MAX_SPECIAL_INPUTS-4..-10]  cancel-window metadata
```

`check_combo()` walks the player's recent input ring buffer, compares backward against every row in the move table, and **picks the deepest match** when sequences overlap (Shoryuken vs Hadouken disambiguation).

**Portable data layout:**

```c
struct SpecialMove {
    uint8  seq[MAX_SEQ];    // direction|button bitflags
    uint8  len;
    uint16 cancel_window;   // frames during which this cancels prior
    uint16 anim_id;
};
```

**Community-documented fighting game buffers:**

- Street Fighter 6 quarter-circle: **11-frame buffer**
- Classic SF / Guilty Gear: **6-frame** motion buffer
- Typical algorithm (Seung-Cha, Celia Wagar):
  1. Ring buffer of `(frame, directionMask, buttonMask)` per player
  2. Each frame, for every registered move pattern, walk backward up to `buffer_len`, match symbols in order with 1–2 frame slack between entries
  3. First match wins; longer sequence wins ties
  4. Optional: compile moves into a DFA for O(1)-per-frame matching

**Mapping to Helldivers 2 stratagem codes:** a stratagem code (↑↑↓↓←→←→ + button) is a motion input with zero directional slack and no button repeats. **Helldivers's entire input layer is a ~50-line specialisation of OpenBOR's `check_combo`** with `MAX_SEQ = 10`, no timing slack between entries, and final button = "Call-In" rather than attack.

**Rust port:** one `VecDeque<InputFrame>` per agent, one `Vec<Stratagem>` pattern table, `match_stratagem(deque, pattern) -> Option<Stratagem>`. Strict DFA. Celia Wagar's [*How to Code Fighting Game Motion Inputs*](https://critpoints.net/2025/02/05/how-to-code-fighting-game-motion-inputs/) is the canonical walkthrough.

**Sources:**
- [pangaea — Fighting Game Input Systems](https://pangaea.neocities.org/post/fighting-game-input-systems/)
- [Seung-Cha — Fighting Game Input Buffer](https://seung-cha.github.io/coding/2024/01/26/fighting-game-input-buffer.html)
- [EventHubs — SF6 input trouble breakdown](https://www.eventhubs.com/news/2023/jun/17/sf6-input-trouble-breakdown/)

### B.11 Paired cooperative animations — UE Contextual Animation only

**Flag: this is the weakest open-source category in the entire spec.** No open-source game ships Splinter-Cell-style strict paired animations.

**Only production-quality option — Unreal Contextual Animation System** (UE 5.3+). Handles synced animations between actors where attacker montage is "inserted" into target montage with shared transforms. Best tutorial: <https://vorixo.github.io/devtricks/contextual-anim/>. UE source access requires Epic Games sign-up but is technically open.

**Minimal open references:**
- [`srikanthpolineni/UnrealAnimSync`](https://github.com/srikanthpolineni/UnrealAnimSync) — multi-character animation syncing in UE. Minimal, not production-scale.
- [`Pokeyi/VRC-Animation-Sync`](https://github.com/Pokeyi/VRC-Animation-Sync) — Unity/VRChat paired animation prefabs, MIT. Documents the alignment-transform pattern.

**Naughty Dog publicly moved AWAY from paired animations.** Anthony Newman's [*"Unsynced: The Last of Us Melee System"*, GDC 2014](https://gdcvault.com/play/1020368/Unsynced-The-Last-of-Us) — **the talk title is literally "unsynced."** Paired animations broke whenever geometry/physics disagreed; the Last of Us adopted interruption-safe "unsynced" hit exchanges instead.

**Recommendation for the spec:** adopt the Last of Us "unsynced" model rather than the Splinter Cell paired model. If Springtale's cooperation must support `SequentialDependency` handoffs (§20), implement them as independent timed actions with a shared environment key rather than strict animation pairing.

### B.12 Mission pacing / objective heat — DayZ + Vampire Survivors

Already covered in B.2 for DayZ's `cfgeventspawns.xml` schema.

**Vampire Survivors timed spawns** — [`vampire.survivors.wiki/w/Timed_Enemy_Spawn`](https://vampire.survivors.wiki/w/Timed_Enemy_Spawn). Publicly documented:

- One wave per minute
- Min-count + spawn-interval per wave
- **300 alive-enemy hard cap** past which only bosses spawn
- Curse stat scales both spawn frequency and enemy health

Simpler than DRG but fully transparent. Useful as a minimal baseline for §22 pacing when you want a fixed-cadence spawner without an intensity feedback loop.

### B.13 Summary — swap table for the spec

| Mechanism (§) | Closed reference in PDF | Open replacement in Appendix B |
|---|---|---|
| §5 / §22 Director pacing | L4D AI Director | L4D Booth PDFs (public) + Payday 2 `GroupAIStateBesiege.lua` + PaceMaker arXiv 2408.15001 |
| §7 / §22 Spawn budget | DRG Difficulty Points | DRG wiki tables + `trumank/drg-custom-difficulties` + KF2 Controlled Difficulty + Minecraft raid + DayZ cfgeventspawns + Warframe scaling formulas |
| §6 / §8 / §15 Morale | Total War `_kv_morale_tables` | 0AD-Morale-System JS + Spring/BAR XP formulas + OpenRA veterancy + Wesnoth ZoC |
| §10 / §13 Surfaces | DOS2 elemental chains | Noita `<Reaction/>` XML + CDDA `field_type.json` + Powder Toy FIRE.cpp + DCSS `cloud.cc` |
| §10 Destruction | R6 Siege RealBlast | Nvidia Blast (BSD-3) + Unreal Chaos docs + Teardown blog + voro++ |
| §14 Role transform / stagger | Monster Hunter part HP | Kiranico tables + SmartHunter/MHWMasterDataUtils data model + Sekiro datamined regen table + DS3/ER poise formulas + OpenMW `combat.cpp` knockdown (real C++) |
| §5 / §12 Beat clock | Necrodancer timing | osu! `OsuHitWindows.cs` + StepMania `TimingWindowSecondsInit` + FNF `Scoring.hx` PBOT1 + BMS/LR2 asymmetric windows |
| §9 Attention Economy | Army of Two aggro | TrinityCore `ThreatManager` (1000ms tick, 110/130% hysteresis, Fib-heap reselect, full wipe/scale/modify API) |
| §11 Consensus override | As Dusk Falls tokens | Ink `VariablesState.cs` + visit-counted `*` choices + Yarn Spinner `VariableStorageBehaviour` + ChoiceScript fairmath |
| §19 Motion input | HD2 stratagems | OpenBOR `check_combo` + Celia Wagar DFA pattern |
| §20 Paired handoff | Splinter Cell boost | **No good open ref.** UE Contextual Animation is only option; Naughty Dog "unsynced" model is the recommended alternative |

**Every row in this table means:** the spec can cite *readable open code* instead of (or alongside) the closed-game design reference. For anything marked weak, flag it explicitly in the final implementation doc.

---

## Appendix C — LFCG alignment (Pais et al., CHI 2024)

The most useful external anchor for this entire document is the **Living Framework for Cooperative Games (LFCG)**, published by an 11-author team from LASIGE (Lisbon) + KIT Karlsruhe at CHI 2024. It analyzes 129 cooperative games via Template Analysis and produces a four-axis taxonomy with explicit sub-patterns. Springtale's cooperation module is not cited by LFCG (the paper is about human-human cooperative games, not machine-agent systems), but LFCG's vocabulary maps cleanly onto most of what this document specifies. Using LFCG's names where they fit anchors Springtale in a peer-reviewed academic lineage and positions it as *"extends Pais et al. 2024 to machine-agent cooperation."*

**Full paper:** Pedro Pais, David Gonçalves, Daniel Reis, João Cadete Nunes Godinho, João Filipe Morais, Manuel Piçarra, Pedro Trindade, Dmitry Alexandrovsky, Kathrin Gerling, João Guerreiro, André Rodrigues. *"A Living Framework for Understanding Cooperative Games."* CHI '24, May 11–16 2024, Honolulu, HI. Paper 220, pp. 1–17. DOI **10.1145/3613904.3641953**. CC BY 4.0.

- **Open-access PDF:** <https://techandpeople.github.io/downloads/2024_chi_lfcg.pdf>
- **Interactive web app:** <https://www.lfcooperativegames.com/> (authoring new reports and custom framework versions is supported)
- **ACM DL:** <https://dl.acm.org/doi/10.1145/3613904.3641953>
- **dblp:** <https://dblp.org/rec/conf/chi/PaisGRGMPTAG0024.html>

### C.1 Full taxonomy tree (verbatim from the paper + live framework page)

**Top level — four axes** (Table 1 of the paper):

- **Play Structures** — *"The overarching structures of play."*
- **Player Context** — *"How players engage with the gameplay."*
- **Forms of Cooperation** — *"How games support player cooperation."*
- **Cooperation Design Patterns** — *"How games promote cooperation."*

#### C.1.1 Play Structure (§4.1)

- **Progression Structure** — how the overall experience advances
  - Community (*Destiny 2*)
  - Server (*Valheim*)
  - Party (*Overcooked 2*)
  - Individual (*Gunfire Reborn*)
- **Group Formation**
  - Serendipitous (*World of Warcraft*)
  - Party Creation (*Fortnite*)
  - Drop-in/Drop-out (*Brothers: A Tale of Two Sons*)
  - Looking for Group (*Rec Room*)
  - Organised Grouping (*Flat Heroes*)
- **Goal Structure**
  - Shared (*Flat Heroes*)
  - Intertwined (*BoxBoy! + BoxGirl!*)
  - Independent (*Gloomhaven*)
  - Conflicting (*A Way Out*)
  - No Goal Structure (*Minecraft*)

#### C.1.2 Player Context (§4.2)

- **Player Identity**
  - *Representation:* Single (*Tiny Brains*), Dispersed (*Age of Empires IV*), Distinct (*Rayman Legends*), Shared (*Octodad*), No representation (*Keep Talking and Nobody Explodes*)
  - *Selection:* Arbitrary (*Unravel Two*), Pool (*Fight'N Rage*), Customisation (*Terraria*)
  - *Progress:* Predefined (*Portal 2*), Static, Customisable (*Guild Wars 2*), Switchable (*Lego Star Wars*)
- **Relationships between Player Entities**
  - Individuals (*Portal 2*)
  - Sidekick (*Child of Light*)
  - Teammates (*Ghost of Tsushima: Legends*)
  - Allies (*WoW*)
  - Competitors (*It Takes Two* mini-games)
- **Game World**
  - Shared (*Overcooked 2*)
  - Unique (*Savage 2*)
  - Distinct (*Keep Talking and Nobody Explodes*)
- **Player Viewpoint**
  - Shared (*Rayman Legends*)
  - Split (*It Takes Two*)
  - Distinct (*Destiny 2*)

#### C.1.3 Forms of Cooperation (§4.3)

- **Arrangement**
  - Strict (*We Were Here Together*)
  - Free (*Lovers in a Dangerous Spacetime*)
  - Coupled (*Sea of Thieves*)
  - Coincident (*Cuphead*)
- **Synchronicity**
  - Sequential (*Mario + Rabbids: Kingdom Battle*)
  - Concurrent (*Counter-Strike*)
  - Asynchronous (*Minecraft*)
- **Communication**
  - *Communication by Design:* Agnostic (*Necesse*), Limited (*Among Us*), Required/Incentivised (*Keep Talking and Nobody Explodes*)
  - *Means of Communication:* Voice Chat, Text Chat, Pings, Pins, Drawings, In-Game Movement/Actions, Voice Lines, Premade Messages, Emotes, Body Posture (VR), Hand Tracking (VR)

#### C.1.4 Cooperation Design Patterns (§4.4)

Figure 5 shows seven CDP columns, but two re-reference the first two axes (Cooperative Play Structures, Cooperative Player Contexts). The five genuinely new CDP categories:

- **Dependencies** — *"cooperation incentives derived from the way gameplay activities are structured or from the gameplay actions and the constraints that they put upon the players."*
  - Task (*Unravel Two*)
  - Grouping (*Destiny 2*)
  - Spatial (*Left 4 Dead 2*)
  - Temporal
  - Fixed Difficulty (*Destiny 2* Strikes)
  - Scaling Difficulty (*Diablo 2*)

- **Affecting Others** — *"mechanics that enable players to affect others unidirectionally. All of them, depending on the context and implementation, can be altruistic… or non-altruistic…"* Extends Rocha et al. [64] *"Abilities that can only be used on another player"* and Björk et al. [9] *"Delayed Reciprocity"*.
  - Assistive Actions (*Age of Empires*)
  - Manipulating Others' Entities (*Humans Fall Flat*)
  - Piggy-Backing (*Super Mario 3D World*)

- **Resource Sharing** — *"captures when the control and/or management of resources… pertains to more than one player. This creates a direct way that players affect and/or interact with each other, incentivising them to collectively negotiate how to manage and utilise them."*
  - Consumables (*Cuphead*)
  - Unlockables (*Guacamelee*)
  - Interactables (*Sea of Thieves*)
  - Playable Characters (*Lego Star Wars*)
  - Space (*Counter-Strike*)

- **Asymmetry** — *"asymmetric patterns that are leveraged to promote cooperation between players."* All three sub-leaves derive from Harris et al. (2016).
  - Information (*The Timeless Child — Prologue*)
  - Abilities (*Magicka*)
  - Usefulness (*Borderlands*)

- **Relations between Player Actions** — *"describes the type of in-game actions in relation to the other player."* Both leaves extend Rocha et al. [64].
  - Synergies (*World of Warcraft* shadow-priest → warlock)
  - Complementarity (*Gloomhaven*)

### C.2 Methodology (§3) — short summary

- **Template Analysis** (Brooks & King 2012; Braun & Clarke), hierarchical coding with a priori template refined iteratively.
- **Corpus construction**: Metacritic top-rated per year 2017–2022 co-op filter (50 games) + Steam Co-op/Online Co-op/Local Co-op/Team-based/Co-op Campaign tags by concurrent users (68 games) → **118 games**, expanded to **129 after validation**.
- **Per-game protocol**: store description → top review (≥1h playtime) → trailer → most-relevant review video → playthrough (≥20 min excluding tutorial).
- **Calibration game**: *Overcooked* — all 7 first-pass coders analyzed it and met to iterate the codebook.
- **Validation**: 9 games re-analyzed by 1 senior + 1 junior coder; full LFCG review by an additional senior researcher.
- **No inter-rater reliability metric** — consistent with Template Analysis (consensus-based, not statistical).

### C.3 Corpus overlap with Springtale's 14 reference games

| Springtale game | In LFCG corpus? |
|-----------------|:---------------:|
| Left 4 Dead 2 | **Yes** — cited §4.4.2 Spatial example |
| Deep Rock Galactic | **Yes** — live report |
| Overcooked (1 or 2) | **Partial** — used as the codebook-iteration calibration game per §3.4, but **no formal report was ever published** on the LFCG web app (verified via `/api/reports?game={18433,103341,135963}` — all return empty) |
| It Takes Two | **Yes** — cited §4.2, §4.3, live report |
| Helldivers 2 | No |
| Army of Two | No |
| Total War (series) | No |
| Patapon | No |
| Crypt of the NecroDancer | No |
| Monster Hunter | No |
| Divinity: Original Sin 2 | No |
| Rainbow Six Siege | No |
| Splinter Cell | No |
| As Dusk Falls | No |

**Overlap: 3 formal reports (L4D2, DRG, ITT), plus Overcooked as codebook calibration.** LFCG draws from storefront-surfaced 2017–2022 Steam/Metacritic co-op; Springtale's catalog weights squad-cadence and tactical titles (Helldivers, R6, Splinter Cell, Monster Hunter, Total War) that Steam tag-filters often miss. The two corpora are **complementary, not redundant** — and the 11 non-report games are a research-contribution opportunity (see C.8).

**Per-game coding retrieved via the LFCG public API** (`/api/reports?game={id}`) — dumps at `docs/intended-arch/research-sources/lfcg-reports/*.json`, parsed in `parsed.md`. See C.3.1 below for the verbatim codings.

### C.3.1 Verified LFCG codings for the 3 overlap games

The LFCG web app is an SPA but exposes a straightforward REST API. Game IDs are looked up via `/api/games?name=...` and reports via `/api/reports?game={id}`. Three reports retrieved:

#### Left 4 Dead 2 — coded by **Pedro Pais** (LFCG lead author), 2024-05-02

**Framework difficulty self-reported: 3/5** (medium). Analysis level: macro. Analysis type: played directly.

| Axis | Coding | Pais's example (verbatim) |
|------|--------|--------------------------|
| **Progression** | Party | *"The whole party progresses together (even if the save is stored in the device of one of the players)"* |
| **Group Formation** | Party Creation | *"Players have to organise themselves before playing"* |
| **Goal** | Shared | *"Players pursue the same goal (finishing the level/game)"* |
| **Relationships** | Individuals | *"There is no mechanic that relies on the relationship between players"* |
| **Game World** | Shared | — |
| **Viewpoint** | Distinct | — |
| **Identity → Representation** | Single | *"Players control their own character, and nothing else"* |
| **Identity → Selection** | Arbitrary | *"No selection exists that the game assumes (it attributes specific characters to players 1 and 2)"* |
| **Identity → Progress** | Static | *"Outside of tutorial sections, a player's character can always do the same thing (shoot portals, pick up interactables)"* |
| **Arrangement** | Free + Coincident | *"Some tasks ask players to do the same thing at the same time (e.g., hold a point)"* |
| **Synchronicity** | Concurrent + Sequential | — |
| **Communication by Design** | Required/Incentivised | — |
| **Means of Comms** | Pings, Voice Lines, In-Game Movement/Actions | — |
| **CDP → Dependencies** | **Spatial** | *"The game's enemies deliberately focus isolated players"* |
| **CDP → Affecting Others** | Assistive Actions | *"Players can heal each other"* |
| **CDP → Resource Sharing** | Consumables, Interactables, Space | *"Ammo and healing items are shared"* and *"Some items are explosive and can damage friendly players"* |

**Cross-reference to Appendix A.1:** Pais's "Spatial dependency" coding matches Booth's "75% of Mobs come from behind + specials target isolation" design intent. Pais's "Free + Coincident Arrangement" matches the "any survivor can revive any survivor, and specials require the whole team to respond to" semantic.

#### Deep Rock Galactic — coded by Tiago Pereira, 2025-10-19

**Framework difficulty self-reported: 3/5.** Analysis type: observations (not played). Note from Tiago (Portuguese, translated):

> "The LFCG framework is very practical for systematizing the analysis of complex cooperative games, and allows identification of dependency, communication and asymmetry patterns. I'd say the main difficulty is correctly interpreting some categories without playing the game directly, especially in games with many layers of both individual and collective progression. It might be useful to have examples and guidelines on how to properly observe cooperation patterns indirectly."

| Axis | Coding |
|------|--------|
| **Progression** | **Party + Individual** (both — missions are team, XP/loot are per-player) |
| **Group Formation** | **Party Creation + Drop-in/Drop-out + Looking for Group** (all three — DRG supports matchmaking) |
| **Goal** | **Shared + Intertwined + Independent** (mission shared, but secondary objectives diverge) |
| **Relationships** | Allies (shared faction) |
| **Game World** | Shared |
| **Viewpoint** | Distinct |
| **Identity → Representation** | **Distinct** (Gunner/Engineer/Driller/Scout) |
| **Identity → Selection** | Pool |
| **Identity → Progress** | Customisable |
| **Arrangement** | Free + **Coupled** (classes intertwine) |
| **Synchronicity** | Concurrent |
| **Communication by Design** | Required/Incentivised |
| **Means of Comms** | Text Chat + Pings + Voice Chat |
| **CDP → Dependencies** | **Task + Spatial + Scaling Difficulty** |
| **CDP → Affecting Others** | Assistive Actions |
| **CDP → Resource Sharing** | Consumables + Unlockables + Interactables + Playable characters + Space |
| **CDP → Asymmetry** | **Abilities + Usefulness** |
| **CDP → Relations** | **Synergies + Complementarity** |

**This DRG coding is the single most-dense LFCG classification in the 3-report set** — DRG exercises more LFCG axes than L4D2 or ITT. Tiago noted "complementarity" explicitly ("Cada classe completa as outras, o que gera interdependência e reforça cooperação" = "Each class completes the others, generating interdependence and reinforcing cooperation"). This is direct empirical support for Springtale's §23 specialization design principle.

#### It Takes Two — coded by Jorge Guerreiro, 2023-10-22

**Framework difficulty self-reported: 2/5** (easier — because ITT is fundamentally a 2-player game with cleaner axes).

| Axis | Coding |
|------|--------|
| **Progression** | Party |
| **Group Formation** | Party Creation |
| **Goal** | **Shared + Conflicting** (main shared, mini-games competitive) |
| **Relationships** | **Teammates + Competitors** (same dual structure) |
| **Game World** | Shared |
| **Viewpoint** | **Split** (split-screen co-op) |
| **Identity → Representation** | Single |
| **Identity → Selection** | Pool |
| **Identity → Progress** | **Predefined** |
| **Arrangement** | **Coupled** |
| **Synchronicity** | **Sequential + Concurrent** |
| **Communication by Design** | Required/Incentivised |
| **Means of Comms** | In-Game Movement/Actions |
| **CDP → Dependencies** | Task |
| **CDP → Affecting Others** | Assistive Actions |
| **CDP → Resource Sharing** | Playable characters |
| **CDP → Asymmetry** | Abilities |
| **CDP → Relations** | **Complementarity** |

**Critical confirmation for §20 handoff:** Guerreiro coded ITT as **Coupled + Sequential + Concurrent** — this is the exact "Coupled Sequential" pattern that §20 identifies as the strongest LFCG anchor for handoff/transition. Guerreiro's own example: *"The players have to do tasks in a sequential order, working together, mainly in a coordinated manner to progress."*

### C.3.2 What the live LFCG app calls "Collaboration" vs. the paper's "Cooperation"

**Nomenclature correction:** the paper uses *"Forms of Cooperation"* and *"Cooperation Design Patterns"* but the live web app (`/frameworks/1`) uses *"Forms of Collaboration"* and *"Collaboration Design Patterns"* with sub-leaves also suffixed — *"Strict Collaboration"*, *"Coupled Collaboration"*, *"Free Collaboration"*, *"Concurrent Collaboration"*, etc. The two terms appear interchangeably in LFCG's own materials. Springtale should adopt **"Cooperation"** (the paper's term) for consistency, but anyone browsing the live app will see "Collaboration" suffixed on every sub-leaf.

The LFCG framework description field (fetched via `/api/frameworks`) uses *"Forms of Collaboration"* and *"Collaboration Design Patterns"* verbatim, confirming the web app's terminology is the current one. The CHI 2024 paper was written with "Cooperation" — this is a post-publication rebranding by the authors.

### C.3.3 Google Scholar citation count

LFCG has **10 citations on Google Scholar** as of April 2026 (fetched via Playwright with scroll). Follow-up work includes:

- **Reis, Pais, Gonçalves, Gerling, Rodrigues (2025)** *"Exploring Asymmetry of Information in Cooperative Games"* — Springer CCIS 2324, DOI 10.1007/978-3-031-81713-7_11. First published extension by the original team, deepening the Asymmetry → Information leaf.
- **"From Solo to Social" CHI 2025** — cited LFCG as a design tool for a co-located cooperative exergame. DOI 10.1145/3706598.3713937.

Individual cite list requires a logged-in Scholar session and is not retrievable by automated fetch.

### C.4 Predecessor / lineage chain

LFCG's bibliography is the cleanest academic lineage for cooperative-game patterns. Papers Springtale should cite alongside (where applicable):

- **Zagal, Rick & Hsi (2006)** *"Collaborative games: lessons learned from board games"* — foundational collaborative-game analysis. Source of the "solitaire degeneration" pitfall. PDF: <https://my.eng.utah.edu/~zagal/Papers/Zagal_et_al-Collaborative_Games.pdf>. **Relevant to §23 specialization** — solitaire degeneration is exactly the anti-pattern Springtale's formation model avoids.
- **El-Nasr et al. (2010)** *"Understanding and Evaluating Cooperative Games"* — primary prior cooperative-game taxonomy. Source of *"Limited Resources"* and *"Interacting with the Same Object"* constructs. **Relevant to §10 shared environment.**
- **Rocha, Mascarenhas, Prada (2008)** *"Game Mechanics for Cooperative Games"* — source of *"Abilities only usable on another player"*, *"Synergies"*, *"Complementarity"*. Directly extended by LFCG into Affecting Others and Relations between Player Actions. **Relevant to §18 recovery, §20 handoff, §23 specialization, §24 sacrifice.**
- **Björk et al. (2003) / Björk & Holopainen (2005)** *"Patterns in Game Design"* — the original game-design-patterns book. Source of *"Dynamic alliances"* and *"Delayed Reciprocity"*. **Relevant to §11 consensus, §18 recovery.**
- **Toups et al. (2014)** *"A Framework for Cooperative Communication Game Mechanics"* — grounded-theory prior work on co-op comms. **Relevant to §19 communication protocols.** (Already cited in the existing §21 mapping to Cannon-Bowers 1993 shared mental models.)
- **Reuter et al. (2014 DiGRA)** *"Game Design Patterns for Collaborative Player Interactions"* — source of the *"Spatial"* dependency pattern. **Relevant to §10 shared environment.**
- **Harris et al. (2016)** — source of LFCG's entire Asymmetry CDP (Information, Abilities, Usefulness/Interface). **Relevant to §8 awareness, §16 dynamic capability binding, §23 specialization.**
- **Sykownik, Emmerich & Masuch (2018)** *"Exploring Patterns of Shared Control"*. **Relevant to §14 role transformation, §16 capability binding.**
- **Sicart (2008)** *"Defining Game Mechanics"* — mechanic definition framework.
- **Gonçalves et al. (2023)** — systematic review of 263 social-gaming publications, the LFCG team's immediate prior work that seeded the framework.
- **MDA framework (Hunicke, LeBlanc, Zubek)** — mechanics/dynamics/aesthetics lens.

### C.5 Springtale § → LFCG axis mapping table

| Springtale § | Closest LFCG concept | Fit |
|---|---|---|
| §5 Cadence | Forms of Cooperation → Synchronicity (Sequential/Concurrent/Asynchronous) | **Strong — adopt LFCG vocabulary** |
| §6 Formation | Play Structures → Group Formation + Player Context → Relationships (Teammates/Allies) | **Strong** |
| §7 Momentum | No LFCG equivalent | **Novel — fills LFCG §5.4 limitation** |
| §8 Awareness | CDP → Asymmetry → Information + Forms → Communication (Means) | **Strong** |
| §9 Attention Economy | No LFCG equivalent | **Novel — fills LFCG §5.4 limitation** |
| §10 Shared Environment | CDP → Resource Sharing → Space + Interactables | **Strong** |
| §11 Consensus Engine | Forms → Communication by Design → Required/Incentivised | Partial — LFCG says *whether* comms are required, not *which protocol* |
| §12 Synchronized Commit | CDP → Dependencies → Temporal + Task | Moderate |
| §13 Interference Detection | CDP → Affecting Others → Manipulating Others (non-altruistic) | Moderate |
| §14 Role Transformation | Player Context → Identity → Progress → Switchable | **Strong** |
| §15 Rally & Cascade Recovery | CDP → Affecting Others → Assistive Actions | Partial — LFCG covers the atomic primitive, not cascades |
| §16 Dynamic Capability Binding | CDP → Asymmetry → Abilities + Player Identity → Customisation | Moderate |
| §18 Recovery & Mutual Aid | CDP → Affecting Others → Assistive Actions (altruistic) | **Strong** |
| §19 Communication Protocols | Forms → Communication → Means of Communication | Partial — LFCG's means are human-UI; Springtale needs typed message schemas (see C.7) |
| §20 Handoff & Transition | Forms → Arrangement → Coupled + Synchronicity → Sequential | **Strong — "Coupled Sequential" IS handoff** |
| §21 Shared Mental Model | CDP → Asymmetry → Information (inverted) + Communication by Design | **Strong** |
| §22 Tempo & Pacing | Forms → Synchronicity | Partial — LFCG has no pacing gradient; this is a Springtale extension |
| §23 Specialization vs Generalization | CDP → Relations → Complementarity + Asymmetry → Abilities | **Very Strong** — LFCG Complementarity is exactly the specialization axis |
| §24 Sacrifice & Covering | CDP → Affecting Others → Assistive Actions (altruistic) + Relations → Synergies | **Strong** |

**Translate-cleanly set (adopt LFCG vocabulary directly):** §5, §6, §8, §10, §14, §18, §20, §21, §23, §24 — 10 of 20 module sections.

**Partial-fit set (LFCG gives the atomic pattern, Springtale extends with machine semantics):** §11, §12, §13, §15, §16, §19, §22 — 7 sections.

**LFCG-gap set (Springtale genuinely novel):** §7 Momentum, §9 Attention Economy — 2 sections. See D.6.

### C.6 What Springtale extends beyond LFCG (the novel contribution)

LFCG's own **§5.4 Limitations** and **§6 Outlook** explicitly acknowledge:

1. The framework captures games as static artefacts, not as play sessions across time.
2. Pacing gradients and experience-over-time are not modeled.
3. The corpus is entirely human-human cooperative; AI teammates, bot colonies, and machine cooperation are not considered.

**Springtale's §7 Momentum** directly addresses #1 and #2 — momentum tiers (Cold/Warming/Hot/Fever) are exactly the time-axis state machine LFCG flags as missing. Citing this turns §7 from an apparently-invented construct into "answering an open problem in Pais et al. §6."

**Springtale's §9 Attention Economy** addresses a deeper assumption — LFCG's framework implicitly treats human player attention as uncapped per-player. Machine agents have hard attention budgets (fuel meters, tick quotas, bounded concurrent actions). This is not a limitation the paper calls out because human attention budgeting is out of scope; Springtale's §9 is a genuine extension.

**Springtale's §11 consensus protocols, §12 commit barriers, §13 interference detection, §15 rally supervision, §22 adaptive pacing** also extend LFCG — the paper describes *that* cooperation happens via communication, conflict, and help, but not *which protocol* (Paxos vs Raft vs gossip; 2PC vs saga; optimistic vs pessimistic concurrency; callback vs tree supervision; GCRA vs AIMD). These are all machine-specific implementation axes LFCG legitimately leaves out.

### C.7 Restructuring §19 around LFCG's Communication split

LFCG's cleanest direct contribution to Springtale's internal structure is the **Communication → Communication-by-Design ↔ Means-of-Communication split** (§4.3.3). Adopt it verbatim:

- **`CommunicationByDesign` configuration flag** per formation:
  - `Agnostic` — formation works regardless of whether agents share state
  - `Limited` — formation intentionally constrains information bandwidth (e.g., summarization only)
  - `RequiredOrIncentivised` — formation cannot make progress without explicit comms

- **`MeansOfComms` typed Rust `enum`** per message — bot-appropriate variants of LFCG's human-UI variants:
  - `TypedMessage(ProtocolPayload)` — structured IPC (LFCG's Text Chat analog)
  - `StateBroadcast(SnapshotDelta)` — watch-channel observation (LFCG's Voice Lines analog — state-triggered)
  - `Ping(ObjectRef, f32 urgency)` — DRG laser-pointer analog (LFCG's Pings)
  - `Pin(EnvironmentKey)` — DRG mineral marker analog (LFCG's Pins)
  - `CohesionSignal` — "Rock and Stone" social ack (LFCG's Emotes)
  - `ObservedAction(ActionDescriptor)` — Overcooked chicken-throwing analog (LFCG's In-Game Movement/Actions)

The current §19 matrix lines up with LFCG 1:1 at the conceptual level — making the enum names explicit gives us bidirectional traceability ("every `MeansOfComms` variant corresponds to an LFCG leaf").

### C.8 Follow-up opportunities

1. **Author an LFCG custom framework version: "LFCG-M: Machine-Agent Cooperation."** The web app supports this via `/frameworks/new`. Extend the leaves of Asymmetry→Abilities, Means-of-Communication, and Dependencies with machine-specific values. Springtale would be the first published LFCG extension by a non-author team. Gives a citation line back from any future LFCG paper.

2. **Author LFCG game reports for the 10 non-corpus Springtale games** via `/reports/new`:

   Helldivers 2 · Rainbow Six Siege · Monster Hunter · Patapon · Total War · Divinity: Original Sin 2 · Splinter Cell · Army of Two · Crypt of the NecroDancer · As Dusk Falls

   Every one of these exercises axes the current LFCG corpus under-samples (momentum, asymmetric-info squad tactics, rhythm synchronicity). Authoring reports would (a) double-check Springtale's own co-op analysis against a validated template, (b) contribute back to the research community, and (c) expose any LFCG taxonomy leaves that need new sub-patterns.

3. **Pull LFCG's existing reports for L4D2, DRG, Overcooked, It Takes Two** from the web app via browser session and store as `docs/intended-arch/lfcg-reports/*.json`. This gives Springtale an outside structural analysis of 4 of its 14 reference games — a cross-check against Appendix A claims.

4. **Cite Reis et al. 2025** *"Exploring Asymmetry of Information in Cooperative Games"* (Springer CCIS 2324, DOI 10.1007/978-3-031-81713-7_11) in §8 awareness. It's the first published LFCG extension, by the same team, deepening the Asymmetry → Information leaf. Directly relevant to Springtale's neighbor-snapshot gossip model.

### C.9 What LFCG does NOT close (honest list)

Things we hoped LFCG would close, and what it actually gives us:

| Hoped for | Reality |
|-----------|---------|
| Per-game technical internals for the 4 overlap games | **No** — LFCG codes design-level axes, not implementation data structures. Appendix A still needed for L4D `DirectorOptions`, DRG difficulty points, Overcooked recipe constants, ITT AngelScript chapter inventory. |
| Downloadable taxonomy in JSON/RDF | **No** — prose + interactive web app. No schema export. |
| Per-game LFCG codings | **Not without a browser session** — web app is SPA-rendered. |
| Inter-rater reliability stats | **Not reported** — consistent with Template Analysis methodology. |
| Academic grounding for §7 Momentum and §9 Attention | **Partial — as an open problem citation.** LFCG §5.4 and §6 explicitly flag "no time axis" as a limitation. Citing this grounds Springtale's momentum as "answering an open problem in Pais et al. 2024." |
| Machine-agent cooperation patterns | **No** — every example in LFCG is human-human. This is the open lane Springtale occupies. |

### C.10 Concrete recommendations (summary)

1. **Do not restructure Springtale wholesale around LFCG.** LFCG is *descriptive* (what exists in games), Springtale is *prescriptive* (how the daemon implements cooperation). Keep the module layout.
2. **Cite LFCG vocabulary inline in the 10 strong-fit sections.** Example: §20 should open with *"A Springtale handoff is an instance of LFCG's Coupled Arrangement + Sequential Synchronicity (Pais et al. 2024 §4.3)."*
3. **Add explicit LFCG limitation citation to §7 and §9.** Frame these as extensions that fill gaps the authors themselves flagged.
4. **Restructure §19 to mirror LFCG's Communication-by-Design ↔ Means-of-Communication split** as per C.7 above.
5. **Cite the predecessor chain** (Zagal 2006, El-Nasr 2010, Rocha 2008, Björk 2003, Toups 2014, Reuter 2014, Harris 2016) in the relevant module sections per C.4.
6. **Cite Reis et al. 2025** in §8 as the first LFCG extension.
7. **Treat the 10 non-corpus games as a contribution opportunity**, not just reference material. Authoring LFCG reports for them is cheap, contributes to the research community, and validates Springtale's own game analysis.

---

## Appendix D: source verification summary

Everything in a code block above was copied verbatim from one of:

- Local `~/.cargo/registry/src/index.crates.io-.../` at the versions listed
- Upstream GitHub `main` branches fetched during research (timestamps ~April 2026)
- Published docs.rs pages at the versions listed

Where the research agents could not open source directly, this is stated inline
(search "honest gap" or "not verified" or "unverified"). No code block is invented.

If a specific snippet stops compiling against its cited crate version, it's because
the upstream crate drifted after this research was done, not because the snippet was
fabricated. Pin git revs if line-ref stability matters.
