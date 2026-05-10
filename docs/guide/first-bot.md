# Your First Bot in 60 Seconds

This guide takes you from install to a running bot answering messages.
No config file edits, no vault gymnastics, nothing that requires reading
reference docs first. If you follow this and it takes longer than 60
seconds, the bot is wrong — open an issue.

## Install

```bash
curl -sSL https://springtale.run/install.sh | sh
```

(Single static binary. No Python env, no Node modules, no runtime
dependencies.)

## Pick a template

`springtale new <template>` scaffolds a starter project you can run
right away. Pick one of:

| Template | What it does | Connector tokens needed |
|----------|--------------|-------------------------|
| `blank-bot` | Empty skeleton for experts | None |
| `cli-runner` | Headless task runner, no chat | None |
| `telegram-bot` | Telegram bot with `/start` welcome | Telegram bot token |
| `discord-bot` | Discord bot with `!start` welcome | Discord bot token |
| `matrix-bot` | Matrix/Element chatbot skeleton | Matrix access token |
| `cron-runner` | Scheduled task with heartbeat | None |
| `webhook-receiver` | HTTP webhook → formation fan-out | None |
| `file-watcher` | Filesystem event → formation | None |
| `llm-assistant` | AI-powered chat assistant | Ollama or API key |
| `llm-swarm` | 3-agent AI swarm on a prompt | Ollama or API key |
| `research-assistant` | Multi-source research LLM swarm | Ollama or API key |
| `code-review-swarm` | PR → 3-agent code review | GitHub PAT + AI key |
| `meeting-summarizer` | Audio / transcript → structured summary | Filesystem path + AI key |
| `github-monitor` | GitHub webhook → Telegram push | GitHub + Telegram tokens |

Run the one closest to what you want:

```bash
springtale new telegram-bot
```

This writes a project directory under `~/.springtale/projects/`.
Springtale picks the path itself — there's nothing to edit. The CLI
prints the full path when it finishes.

## Drop your tokens in the vault

The starter TOML contains `YOUR_TOKEN` placeholders on purpose. Don't
edit the file. Put the real values in the vault:

```bash
springtale vault set telegram.bot_token
# (paste token when prompted — stdin only, never argv, never env)
```

Every connector the template uses lists the vault keys it needs in a
comment at the top of `springtale.toml`. The vault keeps tokens at rest
with authenticated encryption; you never re-type them after this.

## Run

```bash
springtale server start
```

The daemon boots in ≤5s on a laptop. `springtale logs` streams what
it's doing. The colony canvas at `http://127.0.0.1:8080/dashboard`
shows every bot, every tick, live.

## What to do next

- Send your bot a message. `/start` triggers the welcome rule.
- Add a second rule: `springtale rule add my-rule.toml`.
- Add a second connector with another `springtale new <other-template>`
  in the same project directory (the rules engine merges them).
- Read [architecture.md](architecture.md) once the bot is real enough
  that you want to know what's happening underneath.

## If it breaks

Every error carries a stable ID: `E001`-`E009` for operational errors,
`COOP-NNNN` for cooperation-layer errors. Run:

```bash
springtale fix E001
springtale fix COOP-2003
```

Some errors have an automated fix the command runs for you. All of
them have a clear causes-and-suggestions writeup. See
[fixing-errors.md](fixing-errors.md) for the full error catalog.
