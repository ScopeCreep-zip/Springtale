# Cookbook

Short, focused recipes for common Springtale patterns. Each is
self-contained — paste the TOML, replace the placeholders, you're
running.

These are not tutorials. Tutorials walk you through learning a shape.
Recipes assume you already know the shape and just want the working
example. If you don't know the shape, start with
[`docs/tutorials/`](../tutorials/).

## Recipes

| Recipe | What it solves |
|---|---|
| [Scheduled tasks](scheduled-tasks.md) | Run something on a cron; common patterns including idempotency and skip-on-recent-success |
| [Event fan-out](event-fan-out.md) | One trigger → N actions; iterate over a list, sequence vs parallel |
| [LLM with tools](llm-with-tools.md) | An LLM action that can invoke connectors mid-conversation |
| [Cooperative rate limiting](cooperative-rate-limiting.md) | Two bots that need to share a quota (e.g. shared API rate limit) |
| [Manifest signing](manifest-signing.md) | Sign your own community connector + verify others' |

## What this is and isn't

Each recipe is the **smallest working example** of the pattern. We
strip everything that isn't load-bearing for the recipe — error
handling, retries, audit considerations are mentioned but not
exhaustively shown.

If you need a production-grade implementation of any of these:

1. Start from the recipe.
2. Layer in error handling per [`docs/guide/rules.md`](../guide/rules.md).
3. Add capability assertions per [`docs/guide/security.md`](../guide/security.md).
4. Add the sentinel approval gate per
   [`docs/adr/0010-default-deny-approval-gate.md`](../adr/0010-default-deny-approval-gate.md).
5. Test against synthetic triggers (`springtale-cli rule run …`)
   before pointing at real traffic.

## Conventions

- TOML examples are valid against the current schema. If a key is
  marked `# REPLACE ME`, that's a placeholder you have to fill in.
- File paths in examples assume your project root is
  `~/.springtale/projects/example-bot/`. Adjust to your real path.
- `${trigger.*}` interpolation uses the rule engine's variable
  syntax. See [`docs/guide/rules.md`](../guide/rules.md) §5.
- Every recipe ends with a **gotchas** list — the things you'd
  trip over running it in real life.
