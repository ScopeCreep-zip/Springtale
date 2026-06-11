# Installing with Docker / Compose

The repo ships a `Dockerfile` and `docker-compose.yml` at the root. The
image is a multi-stage build that produces a static `springtaled` on top
of `gcr.io/distroless/cc-debian12` — no shell, no package manager, no
attack surface beyond what the daemon needs to run.

## Quick start

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale
docker compose up -d
```

That brings up the daemon listening on `127.0.0.1:8080` and writes
vault + database files to `./.data/springtale/` on the host (bind-mounted).

To stop:

```bash
docker compose down
```

## What `docker-compose.yml` does

```
┌──────────────────────────────────────────────────────────────┐
│ springtaled container                                        │
│ - distroless/cc base, no shell                               │
│ - runs as uid 1000 (non-root)                                │
│ - capabilities: NONE (no --cap-add)                          │
│ - read-only rootfs, /data mounted writable                   │
│ - 127.0.0.1:8080 published on host                           │
│ - SPRINGTALE_PASSPHRASE_FILE=/run/secrets/springtale_passphrase │
└──────────────────────────────────────────────────────────────┘
              │
              ▼
         ./data              ←  bind-mounted at /data
         ./secrets           ←  Docker secrets directory
```

## Passphrase handling

The vault passphrase is acquired at boot via a 3-way fallback in priority
order:

1. **`SPRINGTALE_PASSPHRASE_FILE`** — path to a file containing the
   passphrase. This is the **Docker secrets** pattern and the
   recommended production option.
2. **`SPRINGTALE_PASSPHRASE`** — literal passphrase in the environment.
   **Dev only** — visible in `docker inspect`, process listings, shell
   history. Don't ship it.
3. **Interactive prompt** — if stdin is a TTY, prompt the user. Not
   useful in a daemon container.

The compose file uses option 1 by default. Create
`secrets/passphrase.txt` with your vault passphrase before `docker compose up`
(the compose file maps it to `/run/secrets/springtale_passphrase`):

```bash
mkdir -p secrets && chmod 700 secrets
printf '%s' 'your-long-strong-passphrase' > secrets/passphrase.txt
chmod 400 secrets/passphrase.txt
```

## Connector tokens

Connector credentials (Telegram bot tokens, GitHub PATs, Bluesky app
passwords, etc.) are parsed into `Secret<String>` at config load and
never logged. Two ways to provide them:

1. **`springtale.toml`** — the compose file bind-mounts
   `./springtale.toml` read-only at `/etc/springtale/springtale.toml`.
   Put the connector section there (e.g. `[telegram] bot_token = "..."`)
   and `chmod 600` the file on the host.
2. **Management API / dashboard** — upsert connector config over
   `POST /connectors/setup` (or the dashboard UI) after the daemon is
   up. Secrets land in the encrypted store, not in a file.

The container image ships the CLI as `/usr/local/bin/springtale` if you
need to exec in (note: distroless — there is no shell, so use
`docker compose exec springtaled springtale <subcommand>` directly).

## Webhooks

If you're using webhook-driven connectors (GitHub, Telegram-webhook,
Kick, etc.), the daemon needs to be reachable from the outside. The
default compose file binds `127.0.0.1:8080` for safety — webhooks won't
reach you. Options:

- **Reverse proxy** — put nginx, Caddy, or traefik in front. The proxy
  terminates TLS, validates the webhook signature header is present
  (Springtale will validate the signature itself), and forwards to
  `127.0.0.1:8080`. **Recommended.**
- **`HOST_BIND=0.0.0.0`** — change the published port to `0.0.0.0:8080`.
  Only do this if your firewall does the job a reverse proxy should be
  doing. **Read [`docs/guide/security.md`](../guide/security.md) §1
  before doing this.**
- **Tailscale / WireGuard** — keep the bind on `127.0.0.1` and only
  expose it over a private mesh. Cleanest option if you already have a
  mesh.

## Operations

### Logs

```bash
docker compose logs -f springtaled
```

Springtaled writes structured JSON logs to stdout. Pipe through `jq` for
readability. See [`docs/operations/log-rotation.md`](../operations/log-rotation.md).

### Backup

The vault + database live in `./.data/springtale/`. Backing up that
directory while the daemon is stopped is a complete backup. Backing it
up while running can produce a torn vault file — use the explicit
backup flow:

```bash
docker compose exec springtaled springtale-cli travel prepare --backup-to /data/backup.tar.gz.enc
```

See [`docs/operations/backup-and-restore.md`](../operations/backup-and-restore.md).

### Update

```bash
git pull
docker compose build --no-cache
docker compose up -d
```

Schema migrations are declarative (`crates/springtale-store/src/schema/sql/`)
and applied automatically at boot. If the schema version mismatches an
expected version, the daemon refuses to start rather than silently
re-applying — that's an explicit upgrade-path concern, not a transient
error. See [`docs/operations/upgrade.md`](../operations/upgrade.md).

## What the image does NOT include

- A shell. You can't `docker exec sh`. Use `docker exec springtaled springtale-cli …` instead.
- Compilers or build tools. The image is a runtime image; build on a separate machine or in CI.
- Telemetry. There is none. The container makes zero outbound connections at idle.

## Threat model notes

- The container runs as a non-root user. Capabilities are dropped to `NONE`. The rootfs is read-only except `/data`.
- The daemon writes the vault file with `0o600` permissions inside the container; the bind mount preserves that on the host.
- `docker inspect` will show the bind-mount path. If your threat model includes someone with shell access to the host, that path will reveal "this person uses Springtale" even if the data is encrypted at rest. Use `./.data` rather than a path that names you (e.g. `/home/yourname/springtale-data`).
