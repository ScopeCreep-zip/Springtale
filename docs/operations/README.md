# Operations

Running Springtale long-term. These guides assume you have it
installed and a few bots in production — they're not about getting
started.

| Topic | When you need it |
|---|---|
| [Backup and restore](backup-and-restore.md) | Before anything bad happens, and again after |
| [Log rotation](log-rotation.md) | Disk filling up, journal entries getting cut off |
| [Database management](database.md) | SQLite is filling up, you need to inspect or archive |
| [Upgrade](upgrade.md) | Pulling a new version; schema-apply doesn't auto-migrate |
| [Host migration](host-migration.md) | Moving the daemon to a new machine without losing identity |
| [Versioning policy](versioning-policy.md) | Reading a CHANGELOG and deciding whether to upgrade |

If you're running this in production on someone else's behalf (e.g.
a mutual-aid org running a moderation bot fleet), the **threat-model
FAQ** [`docs/threat-model-faq.md`](../threat-model-faq.md) is worth a
read first.

## Where everything lives

```
~/.local/share/springtale/          (or your SPRINGTALE_DATA_DIR)
├── springtale.db          ← SQLite file (encrypted at rest via SQLite3MultipleCiphers)
├── springtale.db-wal      ← WAL file (transient, will be empty after clean shutdown)
├── springtale.db-shm      ← shared-memory file (transient)
├── vault.bin              ← AEAD-encrypted vault (constant 131,152 bytes; two regions)
├── api_token              ← HMAC bearer token for the management API
├── connectors/            ← installed connector binaries (WASM) + manifests
│   └── manifest_*.toml
└── audit.log              ← rotating audit log (also mirrored to audit_trail table)
```

Permissions on this directory are `0o700`. The daemon enforces this on
boot — if it finds the directory readable by anyone else, it refuses
to start. Don't `chmod 755` to "make it work".

## What "running well" looks like

- Memory usage stable around 30–150 MB depending on load.
- CPU near zero at idle. Bursty per cooperation tick.
- `springtale-cli doctor` returns 0 with no warnings.
- `audit_trail` table growth proportional to your action volume.
- Sentinel circuit-breaker rarely fires.
- Mental-model rows accumulating for long-running formations.

## What "running poorly" looks like

- Memory growing without bound — usually a formation that won't dissolve.
- CPU pegged — usually orchestrator AI calls in a loop (Fever-tier
  intervention without a fix).
- Audit log growing unbounded — set `[sentinel] audit_retention_days`.
- Repeated `E007` errors — connector that needs a manual recovery action.

## Production checklist

Before you point a real workload at a Springtale instance:

- [ ] Backup flow tested — you've restored a vault to a different machine and confirmed your formations resume.
- [ ] Logs going somewhere durable (journald, file with rotation, log aggregator).
- [ ] Vault passphrase stored in a real secret manager (not your shell history).
- [ ] API bound to `127.0.0.1` (or behind a reverse proxy / mesh).
- [ ] Sentinel approval gate wired if running headless and you've enabled any destructive actions.
- [ ] `cargo audit` clean on the version you're running.
- [ ] You've read [`docs/guide/security.md`](../guide/security.md) end to end at least once.

## Failure modes

The full error catalogue is at [`docs/guide/fixing-errors.md`](../guide/fixing-errors.md).
Every error has a stable ID — run `springtale-cli fix <ID>` for the
runbook.

The most common operational errors:

| ID | What it means |
|---|---|
| E001 | Vault not initialized; run `springtale-cli init` |
| E002 | Vault decryption failed; wrong passphrase |
| E003 | Database file permissions wrong (not 0o600) |
| E004 | Schema version mismatch; daemon refuses to start; see [upgrade.md](upgrade.md) |
| E007 | Connector failed health check; circuit breaker tripped |
| E008 | API token missing or invalid |
| E009 | Manifest signature verification failed; connector won't load |
| COOP-1xxx | Formation lifecycle errors |
| COOP-2xxx | Cooperation runtime errors (interference, rally exhausted) |
| COOP-3xxx | Mental-model persistence errors |
