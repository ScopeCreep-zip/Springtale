# Python API reference

Every type and method exposed by `springtale-py`. Authoritative source
is `crates/springtale-py/src/` (one module per class); this doc is the
user-facing view.

The module exposes exactly four classes — `MomentumTier`, `Intent`,
`FormationId`, `Formation` — plus `__version__`. Python embeds the
cooperation *model* only; the live runtime stays in `springtaled`.

## `springtale.MomentumTier`

Enum representing a formation's momentum tier (the capability gate from
`COOPERATION.md §7`).

```python
from springtale import MomentumTier

MomentumTier.Cold       # newly created, no successful cooperation yet
MomentumTier.Warming    # ≥3 successful ticks
MomentumTier.Hot        # ≥8 successful ticks, no interference
MomentumTier.Fever      # ≥15 successful ticks, no interference
```

Frozen (`frozen` in pyo3) — you can't mutate. Supports `==` between
members and against ints (`eq, eq_int`), and members are hashable, so
tiers work as `dict` keys.

There is no `from_str` constructor. If you're mapping API strings to
tiers, build the dict yourself:

```python
TIER = {
    "cold": MomentumTier.Cold,
    "warming": MomentumTier.Warming,
    "hot": MomentumTier.Hot,
    "fever": MomentumTier.Fever,
}
```

## `springtale.Intent`

A formation's high-level goal. Five variants, built through static
factory methods. Each payload is a plain Python string — the Rust
newtype layer (`TaskDescriptor`, `PlanId`, `StabilizeReason`,
`DissolveReason`) is collapsed so callers don't model every newtype.

```python
from springtale import Intent

i = Intent.reconnoiter(target="github/issues")
i = Intent.execute(plan_id="plan-2026-05-11-incident-response")
i = Intent.execute()                       # plan_id=None — orchestrator picks
i = Intent.stabilize(reason="cooldown after surge")
i = Intent.surge(objective="ship the release")
i = Intent.dissolve(reason="task complete")
```

| Method | Returns | Description |
|---|---|---|
| `Intent.reconnoiter(target: str)` | `Intent` | Gather information. `target` describes what to observe (`"news/feed"`, `"github/issues"`, …). Required. |
| `Intent.execute(plan_id: str \| None = None)` | `Intent` | Act on a known plan. `plan_id` is opaque; `None` lets the orchestrator pick. |
| `Intent.stabilize(reason: str)` | `Intent` | Defensive hold. `reason` documents why the formation is pausing. Required. |
| `Intent.surge(objective: str)` | `Intent` | Maximum commitment to one objective. Required. |
| `Intent.dissolve(reason: str)` | `Intent` | Graceful wind-down. `reason` is recorded into the global knowledge store. Required. |
| `kind()` | `str` | Variant name — `"reconnoiter" \| "execute" \| "stabilize" \| "surge" \| "dissolve"`. Matches the snake_case serde tags the rest of the system uses. Note: a method, not a property. |

Frozen — instances are immutable.

## `springtale.FormationId`

Formation identity. Wraps the same 128-bit UUID the rest of the system
uses; Python sees it as a string.

```python
from springtale import FormationId

fid = FormationId()                                          # fresh random id
fid = FormationId.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479")
str(fid)        # canonical UUID string
```

| Method | Returns | Description |
|---|---|---|
| `FormationId()` | `FormationId` | Generate a fresh formation id. |
| `FormationId.parse(s: str)` | `FormationId` | Parse from the canonical UUID string. Raises `ValueError` on invalid input. |
| `__str__()` | `str` | Canonical UUID string form. |
| `__eq__(other)` / `__hash__()` | — | Usable in `dict` / `set`. |

## `springtale.Formation`

Lightweight, read-only formation handle. Mirrors the `FormationView`
gossip record without the live runtime hookup — pure-Python use is
scripting and simulation; `springtaled` owns the real thing.

```python
from springtale import Formation, Intent

f = Formation(Intent.reconnoiter(target="watch issues"))
f.id              # FormationId (auto-generated)
f.intent          # the Intent you passed
f.momentum_tier   # MomentumTier.Cold — formations always start Cold
```

Constructor takes exactly one argument:

| Param | Type | Description |
|---|---|---|
| `intent` | `Intent` | The formation's intent. The id is auto-generated and `momentum_tier` starts at `Cold` — momentum is *earned* through cooperation ticks, never assigned. |

Properties (read-only): `id: FormationId`, `intent: Intent`,
`momentum_tier: MomentumTier`.

## Module-level constants

```python
springtale.__version__        # str matching the workspace crate version
```

## Errors

The bindings raise `ValueError` on invalid input — e.g.
`FormationId.parse("not-a-uuid")`. We deliberately don't define custom
exception classes; catching `ValueError` works.

## Thread safety

All types are immutable (`frozen` in pyo3). Sharing instances across
threads is safe.

## Caveats

- **The bindings don't connect to a daemon.** They're pure types.
  Anything that needs daemon state has to fetch over HTTP first — see
  [`docs/reference/api-clients/python.md`](../reference/api-clients/python.md).
- **No async.** The bindings are synchronous because they don't do
  I/O. The types are usable from async code without ceremony.
- **No dict round-tripping.** There is no `from_dict` / `to_dict`;
  parse API JSON yourself and construct via the factory methods.
- **No pickle.** pyo3 frozen classes don't pickle by default.

## Examples in repository

See the worked CLI example at
`apps/springtale-cli/examples/llm-swarm.rs` for the Rust side that
produces the data these Python types model.
