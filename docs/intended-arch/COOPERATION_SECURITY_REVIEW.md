# Cooperation module security review

**Verification:** the threat model and per-module mitigations below were
re-validated against the shipped `crates/springtale-cooperation/` on
2026-05-10. No drift found; the code matches the controls described.

This is the addendum called for by `COOPERATION_IMPLEMENTATION_PLAN.md` §9 /
§16.7. Every module in `crates/springtale-cooperation` gets a paragraph
below, framed against the six attacker capabilities from §9:

1. Malicious connector — benign capability claim, hostile behavior.
2. Compromised agent — code swapped mid-run.
3. Byzantine formation member — deliberately false reports.
4. Resource exhaustion — drain tokens, barriers, environment slots.
5. Information leak — exfiltration via comms, mental model, logs.
6. Replay — capture a tick stream, replay for duplicate effect.

Each paragraph reports: *threat → detection → mitigation → recovery SLA*.
Paragraphs are deliberately terse; the code + tests are the authority.

---

## `cadence` (`cadence.rs`, `tick_processor.rs`)

Threats: replay of a recorded tick stream (6), resource exhaustion via tick
spam (4). Detection: tick sequence is a strictly monotonic `u64` — any
duplicate or out-of-order sequence is a bus bug, not input; the
broadcast-channel backlog bounds lag to 256 ticks. Mitigation: the bus is
in-process only (no peer injects ticks) and `TickReport::tick_sequence`
must match an outstanding tick or is dropped upstream. Recovery: one tick
(≤33 ms at 30 Hz).

## `formation/` (formation module under `crates/springtale-bot`, types here)

Threats: byzantine member (3) voting or reporting false state, compromised
agent (2) staying on the roster after code swap. Detection: every report
carries `agent_id` + the formation's own roster — an unknown id is dropped;
capability checks reject actions the member never declared. Mitigation:
membership mutations go through `Formation::add_member` / `remove_member`,
both audit-logged; roles are declared at construction and cannot be widened
mid-run. Recovery: same-tick remove; next-tick re-report.

## `momentum/`, `momentum.rs`

Threats: attacker floods reports to force a tier promotion and unlock
higher-impact actions (4 + 1). Detection: tier transitions are driven by
`load_average` over a 30-tick window, not a single report; every promotion
is logged with the triggering window. Mitigation: `CapabilityChecker::with_tier`
recomputes the allow-list per invocation, so even a wrongly-promoted tier
can't widen capability beyond what the role declares. Recovery: one window
(~1 s) once the load subsides.

## `awareness/` (`InMemoryGossipStore`, snapshots)

Threats: information leak via unbounded gossip retention (5), byzantine
member poisoning the shared view (3). Detection: every snapshot is keyed
by agent id + tick; divergent snapshots for the same key are discarded by
`publish` with tracing. Mitigation: the store is bounded (ring-buffered per
agent), scrubbed on formation dissolve, never serialized to disk outside
the opt-in durable handoff path. Recovery: next tick (`publish` + rebroadcast).

## `attention/`

Threats: a compromised agent over-declares focus to starve the swarm (4),
or under-declares to hide its own activity (3). Detection: attention
allocation is computed from `TickReport`s, not self-declared; a member
that stops reporting is marked stale by the liveness gate. Mitigation:
attention load caps are per-agent and enforced by the tick processor;
over-focus triggers an interference check (§interference). Recovery: one
tick.

## `consensus.rs`

Threats: byzantine vote-stuffing (3), replay of a valid vote (6), override
drain (4). Detection: votes are tagged by `(agent_id, proposal_id, tick)`
— duplicates per key are dropped; `ConsensusVote::override_used` is a
single bit per agent per proposal. Mitigation: `resolve` enumerates eligible
voters from the current formation roster, so swapped/unknown ids cannot
vote; timeouts only fire after `deadline_tick` to stop premature resolves.
Recovery: a single proposal window (default ≤10 ticks).

## `commit.rs`

Threats: partial commit after node failure (2 or 4), replay of a prior
commit message (6). Detection: two-phase commit requires a `PrepareAck`
from every live voter before `CommitNow` runs; any missing ack aborts. All
commit messages carry a `commit_id` checked against the active ballot.
Mitigation: atomic execution — either all members apply the side effects
or none do; audit log records abort reason. Recovery: next tick after a
`RollbackAll` broadcast.

## `rally/`

Threats: rally-token double-spend (4), forged rally targets by a byzantine
member (3), cascade contagion amplification (4). Detection: tokens are
consumed atomically from the formation's token pool; cascade spread uses
the WH3 caps (4 friends / 5 enemies) hard-coded in the supervisor.
Mitigation: supervisor runs in one task per formation and reconciles the
pool each tick; a member trying to spend a token it doesn't hold gets a
`COOP-4001` (rally.token_missing). Recovery: next tick.

## `handoff/`

Threats: exfiltration via crafted handoff payloads (5), replay (6),
man-in-the-middle (1 for cross-process flex chains). Detection: payloads
are typed (`HandoffType`) and serde-validated; every payload has an
`origin_agent` + `payload_hash` checked at receive. Mitigation: flex chain
pool authenticates inter-process traffic via HMAC tokens; local handoffs
run over tokio mpsc with no external edge. Recovery: receiver re-requests
next tick on hash mismatch.

## `interference/`

Threats: interference blind-spot allowing two compromised agents to collude
(2+3), false-positive DoS on a legitimate pair (4). Detection: the pair
detector is commutative by construction (property test covers this); every
positive finding carries both `agent_a` and `agent_b` and is idempotent
per `(pair, tick)`. Mitigation: interference findings demote momentum, not
kill agents — so a false positive degrades, it doesn't destroy; the pair
history windows roll so a cleared pair recovers. Recovery: one momentum
window (~1 s).

## `recovery/`

Threats: a replaced agent repeatedly triggers quick-fixes to mask its own
failures (2), resource drain via infinite retry (4). Detection:
`MAX_QUICK_FIX_COUNT = 2` — after two quick-fixes the module escalates to
`RecoveryAction::Replan` and emits a `COOP-7003`. Mitigation: the
recovery FSM is per-formation, persisted, and cleared only on a clean
`IntentPattern::Stabilize`. Recovery: capped at two quick-fix windows
before unconditional replan.

## `role/`, `authority/`

Threats: a malicious connector ships a role declaring over-broad actions
(1), a compromised agent claims a role it doesn't actually have (2).
Detection: `CommunityRole` glob allowlist is matched per-action at
dispatch; `RoleRegistry::with_builtins` + connector-registered roles are
the only sources, no dynamic role injection. Mitigation: role grants are
scoped by the role's declared capability glob; anything outside is a hard
`COOP-9001` (role.capability_denied). Recovery: immediate — per-call
denial.

## `capability/`

Threats: a momentum-tier bump used to unlock a denied capability (2 or 4),
a replay of a per-invocation check (6). Detection: `CapabilityChecker::with_tier`
snapshots the tier at check time, and the resulting `CapabilityDecision`
carries the tick; downstream dispatch verifies the tick hasn't slipped
more than one step. Mitigation: hot-tier capabilities always require an
explicit role declaration, never a bare tier promotion; every grant is
audit-logged via `Sentinel`. Recovery: immediate.

## `mental_model/`

Threats: information leak via model queries by another agent (5), poisoning
by a byzantine member writing false beliefs (3). Detection: queries are
scoped to the querying agent's formation; write operations record
`author_agent_id` and divergent beliefs about the same fact trigger a
`COOP-8001`. Mitigation: persistent mental models go through the
`BackendStore` encrypted SQLite path, so disk state is as private as the
vault; in-memory models are dropped on formation dissolve. Recovery: next
tick after a `replan::refresh` pass.

## `comms/`

Threats: secret exfiltration via the comms channel (5), replay of a
command message (6), unauthorized command injection (1 or 3). Detection:
every `CommMessage` is typed and serde-validated at the boundary; each
message includes `sender` + `seq` and the channel rejects duplicates.
Mitigation: the comms module never serializes `Secret<T>` types (compiler-
enforced by `serde::Serialize` *not* being implemented on `Secret`);
command routes check sender role before dispatch. Recovery: one tick.

## `contract_net/`

Threats: a byzantine bidder flooding bids (3 + 4), a compromised agent
accepting its own bid (2), replay of winning bids (6). Detection: bids
carry `(announcement_id, agent_id)` — duplicates are dropped; the
announcement's expiry tick bounds the bid window. Mitigation: the auctioneer
is the only task that calls `award`, so self-award is impossible; bids
beyond the deadline are rejected before scoring. Recovery: one
announcement window.

## `dissemination/`, `routing/`, `peer.rs`

Threats: gossip amplification DoS (4), spoofed peer identities (3), replay
of join/leave messages (6). Detection: peer messages carry a monotonic
`peer_seq` per sender; routing tables reject duplicates. Mitigation: peer
transport is HMAC'd (shared with the flex chain pool); unknown peers are
logged and dropped, not added. Recovery: next SWIM tick (~1 s via foca).

## `stigmergy/`, `transformation/`

Threats: environment channel drain (4), forged environmental marks by a
malicious connector (1), information leak via RCU snapshots (5). Detection:
every mark carries a writer id and TTL; expired marks are pruned each
tick. Mitigation: RCU writes are bounded by channel size; snapshots are
read-only views, never returning the underlying `Arc` past the requesting
agent's scope. Recovery: one TTL window.

## `replan/`, `supervision/`, `sacrifice/`

Threats: a replan loop used as a DoS (4), supervisor misclassifying an
agent as dead to free its slot (3), sacrifice decisions triggered
maliciously (2). Detection: replan attempts per formation are counted and
escalate to `COOP-8004` after a configured cap; sacrifice requires a
consensus vote (see `consensus.rs`). Mitigation: supervisor liveness uses
foca SWIM with a quorum confirmation before eviction; sacrifice side
effects are recorded in the audit trail. Recovery: bounded by replan cap.

## `utility/`, `action_state.rs`, `pacing/`

Threats: utility score inflation to force a specific action (3), pacing
starvation of low-priority actions (4). Detection: utility scores are
bounded `[0.0, 1.0]` and recomputed per tick from observable state;
pacing quotas are per-role not per-agent so one agent can't hog the role's
budget. Mitigation: action selection records the top-K score with its
inputs, making offline replay possible for post-incident review.
Recovery: one tick.

## `layer/`, `state/`, `types.rs`, `error/`, `command.rs`, `context.rs`

These are shared type modules — no runtime surface to attack directly.
Threats land on the modules above via the types defined here. Review is
therefore `N/A` for primary threats but: error IDs are `COOP-NNNN`
machine-readable (§16.6), `types.rs` exposes no `Secret` types, and
`command.rs` serializes through the same typed pipeline so the boundary
is serde-validated.

---

## Summary: per-attacker aggregate

| Attacker | Covered by |
| --- | --- |
| 1. Malicious connector | role/, capability/, dissemination/, routing/ |
| 2. Compromised agent | formation, capability, recovery, replan, supervision |
| 3. Byzantine member | consensus, attention, interference, mental_model, stigmergy |
| 4. Resource exhaustion | cadence, rally, dissemination, environment, pacing |
| 5. Information leak | awareness, mental_model, comms, stigmergy |
| 6. Replay | cadence, consensus, commit, handoff, contract_net, peer |

All six attacker capabilities are addressed by at least two independent
modules, matching the defense-in-depth posture required by
`docs/arch/SECURITY.md`.
