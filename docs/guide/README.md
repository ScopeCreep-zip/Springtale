# Learning Path

New to Springtale? Read these in order. Each builds on the last.

```
  ┌─────────────────────────────────────────────────────────┐
  │                                                         │
  │   1. README.md ─────────────> What is it? Who is it for?│
  │         │                                               │
  │         v                                               │
  │   2. guide/architecture.md ─> How the pieces fit        │
  │         │                                               │
  │         v                                               │
  │   3. guide/security.md ────> How it protects you        │
  │         │                                               │
  │         v                                               │
  │   4. guide/connectors.md ──> What connectors are        │
  │         │                                               │
  │         v                                               │
  │   5. guide/rules.md ───────> How to automate things     │
  │         │                                               │
  │         v                                               │
  │   6. QUICKSTART.md ────────> Build it, run it, try it   │
  │         │                                               │
  │         v                                               │
  │   7. GLOSSARY.md ──────────> Look up anything unfamiliar│
  │                                                         │
  └─────────────────────────────────────────────────────────┘
```

*Fig. 1. Suggested reading order for newcomers.*

## Already know what you're looking for?

| I want to... | Go to |
|---|---|
| Look up a CLI command | [reference/cli.md](../reference/cli.md) |
| Look up an API endpoint | [reference/api.md](../reference/api.md) |
| See a specific connector's triggers/actions | [reference/connectors/](../reference/connectors/) |
| Check a config option | [reference/configuration.md](../reference/configuration.md) |
| Understand a technical term | [GLOSSARY.md](../GLOSSARY.md) |
| Build a new connector | [contributing/adding-a-connector.md](../contributing/adding-a-connector.md) |
| Understand why we chose X over Y | [contributing/design-decisions.md](../contributing/design-decisions.md) |
| See what's shipped vs planned | [ROADMAP.md](../ROADMAP.md) |

## Cooperation deep-dives

Once you've shipped your first bot, these task-oriented guides cover
how Springtale's RTS-style cooperation primitives actually work in
practice. Each is opinionated; read them when you hit the symptom,
not before.

| Topic | Guide |
|---|---|
| L6 intervention — when the orchestrator escalates | [intervention.md](intervention.md) |
| Consensus votes at Fever tier | [consensus.md](consensus.md) |
| Voluntary task yielding (sacrifice) | [sacrifice.md](sacrifice.md) |
| What knowledge persists across formations | [mental-model.md](mental-model.md) |
| Throughput governor (preparation→active→peak→…) | [pacing.md](pacing.md) |
| Cross-formation gossip + outcome propagation | [cross-formation.md](cross-formation.md) |
| When something's off — symptoms → fixes | [troubleshooting-cooperation.md](troubleshooting-cooperation.md) |
