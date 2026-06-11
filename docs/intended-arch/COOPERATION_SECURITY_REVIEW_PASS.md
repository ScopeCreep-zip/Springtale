# Cooperation security review pass

Companion to `COOPERATION_SECURITY_REVIEW.md`: this file records the
audit-trail status of each module's security paragraph and the
modules' inline `## Security` doc cross-references.

Refresh procedure: whenever a new cooperation module lands, add its
paragraph to the review *and* tick its row here. The two-document
split keeps the review content-rich (one paragraph per module,
attacker-mapped) while this file stays index-shaped (one row per
module, status-only).

## Module coverage table

Each module listed has both:

1. A paragraph in `COOPERATION_SECURITY_REVIEW.md` mapping it to the
   six attacker capabilities from §9.
2. A `## Security` block in the module's own `mod.rs` doc that
   cross-references the review file.

| Module | Review paragraph | Inline `## Security` |
| ------ | ---------------- | -------------------- |
| `cadence/` | ✓ | inherited from review (single-file module) |
| `formation/` (bot crate) | ✓ | inherited from review |
| `momentum/` | ✓ | inherited from review |
| `awareness/` | ✓ | inherited from review |
| `attention/` | ✓ | inherited from review |
| `consensus.rs` | ✓ | inherited from review |
| `commit.rs` | ✓ | inherited from review |
| `rally/` | ✓ | inherited from review |
| `handoff/` | ✓ | inherited from review |
| `interference/` | ✓ | inherited from review |
| `recovery/` | ✓ | inherited from review |
| `role/`, `authority/` | ✓ | inherited from review |
| `capability/` | ✓ | inherited from review |
| `mental_model/` | ✓ | inherited from review |
| `comms/` | ✓ | inherited from review |
| `contract_net/` | ✓ | inherited from review |
| `dissemination/`, `routing/`, `peer.rs` | ✓ | inherited from review |
| `stigmergy/`, `transformation/` | ✓ | inherited from review |
| `replan/`, `supervision/`, `sacrifice/` | ✓ | inherited from review |
| `utility/`, `action_state.rs`, `pacing/` | ✓ | inherited from review |
| `layer/`, `state/`, `types.rs`, `error/`, `command.rs`, `context.rs` | ✓ (N/A — type-only modules) | n/a |
| `gossip/` (G6) | ✓ | inline cross-link at `crates/springtale-cooperation/src/gossip/mod.rs` |
| `memory/` (G2) | ✓ | inline cross-link at `crates/springtale-cooperation/src/memory/mod.rs` |

## Attacker capability coverage (re-derived)

Same table as the bottom of `COOPERATION_SECURITY_REVIEW.md`, updated
with `gossip/` and `memory/` after their addition:

| Attacker | Covered by |
| -------- | ---------- |
| 1. Malicious connector | role/, capability/, dissemination/, routing/, memory/ |
| 2. Compromised agent | formation, capability, recovery, replan, supervision |
| 3. Byzantine member | consensus, attention, interference, mental_model, stigmergy, **gossip** |
| 4. Resource exhaustion | cadence, rally, dissemination, environment, pacing, **gossip**, **memory** |
| 5. Information leak | awareness, mental_model, comms, stigmergy, **gossip**, **memory** |
| 6. Replay | cadence, consensus, commit, handoff, contract_net, peer |

All six attacker capabilities remain covered by ≥2 independent
modules, preserving the defense-in-depth posture
`docs/arch/SECURITY.md` requires.

## Audit log

| Date | Reviewer | Notes |
| ---- | -------- | ----- |
| 2026-05-10 | initial pass | Validated review covers `crates/springtale-cooperation/` as shipped. No drift. |
| 2026-05-10 | J2 (this pass) | Added paragraphs for `gossip/` (G6) and `memory/` (G2). Added inline `## Security` cross-link blocks to both modules' `mod.rs`. Refreshed attacker-capability summary. |
| 2026-06-10 | gap-closure pass | Consensus resolution now APPLIES typed subjects (one-shot permits, pending-vote guard, timeout-deny for destructive) — `consensus.rs` paragraph extended for the execution path. Rally paragraph corrected: WH3 contagion cap lives in `awareness/types.rs` (`MAX_CONTAGION_DISTRESSED`), now AoI-weighted. `recursive.rs`/`subagent.rs` deleted (zero callers). |

## When to refresh

Refresh this file *and* `COOPERATION_SECURITY_REVIEW.md` together
when:

- A new cooperation module lands (`pub mod foo;` in `lib.rs`).
- A module's threat model changes — e.g. a new wire format, a new
  trust-zone boundary, or a new persistence layer.
- A `COOP-XXXX` error variant gains or loses a remediation path
  that's documented as a security control.

Never refresh in response to a single bug fix unless the fix
changes the threat model itself. The review is a posture document,
not a changelog.

## See also

- `docs/intended-arch/COOPERATION_SECURITY_REVIEW.md` — the review
  itself (one paragraph per module).
- `docs/arch/SECURITY.md` — workspace-wide security posture.
- `docs/intended-arch/COOPERATION_IMPLEMENTATION_PLAN.md §9 / §16.7`
  — the implementation contract this pass enforces.
