# Learning Path

New to Springtale? Read these in order. Each builds on the last.

```
  ┌─────────────────────────────────────────────────────────┐
  │                                                         │
  │   1. README.md ─────────────> What is it? Who is it for?│
  │         │                                               │
  │         v                                               │
  │   2. installation/ ────────> Pick how you'll run it     │
  │         │                                               │
  │         v                                               │
  │   3. QUICKSTART.md ────────> First bot in 60 seconds    │
  │         │                                               │
  │         v                                               │
  │   4. guide/architecture.md ─> How the pieces fit        │
  │         │                                               │
  │         v                                               │
  │   5. guide/security.md ────> How it protects you        │
  │         │                                               │
  │         v                                               │
  │   6. guide/connectors.md ──> What connectors are        │
  │         │                                               │
  │         v                                               │
  │   7. guide/rules.md ───────> How to automate things     │
  │         │                                               │
  │         v                                               │
  │   8. tutorials/ ───────────> Build something real       │
  │         │                                               │
  │         v                                               │
  │   9. GLOSSARY.md ──────────> Look up anything unfamiliar│
  │                                                         │
  └─────────────────────────────────────────────────────────┘
```

*Fig. 1. Suggested reading order for newcomers.*

## Already know what you're looking for?

| I want to... | Go to |
|---|---|
| Install Springtale | [installation/](../installation/) |
| First bot fast | [QUICKSTART.md](../QUICKSTART.md) |
| Multi-step tutorials | [tutorials/](../tutorials/) |
| Cookbook recipes for common patterns | [cookbook/](../cookbook/) |
| Look up a CLI command | [reference/cli.md](../reference/cli.md) |
| Look up an API endpoint | [reference/api.md](../reference/api.md) |
| API client examples | [reference/api-clients/](../reference/api-clients/) |
| See a specific connector's triggers/actions | [reference/connectors/](../reference/connectors/) |
| Check a config option | [reference/configuration.md](../reference/configuration.md) |
| Performance characteristics | [reference/performance.md](../reference/performance.md) |
| Use the Python bindings | [python/](../python/) |
| Understand a technical term | [GLOSSARY.md](../GLOSSARY.md) |
| Frequently asked questions | [FAQ.md](../FAQ.md) |
| Build a new connector | [contributing/adding-a-connector.md](../contributing/adding-a-connector.md) |
| Add a transport / AI adapter / sentinel check | [contributing/extension-points.md](../contributing/extension-points.md) |
| Understand why we chose X over Y | [contributing/design-decisions.md](../contributing/design-decisions.md) + [adr/](../adr/) |
| Operate a daemon long-term | [operations/](../operations/) |
| See what's shipped vs planned | [ROADMAP.md](../ROADMAP.md) |

## Cooperation deep-dives

Once you've shipped your first bot, these task-oriented guides cover
how Springtale's RTS-style cooperation primitives actually work in
practice. Each is opinionated; read them when you hit the symptom,
not before.

| Topic | Guide |
|---|---|
| Why bots cooperate (the overall model) | [cooperation.md](cooperation.md) |
| L6 intervention — when the orchestrator escalates | [intervention.md](intervention.md) |
| Consensus votes at Fever tier | [consensus.md](consensus.md) |
| Voluntary task yielding (sacrifice) | [sacrifice.md](sacrifice.md) |
| What knowledge persists across formations | [mental-model.md](mental-model.md) |
| Throughput governor (preparation→active→peak→…) | [pacing.md](pacing.md) |
| Cross-formation gossip + outcome propagation | [cross-formation.md](cross-formation.md) |
| When something's off — symptoms → fixes | [troubleshooting-cooperation.md](troubleshooting-cooperation.md) |

## Recipes + observability (W- and Phase A/B/D1)

The newer surfaces — click-and-play recipes, the executions log,
external workspaces, dedupe semantics for polling.

| Topic | Guide |
|---|---|
| Click-and-play recipes (browse, fill, deploy) | [recipes.md](recipes.md) |
| Recipe data shape reference | [../reference/recipes-format.md](../reference/recipes-format.md) |
| Authoring tools — preflight, preview, test-step, selector picker | [recipe-authoring-tools.md](recipe-authoring-tools.md) |
| Phase A — `Action::Dedupe` and `Action::Extract` for polling | [dedupe-and-extract.md](dedupe-and-extract.md) |
| Phase B — per-fire executions log + drift detection | [executions-and-drift.md](executions-and-drift.md) |
| D1 — discovered chat destinations (workspace directory) | [external-workspaces.md](external-workspaces.md) |

## Safety and OPSEC

Springtale targets users whose threat model is hostile attention.
These pages cover the trust + privacy posture in depth.

| Topic | Where |
|---|---|
| Vulnerability disclosure policy | [SECURITY.md](../../SECURITY.md) |
| Threat model in plain language | [threat-model-faq.md](../threat-model-faq.md) |
| Running in adversarial environments | [opsec.md](../opsec.md) |
| Anonymous contribution | [anonymous-contribution.md](../anonymous-contribution.md) |
| Privacy policy (the formal one) | [privacy-policy.md](../privacy-policy.md) |
| Backup, restore, host migration | [operations/backup-and-restore.md](../operations/backup-and-restore.md), [operations/host-migration.md](../operations/host-migration.md) |

## Architecture Decision Records

For "why did you pick X?" questions. See [adr/](../adr/) for the
full index. Highlights:

| Decision | ADR |
|---|---|
| rusqlite over sqlx | [0001](../adr/0001-rusqlite-not-sqlx.md) |
| Wasmtime sandbox | [0002](../adr/0002-wasmtime-not-wasmer.md) |
| rustls only; native-tls banned | [0003](../adr/0003-rustls-only-no-native-tls.md) |
| `Secret<T>` discipline | [0004](../adr/0004-secrecy-crate-for-secrets.md) |
| Cooperation as separate crate | [0005](../adr/0005-cooperation-as-separate-crate.md) |
| Declarative schema over migrations | [0006](../adr/0006-declarative-schema-over-migrations.md) |
| Axum for HTTP | [0007](../adr/0007-axum-over-actix.md) |
| Tauri for desktop | [0008](../adr/0008-tauri-over-electron.md) |
| Veilid for Phase 3 P2P | [0009](../adr/0009-veilid-for-phase-3.md) |
| `DefaultDenyApprovalGate` | [0010](../adr/0010-default-deny-approval-gate.md) |
