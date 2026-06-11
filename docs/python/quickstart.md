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
python -c "import springtale; print(springtale.MomentumTier.Hot)"
```

Should print `MomentumTier.Hot` (Python's `__repr__` for the enum).

## Worked example: visualise momentum across a fleet

You're running several Springtale daemons and you want a single
script that pulls formation state from each, classifies them by
momentum tier, and prints a count.

```python
import requests
from springtale import MomentumTier

DAEMONS = [
    ("alpha", "http://10.0.0.10:8080", "TOKEN_ALPHA"),
    ("beta",  "http://10.0.0.11:8080", "TOKEN_BETA"),
    ("gamma", "http://10.0.0.12:8080", "TOKEN_GAMMA"),
]

# The bindings don't ship a from_str — map the API's snake_case
# strings to tiers yourself:
TIER = {
    "cold": MomentumTier.Cold,
    "warming": MomentumTier.Warming,
    "hot": MomentumTier.Hot,
    "fever": MomentumTier.Fever,
}
LABEL = {v: k for k, v in TIER.items()}

def fetch_formations(host, token):
    r = requests.get(
        f"{host}/formations",
        headers={"Authorization": f"Bearer {token}"},
        timeout=5,
    )
    r.raise_for_status()
    return r.json()

# Tally by tier across all daemons. Tiers are hashable — fine as keys.
tally = {t: 0 for t in TIER.values()}
for name, host, token in DAEMONS:
    for row in fetch_formations(host, token):
        tally[TIER[row["momentum_tier"]]] += 1

for tier, count in tally.items():
    bar = "█" * count
    print(f"{LABEL[tier]:>8} {count:>3} {bar}")
```

Run it; you get a quick view of which tiers your fleet is sitting in.

## Worked example: classify intent payloads

```python
from springtale import Intent

intent = Intent.reconnoiter(
    target="monitor github issues for repo radicalkjax/springtale",
)

print(intent.kind())       # 'reconnoiter' — note: kind() is a method

# Pattern matching style for ad-hoc analysis. Variant names are
# snake_case, matching the serde tags the HTTP API uses:
match intent.kind():
    case "reconnoiter":
        print("read-only intent — no destructive actions expected")
    case "execute":
        print("active intent — expect connector dispatches")
    case "stabilize":
        print("maintenance intent — keeps current state")
    case "surge":
        print("burst intent — high rate, high resource use")
    case "dissolve":
        print("dissolving — formation winding down")
```

## Type stubs

`.pyi` stubs don't ship yet — the surface is four classes, so reading
[`api.md`](api.md) covers it. Stubs land with the first PyPI release.

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
- The `crates/springtale-py/src/` source — one module per class
  (`momentum.rs`, `intent.rs`, `formation_id.rs`, `formation.rs`);
  reading them is reasonable if you want to know exactly what's
  exposed.
