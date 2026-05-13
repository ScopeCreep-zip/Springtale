# Python quickstart

## Install

### From PyPI (once we ship a release)

```bash
pip install springtale
```

Not yet published. Cut-over coming with the first 0.x release that's
tagged.

### From source

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale

# pip install maturin if you don't have it
pip install maturin

# Build the wheel + install it into the active venv.
maturin develop --release -m crates/springtale-py/Cargo.toml
```

That puts a `springtale.*.so` in your venv's site-packages.

### Build but don't install

```bash
maturin build --release -m crates/springtale-py/Cargo.toml
# Wheel lands in target/wheels/springtale-*.whl
pip install target/wheels/springtale-*.whl
```

## Smoke test

```bash
python -c "import springtale; print(springtale.MomentumTier.HOT)"
```

Should print `MomentumTier.Hot` (Python's `__repr__` for the enum).

## Worked example: visualise momentum across a fleet

You're running several Springtale daemons and you want a single
script that pulls formation state from each, classifies them by
momentum tier, and prints a count.

```python
import requests
from springtale import MomentumTier, Formation, IntentPattern

DAEMONS = [
    ("alpha", "http://10.0.0.10:8080", "TOKEN_ALPHA"),
    ("beta",  "http://10.0.0.11:8080", "TOKEN_BETA"),
    ("gamma", "http://10.0.0.12:8080", "TOKEN_GAMMA"),
]

def fetch_formations(host, token):
    r = requests.get(
        f"{host}/formations",
        headers={"Authorization": f"Bearer {token}"},
        timeout=5,
    )
    r.raise_for_status()
    # Each row from the API matches the Python Formation shape closely.
    # We construct typed instances so downstream code can rely on types.
    return [
        Formation(
            id=row["id"],
            intent=IntentPattern.from_dict(row["intent"]),
            momentum=MomentumTier.from_str(row["momentum_tier"]),
        )
        for row in r.json()
    ]

# Tally by tier across all daemons.
tally = {t: 0 for t in (
    MomentumTier.Cold, MomentumTier.Warming,
    MomentumTier.Hot, MomentumTier.Fever,
)}
for name, host, token in DAEMONS:
    for f in fetch_formations(host, token):
        tally[f.momentum] += 1

for tier, count in tally.items():
    bar = "█" * count
    print(f"{tier.name:>8} {count:>3} {bar}")
```

Run it; you get a quick view of which tiers your fleet is sitting in.

## Worked example: classify intent payloads

```python
from springtale import IntentPattern

intent = IntentPattern.from_dict({
    "kind": "Reconnoiter",
    "task": "monitor github issues for repo radicalkjax/springtale",
})

print(intent.kind)         # 'Reconnoiter'
print(intent.task)         # 'monitor github issues ...'

# Pattern matching style for ad-hoc analysis:
match intent.kind:
    case "Reconnoiter":
        print("read-only intent — no destructive actions expected")
    case "Execute":
        print("active intent — expect connector dispatches")
    case "Stabilize":
        print("maintenance intent — keeps current state")
    case "Surge":
        print("burst intent — high rate, high resource use")
    case "Dissolve":
        print("dissolving — formation winding down")
```

## Type stubs

The wheel ships `.pyi` files. Your IDE / mypy / pyright will pick them
up automatically — autocomplete and type-checking work out of the box.

```bash
mypy your_script.py     # should validate types from springtale.* cleanly
```

## What you CAN'T do

```python
# These do NOT work — they're intentionally out of scope.
springtale.run_daemon(...)        # not exposed
springtale.deploy_formation(...)  # not exposed
springtale.dispatch_action(...)   # not exposed
springtale.read_vault(...)        # not exposed
```

For any of those, use the HTTP API. See
[`docs/reference/api-clients/python.md`](../reference/api-clients/python.md)
for a worked Python client example.

## Versioning

`springtale-py` is versioned alongside the workspace. A wheel built
from commit X is type-compatible with a daemon at the same commit's
HTTP API. Newer wheels against older daemons may have types the
daemon doesn't produce yet (graceful — just gets None back). Older
wheels against newer daemons may miss newly-added fields (also
graceful — the bindings ignore unknown fields).

## Going further

- [`api.md`](api.md) — full API reference.
- [`docs/reference/api-clients/python.md`](../reference/api-clients/python.md) — how to drive the daemon from Python.
- The `crates/springtale-py/src/lib.rs` source — the bindings are
  ~300 lines; reading them is reasonable if you want to know
  exactly what's exposed.
