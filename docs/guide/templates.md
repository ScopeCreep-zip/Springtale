# Starting from a template

Springtale ships with 14 starter templates. `springtale new <name>` drops
a working scaffold into the current directory — no blank page, no TOML
spelunking.

```
                            springtale new
                                  │
  ┌─────────────────┬─────────────┼─────────────┬──────────────────┐
  │                 │             │             │                  │
  v                 v             v             v                  v
blank-bot     cli-runner     cron-runner   research-assistant   …etc
```

## The full menu

| Template | What it ships | When to pick it |
|---|---|---|
| `blank-bot` | empty skeleton | you want control from line 1 |
| `cli-runner` | headless task runner, no AI | smallest working bot |
| `llm-swarm` | 3-role researcher/writer/critic pipeline | OpenClaw replacement demo |
| `llm-assistant` | single-agent LLM bot | chat + rules, one persona |
| `telegram-bot` | Telegram DM bot | messaging-first workflow |
| `discord-bot` | Discord guild bot | community ops |
| `matrix-bot` | Matrix / Element bot | federated chat, Element app |
| `cron-runner` | scheduled triggers | heartbeats, nightly tasks |
| `webhook-receiver` | inbound HTTP → rules | incoming integrations |
| `file-watcher` | filesystem events → rules | dropbox-style workflows |
| `github-monitor` | push / PR events | CI/CD bridges |
| `research-assistant` | multi-source LLM research | cited writeups |
| `code-review-swarm` | git diff → 3-agent review | PR review automation |
| `meeting-summarizer` | transcript → structured summary | notes pipelines |

## From zero to running

```bash
springtale new cli-runner
# Creating cli-runner project in ~/.local/share/springtale/projects/cli-runner-20260610-091500
cd ~/.local/share/springtale/projects/cli-runner-*/
springtale init                # create vault + DB + config
springtale run                 # start the daemon
```

The CLI picks the destination itself — a timestamped directory under
the data dir's `projects/` — and prints the full path. You never pass
a path, which keeps the scaffold free of path-traversal surprises.

`springtale init` handles vault creation, passphrase setup, DB key
derivation, and any platform-specific onboarding (Telegram bot token,
Discord app credentials, etc.). The template's `springtale.toml` has the
right shape already — `init` fills in the encrypted pieces.

## What's in a template

Every template produces **at minimum** a `springtale.toml` at the project
root. Templates that ship a working trigger also drop one or more rule
files under `rules/` — they're plain TOML, editable by hand.

For example, `springtale new cron-runner` produces:

```
cron-runner-<timestamp>/
├── springtale.toml
└── rules/
    └── heartbeat.toml
```

Other templates add connector-specific stubs (see the table above for
what each ships). No template ships secrets — every credential goes
through the connector setup flow (the connector's card in the dashboard,
or the connector setup API) into the vault, never the TOML.

## AI adapters are optional

Templates that mention an LLM (`llm-swarm`, `research-assistant`,
`code-review-swarm`, `meeting-summarizer`, `llm-assistant`) ship with
**commented-out** provider blocks for Ollama / OpenAI / Anthropic.
Uncomment the one you want. If none are configured, `NoopAdapter` runs
— the bot still works, actions that need an LLM return canned text.
That's the product-model constraint: **every bot works without AI**
([product-model.md](../../.claude/rules/shared/product-model.md)).

## When no template fits

`springtale new blank-bot` gives you the empty skeleton — a
`springtale.toml` with the paths wired up, no rules, no connectors. Add
what you need by hand, or crib from a closer template and delete the
parts that don't apply.

## Next steps

- [first-bot.md](first-bot.md) — 60-second walk-through of `cli-runner`
- [rules.md](rules.md) — rule format in detail
- [cooperation.md](cooperation.md) — how formations cooperate inside a bot
- [connectors.md](connectors.md) — what a connector is + how to add one
