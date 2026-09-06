# Tutorials

Multi-step guides that take you from a clean install to a working
bot doing real work. Read [`docs/QUICKSTART.md`](../QUICKSTART.md)
first to make sure you have a daemon running; tutorials build on top.

| # | Tutorial | What you'll build | Time | Skill |
|---|---|---|---|---|
| [01](01-moderation-bot.md) | Telegram moderation bot | A bot that watches a group, flags slurs, escalates to a human on second offence | 45 min | Beginner |
| [02](02-cross-platform-alerts.md) | Cross-platform stream alerts | Kick stream goes live → Bluesky post + Discord ping + Telegram DM | 30 min | Beginner |
| [03](03-llm-research-swarm.md) | LLM research swarm | A 3-agent formation that researches a topic, drafts a writeup, and critiques it | 60 min | Intermediate |

Each tutorial:

- Starts from a fresh install. Skips ahead to picking a template if
  the previous tutorial set you up already.
- Walks the rule TOML line by line. Doesn't just paste a finished
  file.
- Tells you what to expect at each step + what to do if it doesn't
  match.
- Ends with an "extend this" section pointing at related recipes in
  the [cookbook](../cookbook/).

## What tutorials are not

- **Reference docs.** If you want the exact set of options on `[trigger]`,
  read [`docs/guide/rules.md`](../guide/rules.md) and the per-connector
  reference under [`docs/reference/connectors/`](../reference/connectors/).
- **Architecture explanations.** If you want to know *why* the
  cooperation tick is 25 steps, read [`docs/guide/cooperation.md`](../guide/cooperation.md)
  or [`docs/intended-arch/COOPERATION.md`](../intended-arch/COOPERATION.md).
- **Production deployment guides.** Tutorials run locally on your
  laptop. For server deployment see [`docs/operations/`](../operations/)
  and [`docs/installation/`](../installation/).

## Common shapes

The tutorials build toward three common bot shapes that show up
across real Springtale deployments:

```
┌──────────────────────────────────────────────────────────────┐
│ Single-agent reactor                                         │
│                                                              │
│   trigger ──► condition ──► action                           │
│                                                              │
│   "A happens, react with B." No coordination.                │
│   Tutorial 01 (moderation), tutorial 02 (alerts).            │
└──────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────┐
│ Multi-connector fan-out                                      │
│                                                              │
│           ┌─► action on connector A                          │
│   trigger ┼─► action on connector B                          │
│           └─► action on connector C                          │
│                                                              │
│   "A happens, react in N places at once." Still no           │
│   coordination — the actions don't talk to each other.       │
│   Tutorial 02 (one trigger → 3 outputs).                     │
└──────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────┐
│ Cooperating formation                                        │
│                                                              │
│           ┌─► agent A (researcher)  ──┐                      │
│   intent ─┼─► agent B (writer)       ─┼─► blackboard         │
│           └─► agent C (critic)       ─┘   shared output      │
│                                                              │
│   "These N agents pursue a shared intent, share an           │
│   environment, and rally if one struggles." Tutorial 03.     │
└──────────────────────────────────────────────────────────────┘
```

Most production work is shape 1 or 2. Shape 3 is what differentiates
Springtale from "just write a script".

## After the tutorials

- [`docs/cookbook/`](../cookbook/) — recipes for common patterns
- [`docs/guide/`](../guide/) — concept docs for cooperation, security, rules, connectors
- [`docs/reference/`](../reference/) — exact API / CLI / config surface
- Real worked code: `apps/springtale-cli/examples/{task-runner,llm-swarm,telegram-bot}.rs`
