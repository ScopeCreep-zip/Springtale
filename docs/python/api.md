# Python API reference

Every type and method exposed by `springtale-py`. Authoritative source
is `crates/springtale-py/src/lib.rs`; this doc is the user-facing
view.

## `springtale.MomentumTier`

Enum representing a formation's momentum.

```python
from springtale import MomentumTier

MomentumTier.Cold       # newly created, no successful cooperation yet
MomentumTier.Warming    # ≥3 successful ticks
MomentumTier.Hot        # ≥8 successful ticks, no interference
MomentumTier.Fever      # ≥15 successful ticks, no interference
```

Methods:

| Method | Returns | Description |
|---|---|---|
| `MomentumTier.from_str(s: str)` | `MomentumTier` | Parse from `"Cold"` / `"Warming"` / `"Hot"` / `"Fever"`. Raises `ValueError` on unknown. |
| `MomentumTier.__str__()` | `str` | Round-trip with `from_str`. |
| `MomentumTier.__eq__(other)` | `bool` | Identity comparison. |

Properties (read-only):

- `MomentumTier.name` — `"Cold" | "Warming" | "Hot" | "Fever"`.

Frozen (`frozen=True` in pyo3) — you can't mutate.

## `springtale.AgentId`

Opaque integer-packed identity for an agent within a formation.

```python
from springtale import AgentId

a = AgentId(42)
print(a)            # 'AgentId(42)'
print(a.as_int())   # 42
```

Methods:

| Method | Returns | Description |
|---|---|---|
| `AgentId(value: int)` | `AgentId` | Construct. `value` must be a non-negative int. |
| `as_int()` | `int` | Underlying integer. |
| `__eq__(other)` | `bool` | Identity comparison. |
| `__hash__()` | `int` | Usable in `dict` / `set`. |

## `springtale.IntentPattern`

A formation's high-level goal. Variants:

- `Reconnoiter` — read-only intent (monitor / observe).
- `Execute` — active intent (take action).
- `Stabilize` — maintenance intent (maintain current state).
- `Surge` — burst intent (maximum effort, burn rally tokens freely).
- `Dissolve` — formation winding down.

Each variant may carry a payload string — task description, plan ID,
reason. The Rust types use newtypes (`TaskDescriptor`, `PlanId`,
`StabilizeReason`, `DissolveReason`); the Python facade collapses
them to `Optional[str]`.

```python
from springtale import IntentPattern

i = IntentPattern.reconnoiter(task="watch github issues")
i = IntentPattern.execute(plan="plan-2026-05-11-incident-response")
i = IntentPattern.stabilize(reason="cooldown after surge")
i = IntentPattern.surge()
i = IntentPattern.dissolve(reason="task complete")
```

Methods:

| Method | Returns | Description |
|---|---|---|
| `IntentPattern.reconnoiter(task: Optional[str] = None)` | `IntentPattern` | Construct Reconnoiter. |
| `IntentPattern.execute(plan: Optional[str] = None)` | `IntentPattern` | Construct Execute. |
| `IntentPattern.stabilize(reason: Optional[str] = None)` | `IntentPattern` | Construct Stabilize. |
| `IntentPattern.surge()` | `IntentPattern` | Construct Surge. |
| `IntentPattern.dissolve(reason: Optional[str] = None)` | `IntentPattern` | Construct Dissolve. |
| `IntentPattern.from_dict(d: dict)` | `IntentPattern` | Parse from `{"kind": "Reconnoiter", "task": "..."}` shape used by the HTTP API. |
| `IntentPattern.to_dict()` | `dict` | Round-trip back. |
| `IntentPattern.kind` (property) | `str` | The variant name. |
| `IntentPattern.payload` (property) | `Optional[str]` | The variant payload, or `None`. |

`IntentPattern.kind` aliases: `intent.task` / `intent.plan` /
`intent.reason` map to `payload` for the variants where those names
match the Rust newtype.

## `springtale.Formation`

A formation's identity + intent + momentum tier.

```python
from springtale import Formation, IntentPattern, MomentumTier

f = Formation(
    id="f47ac10b-58cc-4372-a567-0e02b2c3d479",
    intent=IntentPattern.reconnoiter(task="watch issues"),
    momentum=MomentumTier.Warming,
)
```

Constructor parameters:

| Param | Type | Description |
|---|---|---|
| `id` | `str` | UUID string. Must parse as a valid UUID. |
| `intent` | `IntentPattern` | The formation's current intent. |
| `momentum` | `MomentumTier` | The formation's current momentum tier. |

Properties (read-only):

| Property | Type | Description |
|---|---|---|
| `id` | `str` | The UUID. |
| `intent` | `IntentPattern` | The intent. |
| `momentum` | `MomentumTier` | The momentum tier. |

Methods:

| Method | Returns | Description |
|---|---|---|
| `Formation.from_dict(d: dict)` | `Formation` | Parse from the shape returned by `GET /formations/{id}`. |
| `to_dict()` | `dict` | Round-trip back. |
| `__eq__(other)` | `bool` | Equal iff id, intent, and momentum match. |
| `__repr__()` | `str` | Includes id (truncated) + intent kind + momentum tier. |

## Module-level constants

```python
springtale.__version__        # str matching the workspace crate version
springtale.SCHEMA_VERSION     # int matching the daemon's current schema version
```

## Errors

All bindings raise `ValueError` on invalid input. Examples:

- `IntentPattern.from_dict({})` → `ValueError: missing kind`
- `MomentumTier.from_str("Smoldering")` → `ValueError: unknown tier 'Smoldering'`
- `Formation(id="not-a-uuid", ...)` → `ValueError: invalid UUID`

We deliberately don't define custom exception classes. Catching
`ValueError` works.

## Thread safety

All types are immutable (`frozen=True` in pyo3). Sharing instances
across threads is safe. Pass instances freely; the GIL handles
reference counting.

## Caveats

- **The bindings don't connect to a daemon.** They're pure types.
  Anything that needs daemon state has to fetch over HTTP first.
- **No async.** The bindings are synchronous because they don't do
  I/O. If you want async HTTP to a daemon, use `httpx` or `aiohttp`;
  the types are usable from async code without ceremony.
- **`from_dict` is strict-ish.** Extra fields are ignored; missing
  required fields raise `ValueError`. Mismatched types raise
  `TypeError` from the underlying pyo3 conversion.
- **No serialization to/from arbitrary formats.** We provide
  `to_dict()` (matches the HTTP API JSON shape); pickle isn't
  supported (pyo3 enums + frozen classes don't pickle by default).

## Examples in repository

See the worked CLI example at
`apps/springtale-cli/examples/llm-swarm.rs` for the Rust side that
produces the data these Python types model.
