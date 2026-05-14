# Executions log + drift detection (Phase B)

Springtale's older observability surfaces — `events` for trigger
metadata, `execution_results` for per-action output, `audit_trail`
for sentinel verdicts — answer **separate** questions. None of them
answer the one users actually ask:

> "Which agent ran which step in which formation, and what happened?"

**Phase B** answers that. The `executions` table records one row
per chain fire. The `execution_steps` table records each step
inside that chain. Together they give per-fire lifecycle visibility,
cooperation-aligned (every row has `agent_id`, `formation_id`,
`momentum_tier`), with a privacy posture that's stricter than
Apify or n8n by default.

**Drift detection** sits on top: classify the trend in latency,
success rate, and refusal rate over recent executions to surface
"this recipe has been getting slower" or "this rule started
returning 50% errors yesterday."

## ExecutionContext — the cooperation envelope

Every chain fire carries an `ExecutionContext` that scopes it to a
cooperation context:

```rust
// crates/springtale-cooperation/src/execution.rs
pub struct ExecutionContext {
    pub execution_id:  ExecutionId,    // ULID — sortable by time
    pub agent_id:      AgentId,
    pub formation_id:  FormationId,
    pub momentum_tier: MomentumTier,
    pub rule_id:       RuleId,
    pub mode:          ExecutionMode,  // Normal | DryRun
}

pub struct ExecutionId(pub ulid::Ulid);
```

The envelope lives in `springtale-cooperation` (not core) because it
references cooperation types. `ChainContext` from
`springtale-core::rule` holds **within-fire step state**;
`ExecutionContext` holds the **cooperation envelope**. Dispatchers
thread both.

Without `ExecutionContext`:

- Two agents in the same formation running the same dedupe rule
  collide on a global key.
- Sentinel throttling can't scale with momentum — a Fever-tier swarm
  is throttled like a Cold observer.
- The executions log can't answer "what did agent X in formation Y
  do on Sunday morning?".

### Why ULID

`ExecutionId` is a [`ulid::Ulid`](https://github.com/ulid/spec), not
a UUID v4. ULIDs are lexicographically sortable by creation time,
which means:

```sql
SELECT * FROM executions
 WHERE bot_id = ?
 ORDER BY id DESC
 LIMIT 50;
```

…is index-friendly without an extra `started_at` sort key. UUID v4
wastes 122 bits of random for a primary key the database already
sorts by insert time.

## Schema layout

Two tables, both in `crates/springtale-store/src/schema/sql/executions.sql`:

```
┌───────────────────────────────────────────────────────────┐
│  executions                                               │
│  ─────────                                                │
│   id              ULID   PRIMARY KEY                      │
│   bot_id          TEXT                                    │
│   formation_id    TEXT   nullable                         │
│   agent_id        INT    nullable                         │
│   rule_id         TEXT                                    │
│   recipe_id       TEXT   nullable (set on recipe fires)   │
│   momentum_tier   TEXT   "cold" | "warming" | "hot" | …   │
│   mode            TEXT   "normal" | "dry_run"             │
│   status          TEXT   "running" | "success" | "error"  │
│                          | "empty" | "suppressed"         │
│   started_at      TEXT   ISO-8601                         │
│   finished_at     TEXT   nullable until status terminal   │
│   error_kind      TEXT   nullable, enum tag string only   │
│   summary_bytes   INT    total size processed, no content │
└───────────────────────────────────────────────────────────┘
              │
              │ ON DELETE CASCADE
              ▼
┌───────────────────────────────────────────────────────────┐
│  execution_steps                                          │
│  ────────────                                             │
│   execution_id    ULID   FOREIGN KEY                      │
│   step_index      INT                                     │
│   action_kind     TEXT   "Connector" | "Transform" | …    │
│   status          TEXT   "success" | "error" | "suppressed"│
│   started_at      TEXT                                    │
│   finished_at     TEXT                                    │
│   input_bytes     INT    no content                       │
│   output_bytes    INT    no content                       │
│   error_kind      TEXT   nullable                         │
└───────────────────────────────────────────────────────────┘
```

### How `executions` differs from the legacy tables

| Table | Records | Scope | Status |
|---|---|---|---|
| `events` | Trigger metadata (Cron fired at T; webhook arrived) | Per trigger | Legacy, kept |
| `execution_results` | Per-action output payload | Per action, capped 100 per connector | Legacy, kept |
| `audit_trail` | Sentinel verdicts (allow / deny / panic-wipe) | Per dispatched action | Stays |
| `executions` | **Chain lifecycle** — one row per ChainContext fire | Per fire, cooperation-aligned | Phase B (new) |
| `execution_steps` | **Per-step lifecycle** within a chain | One row per step | Phase B (new) |

The legacy tables aren't going away. The executions log is the
cooperation-aligned observability layer that the older tables
couldn't answer.

## Privacy defaults

The executions log is **sizes only by default**. Stricter than
Apify and n8n:

- `input_bytes` / `output_bytes` are counts. The actual payloads
  are **not** in this table.
- `error_kind` is an enum tag (`"timeout"`, `"capability_denied"`,
  `"parse_error"`) — never the full error message. Full text stays
  in `tracing` logs.
- `summary_bytes` is the total bytes processed by the chain. Useful
  for quota accounting; reveals nothing about content.

The retention default is **14 days**, swept by a background task.
Content retention (the legacy `execution_results` table or the
planned Phase C blob store) is a separate, opt-in code path.

## Statuses

| Status | Meaning |
|---|---|
| `running` | Chain dispatched, not yet terminal |
| `success` | All steps completed without error |
| `error` | A step failed; `error_kind` set on the row |
| `empty` | Trigger fired, conditions matched, but the chain produced no work (e.g. dedupe suppressed every step) |
| `suppressed` | Sentinel rate-limited or otherwise blocked the chain |

The `empty` status is important: a dedupe-aware polling recipe
that runs hourly and finds no new items emits an `empty` row, not
a `success`. The drift detector uses this distinction (an empty
rate trending up means the source isn't producing data; a success
rate trending down means real failures).

## Drift detection

Drift answers "how is this recipe trending over time?" It runs
against the executions log without persisting anything — the
report is computed on demand.

```rust
// crates/springtale-runtime/src/operations/executions/drift.rs
pub struct DriftReport {
    pub latency:      LatencyDrift,   // p50 / p95 / p99 over the window
    pub rate:         RateDrift,      // success / error / empty / suppressed counts
    pub classification: DriftClass,   // Stable | Improving | Degrading | Volatile
}

pub enum DriftClass {
    Stable,         // within tolerance
    Improving,      // latency down OR success rate up
    Degrading,      // latency up OR success rate down OR refusals up
    Volatile,       // high variance, no clear trend
}
```

The filter is a window:

```rust
pub struct DriftFilter {
    pub window_hours: u32,         // typically 24 or 168 (1w)
    pub min_samples:  usize,       // refuses to classify below this
    pub bot_id:       Option<String>,
    pub formation_id: Option<FormationId>,
}
```

Two entry points:

- `recipe_drift(&store, recipe_id, filter)` — all chains tagged
  with the given `recipe_id`.
- `rule_drift(&store, rule_id, filter)` — all chains tagged with
  the given `rule_id`.

If the window has fewer than `min_samples` executions, the report
comes back with `DriftClass::Stable` and a `confidence: "low"`
note — the UI shows "not enough data yet."

## UI surface

Two components in the desktop shell:

| Component | What it shows |
|---|---|
| `ExecutionsPanel.tsx` | Recent executions list (newest first), per-formation filter, click-through to step rows |
| `DriftBadge.tsx` | Small badge on recipe / rule cards showing the current `DriftClass` color: green (Stable / Improving), yellow (Volatile), red (Degrading) |

The badge polls every minute. Click-through opens the full drift
report in a side panel.

## API surface

Currently Tauri-IPC + runtime ops. HTTP exposure is planned but
not shipped — see [`docs/reference/api.md`](../reference/api.md)
for the as-built HTTP surface.

| Operation | Tauri command |
|---|---|
| List executions | `list_executions` |
| Get steps for one | `get_execution_steps` |
| Vacuum old rows | `vacuum_executions` |
| Recipe drift report | `get_recipe_drift` |
| Rule drift report | `get_rule_drift` |

The runtime layer is `crates/springtale-runtime/src/operations/executions/`:

- `recorder.rs` — `ExecutionRecorder` trait + `StoreRecorder` (writes to SQLite) + `NoopRecorder` (tests)
- `query.rs` — list / get_steps / vacuum
- `drift.rs` — `recipe_drift` / `rule_drift` / `DriftReport`

The dispatcher calls `ExecutionRecorder::record_start()` at fire
time, `record_step()` per step, `record_finish()` at the end. The
trait owns the privacy posture — `StoreRecorder` writes sizes only;
a future `Phase C` content recorder would opt into blob retention.

## Common queries

### What did agent 3 in formation X do today?

```rust
list_executions(ExecutionFilter {
    formation_id: Some(x),
    agent_id: Some(AgentId(3)),
    since: chrono::Duration::hours(24),
    ..Default::default()
})
```

### Which recipes are degrading?

Iterate `list_recipes()` → `recipe_drift(id, filter)` → filter for
`DriftClass::Degrading`. The dashboard's recipe library does this
to render the drift badge per card.

### What's the error rate for rule R over the last week?

```rust
rule_drift(&store, r, DriftFilter {
    window_hours: 168,
    min_samples: 30,
    ..
})
```

Inspect `report.rate.error_rate`. Confidence is `"low"` if fewer
than 30 fires in the window.

## Vacuum and retention

The background sweeper runs once per hour:

```rust
springtale_runtime::operations::executions::vacuum_executions(
    &store,
    DEFAULT_RETENTION_MS,  // 14 days
)
```

Rows older than the retention threshold are deleted. The
constant is exported from `recorder.rs`; for now it's not
config-driven. Reducing retention is a planned config knob in
Phase C.

## Phase C — opt-in content retention

The current Phase B ships **sizes only**. The follow-up Phase C
adds optional content retention via a separate KV blob store:

- Recipe author declares `retain_content: true` on a step.
- Sentinel approval gate checks the destructive-action classification
  (content retention is reversible but high-impact).
- Approved → step's input / output payloads are blob-stored with a
  capped TTL.
- `execution_steps.input_bytes` / `output_bytes` carry a blob ref;
  the UI offers an "expand" click that requires the user to
  re-confirm (`ActionImpact::Destructive` per
  `crates/springtale-sentinel/src/impact.rs`).

Phase C is out of scope for the current release.

## Gotchas

- **`DryRun` mode never persists.** Test-this-step
  (`runtime::operations::test_step`) sets `ExecutionContext::mode =
  DryRun`; the recorder skips the insert. Drift reports filter
  these out by default.
- **Vacuuming is unconditional.** It doesn't honour per-recipe
  retention overrides yet. A recipe that wants longer history needs
  to dump and re-import; Phase C improves this.
- **`empty` status isn't an error.** A polling recipe with dedupe
  enabled producing 23 `empty` rows in a row is the *expected*
  shape. The drift detector classifies empties separately from
  errors.
- **Status `running` for too long is a bug.** If
  `record_finish()` doesn't fire (daemon crash mid-chain),
  rows stay `running`. The vacuum sweeper flips orphans to
  `error` with `error_kind = "orphaned"` after a 1h grace window.
- **`error_kind` is bounded.** New error categorisations require a
  Rust enum bump in `executions/recorder.rs`. Don't expect a
  free-form error string to land in this column.

## See also

- [`docs/guide/dedupe-and-extract.md`](dedupe-and-extract.md) — the
  Phase A `Action::Dedupe` + `Action::Extract` that produce most of
  the `empty` rows.
- [`docs/guide/external-workspaces.md`](external-workspaces.md) — D1
  workspace directory referenced by the execution context.
- [`docs/reference/recipes-format.md`](../reference/recipes-format.md) —
  recipe schema (drift surfaces on the recipe card).
- `crates/springtale-cooperation/src/execution.rs` — ExecutionContext source
- `crates/springtale-runtime/src/operations/executions/` — operations layer
- `crates/springtale-store/src/schema/sql/executions.sql` — schema
