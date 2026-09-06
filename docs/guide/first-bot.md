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

## Start the project

`springtale init` creates the data directory, the vault and the
database — that is the whole of the setup. Everything after it comes
from a recipe: browse the library in the colony UI, or apply one from
the command line. See [`recipes.md`](recipes.md) for the catalogue.

```bash
springtale init
```


This writes a timestamped project directory under the data dir's
`projects/` (XDG default: `~/.local/share/springtale/projects/`).
Springtale picks the path itself — there's nothing to edit. The CLI
prints the full path when it finishes.

## Drop your tokens in the vault

The starter TOML contains `YOUR_TOKEN` placeholders on purpose. Don't
edit the file. Put the real values in the vault through the connector
setup flow: open the connector's card in the dashboard (or call the
connector setup API) and paste the token there. It never goes through
argv or env.

Every connector the recipe uses lists the vault keys it needs in a
comment at the top of `springtale.toml`. The vault keeps tokens at rest
with authenticated encryption; you never re-type them after this.

## Run

```bash
springtale server start
```

The daemon boots in ≤5s on a laptop. `springtale trace` streams what
it's doing. The colony canvas at `http://127.0.0.1:8080/dashboard`
shows every bot, every tick, live.

## What to do next

- Send your bot a message. `/start` triggers the welcome rule.
- Add a second rule: `springtale rule add my-rule.toml`.
- Add a second connector by deploying another recipe (the rules
  engine merges them).
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
