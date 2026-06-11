# Database management

Springtale uses one SQLite file
(`$SPRINGTALE_DATA_DIR/springtale.db`) encrypted at rest with
SQLite3MultipleCiphers (ChaCha20-Poly1305). This is everything the
daemon persists except the vault: rules, events, formation state,
mental models, audit trail, webhook secrets, runtime config.

## Layout

17 schema files in `crates/springtale-store/src/schema/sql/`, applied
declaratively at boot via `schema/apply.rs`. The split:

| File | Tables | Domain |
|---|---|---|
| `ai_token_usage.sql` | `ai_token_usage` | per-bot daily AI token counters (quota / observability) |
| `approvals.sql` | `pending_approvals`, `tool_loop_checkpoints` | approval-gate decisions + paused tool loops awaiting approval |
| `audit.sql` | `audit_trail` | every sentinel verdict (hash-chained rows) |
| `bot.sql` | `bot_sessions`, `user_prefs`, `bot_memory`, `bot_aliases` | session memory, prefs, aliases |
| `connectors.sql` | `connectors` | installed connectors + their config |
| `cooperation.sql` | `coop_writes`, `coop_deposits`, `mental_model_*` | cooperation state that crosses dissolves |
| `dedupe.sql` | `dedupe_seen` | `Action::Dedupe` seen-keys (blake3 digests only) |
| `events.sql` | `events` | trigger fires + dispatch outcomes |
| `execution.sql` | `execution_results` | legacy rule execution history |
| `executions.sql` | `executions`, `execution_steps` | per-chain-fire observability (Phase B; sizes only, no payloads) |
| `formations.sql` | `formations`, `formation_members`, `formation_momentum`, `formation_rally` | formation rosters + momentum + rally tokens |
| `jobs.sql` | `jobs` | scheduled work (in-memory mpsc for now) |
| `mental_model_workspaces.sql` | `mental_model_workspaces` | discovered external chat destinations (D1) |
| `rules.sql` | `rules` | rule definitions |
| `runtime_config.sql` | `config_store` | hot-swappable config (AI provider, etc.) |
| `safety.sql` | `safety_config` | disguise, quick-hide, panic-tap settings |
| `wasm.sql` | `wasm_binaries` | content-addressed WASM connector blobs |

The schema version (`PRAGMA user_version`) is currently `1`. If the
daemon boots against a database with a different `user_version`, it
refuses to start and reports `E004`. See [upgrade.md](upgrade.md).

## Inspecting the database

You can't open it with stock `sqlite3` — the encryption-at-rest layer
isn't in stock SQLite. Three options:

### Option 1: `springtale-cli` queries

The CLI exposes inspection commands that go through the encrypted layer
automatically:

```bash
springtale-cli rule list
springtale-cli connector list
springtale-cli events --limit 50
springtale-cli memory audit
```

For ad-hoc SQL, the daemon doesn't expose it directly (no eval endpoint
on purpose). Add a one-off subcommand if you really need it.

### Option 2: Build sqlite3mc and use it

The `libsqlite3-sys-mc` shim in `crates/` is the vendored SQLite3MultipleCiphers
build. With that linked:

```bash
sqlite3mc "$SPRINGTALE_DATA_DIR/springtale.db"
PRAGMA key='<your derived key>';
SELECT * FROM rules LIMIT 5;
```

Getting the derived key from the passphrase requires running the same
Argon2id KDF Springtale uses. Easier path:

### Option 3: Export to plaintext SQLite

```bash
springtale-cli data export --output dump.json
```

JSON, not SQL — but structured by table. Load into a regular SQLite
file if you want SQL access:

```bash
# requires writing a small import script. JSON-to-SQLite is a few
# lines of Python or jq + sqlite3. The export is stable, documented
# at docs/reference/data-format.md (when we ship that).
```

For a one-off, just `jq` the JSON directly.

## Growth and bounds

| Table | Growth rate | Bound |
|---|---|---|
| `events` | linear with trigger volume | none — manual purge |
| `audit_trail` | linear with action volume | `[sentinel] audit_retention_days` |
| `bot_memory` | linear with bot conversation length | `[bot] context_window` per session |
| `execution_results` | linear with rule fires | none — manual purge |
| `formation_momentum` | one row per formation per ~tick | grows fast for long-running formations |
| `mental_model_*` | one row per (formation, observation type, …) | growth model varies; bounded per formation |
| `connector_outputs` | bounded — rolling buffer of last 100 per connector | hard cap, oldest dropped |

The two tables that grow fastest in production are `events` and
`audit_trail`. Both have configurable retention. Set sensible bounds:

```toml
[sentinel]
audit_retention_days = 90
```

Events retention isn't yet exposed in config (tracked in
[AUDIT-NOTES.md §9](../arch/AUDIT-NOTES.md)). For now:

```bash
sqlite3mc-via-cli "DELETE FROM events WHERE created_at < datetime('now', '-30 days')"
```

## Vacuuming

SQLite reclaims space lazily. After large deletes, run:

```bash
springtale-cli data vacuum
```

Or, if you've stopped the daemon:

```bash
sqlite3mc "$SPRINGTALE_DATA_DIR/springtale.db" "VACUUM;"
```

Vacuum holds an exclusive lock for the duration — schedule it for low
activity. Typical reclamation rates after a 30-day events purge: 60–80%
of file size returned.

## WAL mode

The database runs in WAL (Write-Ahead Log) mode for two reasons:

1. **Readers don't block writers.** The dashboard can read formations
   while the bot writes momentum updates.
2. **Crash safety.** WAL is replayed on next open; partial writes are
   discarded.

WAL files (`springtale.db-wal`, `springtale.db-shm`) appear next to
the main `.db` while the daemon runs. After a clean shutdown they're
truncated/removed. After a crash they're replayed at next boot.

**Never delete the WAL file while the daemon is running or has
crashed without replaying.** You'll lose every write since the last
checkpoint.

## Encryption keys

The database encryption key is derived from your vault passphrase via
Argon2id (separate KDF from the AEAD vault key, distinct salt). The
derived key lives in process memory only — never on disk.

If you forget the passphrase, the database is unrecoverable. There is
no key escrow, no recovery flow, no support hotline. This is by design.

## Inspecting from the daemon

Some inspection paths are surfaced as API endpoints + CLI:

```bash
springtale-cli memory audit          # session memory summary
GET /sessions/{id}/memory            # via the API
GET /events?since=...&limit=...
GET /audit-trail?since=...&limit=...
```

See [`docs/reference/api.md`](../reference/api.md) for the full surface.

## Forensic extraction

For incident response — pulling data out without a running daemon:

1. Stop the daemon cleanly so WAL is checkpointed.
2. Copy the entire data directory to a forensic-clean machine.
3. Install Springtale on that machine with the **same version** as the
   source.
4. Set `SPRINGTALE_DATA_DIR` to the copied directory.
5. Run `springtale-cli data export --output dump.json` with the
   passphrase.
6. Analyse `dump.json`.

Don't try to read the database file directly with stock SQLite tools.
The encryption layer won't be there.
