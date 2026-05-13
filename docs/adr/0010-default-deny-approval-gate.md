# ADR 0010: `DefaultDenyApprovalGate` as the headless default

**Status:** Accepted
**Date:** 2026-05-05

## Context

Sentinel runs four checks per dispatched action: circuit-breaker, rate
limit, dead-man switch, and (the new one) impact classification +
approval gate for destructive actions. The fourth check needs an
`ApprovalGate` trait implementation. Where does the gate live, and
what's the default behaviour when no surface is wired?

Concretely: a headless `springtaled` running on a server has no UI to
prompt a human. When a connector tries to invoke a destructive action
(`RunShell` on a previously-unseen command, deleting a file, sending
a DM to all members of a formation), the gate has to decide *without
asking anyone*.

Two stances:

- **Default allow** — let the action through; rely on capability
  checks and audit trail for accountability.
- **Default deny** — refuse the action; require an explicit gate to
  be wired (e.g. an email-based approval webhook, a desktop prompt,
  a paired chat user signing off).

## Decision

Default deny. `DefaultDenyApprovalGate` ships as `Sentinel::new()`'s
default. The desktop app wires its own gate
(`crates/springtale-runtime/src/cooperation/capability_bridge.rs` →
Tauri command → SafetyPanel prompt). Headless installs must wire a
gate explicitly via `Sentinel::with_approval_gate(…)` if they want
destructive actions to proceed without human in the loop.

## Consequences

Positive:

- A misconfigured headless install can't be exploited to run
  destructive actions silently. The worst case is "destructive
  action denied, audit trail logged".
- Safe for the most vulnerable user: a user who installs the
  CLI and doesn't read the docs end-to-end ends up in the secure
  configuration by default.
- The audit trail records every denied action with reason. Operators
  can see what they would have approved and decide whether to wire
  a gate.
- Matches our broader posture (rustls-only, `Secret<T>` everywhere,
  `forbid(unsafe_code)` on library crates): safety-by-default, with
  ergonomics layered on top.

Negative:

- First-time users of a headless install can be surprised when an
  action they expected to work gets denied. Mitigated: clear error
  ID (E0xxx), runbook in `springtale-cli fix`.
- Power users running automation that *should* be autonomous have to
  write code to wire a gate. The gate trait is simple
  (`async fn decide(&self, req: ApprovalRequest) -> Decision`) but
  it's still code, not config.
- Some test suites had to be updated to wire an `AlwaysAllowApprovalGate`
  in test setup. Annoying but obvious.

Locks in:

- The trait shape: `ApprovalGate`, `ApprovalRequest`, `Decision`.
  External implementations depend on these. Changes need to follow
  the deprecation policy.
- The classification function `impact::classify_impact(&action)` is
  the source of truth for "is this destructive". Bug there silently
  changes which actions get gated.
- Sentinel boot path includes a gate. Removing the requirement would
  be a major version bump.

## Alternatives considered

### Option A — `DefaultDenyApprovalGate` as the default (picked)

Pros and cons enumerated above.

### Option B — `AlwaysAllowApprovalGate` as the default

Pros: nothing breaks for headless users out of the box. "Just works".
Cons: defeats the entire point of having an approval gate. A
misconfigured install gets all the security of having no gate at all,
while pretending to have one. Worst of both worlds.

Why we didn't pick it: the threat model is targeted attacks; an
attacker who lands code execution in a connector should not get
"defaults allow destructive actions" handed to them.

### Option C — Configurable default

Pros: ergonomic. `springtale.toml` chooses.
Cons: same as Option B for users who don't read the config docs. Plus
adds a "but you can flip it off" footgun. Cargo culting the
plaintext config from a tutorial is the dominant failure mode.

Why we didn't pick it: makes the unsafe choice equally available as
the safe one. Defaults matter.

### Option D — No gate at all; rely on capability checks

Pros: simpler. Capability checks already gate `RunShell` per
manifest.
Cons: the capability says "this connector is *allowed* to
RunShell"; it doesn't say "this specific command, at this specific
moment, is the right thing". The gate is the per-invocation
context-aware check, separate from the per-connector capability
grant.

Why we didn't pick it: defence in depth. Capability + impact gate
are different layers and both add value.

### Option E — Sentinel emits a webhook, lets the operator decide externally

Pros: ergonomic for headless installs with operators.
Cons: requires network connectivity *outbound* for the daemon, which
contradicts the "no outbound at idle" promise. Plus the webhook
target becomes a dependency the gate fails closed on.

Why we didn't pick it: outbound dependency in the security path is
not OK. If we add a webhook gate later, it'll be an *additional*
option on top of `DefaultDeny`, not a replacement for it.

## References

- `crates/springtale-sentinel/src/approval.rs` — trait + DefaultDeny
- `crates/springtale-sentinel/src/sentinel.rs` — wiring at construction
- `crates/springtale-sentinel/src/impact.rs` — classification
- `docs/arch/SECURITY.md` §10.2 — the four-check dispatch flow
- `docs/guide/security.md` §7 — user-facing safety table
- Related: ADR 0004 (`Secret<T>`) — same safe-by-default philosophy
