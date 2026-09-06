# Python bindings (`springtale-py`)

A curated Python facade for Springtale's cooperation primitives. Built
with [pyo3](https://pyo3.rs); shipped as a Python wheel via
[maturin](https://www.maturin.rs).

This is **not** "Springtale in Python". The Rust runtime is the
runtime; nothing here hosts a live bot loop. What Python gets is the
**cooperation model** — formation identity, intent patterns, momentum
tiers, agent IDs. Use it for:

- External tooling that needs to model cooperation state without
  embedding the daemon (analysis scripts, dashboards in Streamlit/
  Gradio, ETL into observability platforms).
- Bridges that read cooperation state from `springtaled`'s HTTP API
  and present it to Python consumers as typed objects.
- Reference / spec correctness — the Python types are auto-derived
  from the Rust types, so version drift is impossible.

If you want to **drive** the daemon from Python — start formations,
deploy connectors, dispatch actions — use the HTTP API instead. See
[`docs/reference/api-clients/python.md`](../reference/api-clients/python.md).

## What's in scope

| Type | What it is |
|---|---|
| `Formation` | Read-only handle: auto-generated id + intent + momentum tier |
| `Intent` | Reconnoiter / Execute / Stabilize / Surge / Dissolve, with payloads |
| `MomentumTier` | Cold / Warming / Hot / Fever enum |
| `FormationId` | 128-bit UUID identity, string-faced |

## What's out of scope

- The runtime (`springtaled` itself).
- Transport, sentinel, connector dispatch.
- The 25-step cooperation tick.
- AI adapters.
- The vault, crypto layer, anything secret-bearing.

If you find yourself wanting any of the above in Python, you want
the HTTP API, not the bindings.

## Documentation

- [Quickstart](quickstart.md) — install + first import + worked example.
- [API reference](api.md) — every type, every method, every gotcha.

## Why is the surface so small

Two reasons:

1. **Threat model.** The daemon holds secrets. The Python process is
   a separate trust domain. We don't want connector credentials,
   vault contents, or AI adapter keys reachable from Python. The
   minimum surface that's useful is types — that's what we ship.
2. **Stability.** A small surface is a slow-changing surface. Python
   downstream consumers don't have to chase Rust API churn on every
   release.

If you have a use case that needs more, file a discussion before
opening a PR. We'd rather expand carefully than expand and then have
to break.
