# When Something Goes Wrong

Every error Springtale surfaces has a stable ID. The CLI's
`springtale fix <id>` command prints causes, suggestions, and — for a
handful of high-leverage errors — runs an automated fix.

## Two error namespaces

| Prefix | Scope | Examples |
|--------|-------|----------|
| `E001`–`E009` | Operational (runtime, storage, daemon) | `E001 store`, `E003 connector`, `E009 init` |
| `COOP-1001`…`COOP-9003` | Cooperation layer (formations, tiers, rally, recovery) | `COOP-2003 not viable`, `COOP-8001 rally exhausted` |

Everything routes through the same command:

```bash
springtale fix E001
springtale fix COOP-2003
```

## Operational errors (E001–E009)

| ID | Title | Auto-fix? |
|----|-------|-----------|
| E001 | Store error — database access failure | ✓ (permission + init) |
| E002 | Rule error — invalid rule definition | — |
| E003 | Connector error — execution failure | — |
| E004 | Formation error — multi-agent group issue | — |
| E005 | Not found — missing resource | — |
| E006 | Validation error — invalid input | — |
| E007 | Serialization — JSON/TOML parsing failure | — |
| E008 | AI error — adapter failure | — |
| E009 | Initialization — startup failure | ✓ (diagnostics) |

## Cooperation-layer errors (COOP-NNNN)

IDs are grouped by module:

### Cadence (COOP-1xxx)
- `COOP-1001` tick bus channel closed
- `COOP-1002` tick sequence wrapped
- `COOP-1003` subscriber lagged

### Formation (COOP-2xxx)
- `COOP-2001` agent not found
- `COOP-2002` empty formation
- `COOP-2003` formation not viable
- `COOP-2004` missing required capability
- `COOP-2005` formation context not initialized

### Momentum (COOP-3xxx)
- `COOP-3001` momentum insufficient (need Hot, have Cold)
- `COOP-3002` capability locked at current tier

### Awareness (COOP-4xxx)
- `COOP-4001` stale neighbor
- `COOP-4002` gossip bridge disconnected

### Consensus (COOP-5xxx)
- `COOP-5001` no override tokens remaining
- `COOP-5002` consensus deadline expired
- `COOP-5003` vote not found

### Commit (COOP-6xxx)
- `COOP-6001` commit barrier failed
- `COOP-6002` prepare timed out
- `COOP-6003` participant dropped
- `COOP-6004` agent not a participant in this barrier
- `COOP-6005` execution phase failed

### Interference (COOP-7xxx)
- `COOP-7001` cross-agent conflict detected (ResourceConflict / ActionNegation)

### Rally (COOP-8xxx)
- `COOP-8001` rally exhausted (no tokens remaining)
- `COOP-8002` cascade threshold exceeded
- `COOP-8003` rally supervisor panicked

### Recovery (COOP-9xxx)
- `COOP-9001` no recovery path
- `COOP-9002` recovery cost exceeds budget
- `COOP-9003` terminal failure (max quick-fixes reached)

## Reading the `springtale fix` output

Each guide reports four things:

1. **Title** — one-line summary of what the error class means.
2. **Causes** — specific things that typically produce this error.
3. **Suggestions** — what you can do. Cheap first, expensive last.
4. **Auto-fix** — if present, `springtale fix <id>` will run it
   and print success/no-change.

Example:

```
$ springtale fix COOP-8001
─── COOP-8001: Rally — no tokens remaining ─────────────────
Causes:
  · The formation has exhausted its rally budget (Monster Hunter
    cart threshold).
Suggestions:
  1. The formation will escalate to orchestrator intervention.
  2. Check `springtale trace` for the cascade reason.
Auto-fix: none for this error.
```

## When the fix command doesn't help

`springtale fix` is a triage tool. When it doesn't have anything
useful to say, two fallbacks:

- **`springtale doctor`** runs full diagnostics (store integrity,
  connector reachability, vault key presence, daemon liveness).
- **`springtale trace`** streams live execution — every trigger, action
  dispatch, and sentinel verdict with the connector, rule, and duration.
  `springtale events` lists what already happened.

Both are read-only and safe to run on a live bot.

## Reporting an error

If you hit an error with no corresponding guide, file an issue at
`https://github.com/anthropics/springtale/issues` with:

- The exact error ID.
- The last 200 lines of `springtale trace` (or `springtale events --limit 200`).
- What you were doing when it fired.

We add new guides to `crates/springtale-runtime/src/operations/error_fixes.rs`
on request. Every new error variant is expected to ship with a
`FixGuide` entry — the tests enforce it.
