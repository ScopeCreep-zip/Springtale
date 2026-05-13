# Upgrading between versions

Springtale is pre-1.0. We haven't promised semver yet. **Read the
[CHANGELOG](../../CHANGELOG.md) before every upgrade.**

## What can change between versions

| Layer | Change risk |
|---|---|
| Wire format (`/api/*` JSON shapes) | Will break until 1.0 |
| Database schema | Declarative — see below |
| CLI flags | Stable-ish but will gain new subcommands |
| Connector manifests | Stable since 0.1 |
| Vault file layout | Will break only with explicit migration; bumped only when crypto changes |
| WASM connector ABI | Stable since the WIT was drafted |

## The schema-apply model

Up until April 2026, Springtale used incremental migrations
(`001_init.sql` … `011_cooperation.sql`). After the G-series refactor,
it uses a **declarative schema** in `crates/springtale-store/src/schema/sql/`
plus a single `apply.rs` that brings any database forward to
`SCHEMA_VERSION`.

```
SCHEMA_VERSION = 1    ← current
SCHEMA_VERSION = 2    ← next breaking schema change
```

When the daemon boots:

1. Open the database.
2. Read `PRAGMA user_version`.
3. If it equals `SCHEMA_VERSION`, proceed.
4. If it's lower, look for migration steps from that version to current.
5. If it's higher, refuse to start (you downgraded; data may use shapes
   the older code doesn't understand).
6. If migration succeeds, write the new `user_version`.

This means upgrades are usually invisible to you — start the new
daemon, schema is applied, you're running.

**When migrations exist for the version you're crossing**, the daemon
logs each step. The migrations themselves are forward-only — there's
no "downgrade" path. If you need to revert, restore from a backup
taken before the upgrade.

## The upgrade procedure

For a routine upgrade:

```bash
# 1. Back up first. Always.
springtale-cli travel prepare --backup-to /backups/pre-upgrade-$(date +%Y%m%d).tar.gz.enc

# 2. Stop the daemon cleanly.
systemctl stop springtaled         # or your equivalent

# 3. Pull and rebuild.
cd /path/to/Springtale
git pull
cargo build --release --workspace

# 4. Read the changelog.
less CHANGELOG.md

# 5. Install the new binary.
sudo install -m 755 target/release/{springtaled,springtale-cli} /usr/local/bin/

# 6. Restart.
systemctl start springtaled

# 7. Verify.
springtale-cli doctor
journalctl -u springtaled --since "2 minutes ago"
```

If something looks off in step 7, restore the backup from step 1.

## Schema-breaking releases

When `SCHEMA_VERSION` bumps, the changelog will say:

```
### Changed

- **Schema version bumped to 2.** The migration runs automatically on
  first boot. No action required. Takes <5 seconds on a 100 MB database.
  Backup before upgrading.
```

If a release requires manual migration (data shape change that can't
be inferred, e.g. you need to pick a value for a new NOT NULL column),
the changelog will say:

```
### Changed

- **Schema version bumped to 2 — MANUAL ACTION REQUIRED.** Run
  `springtale-cli migrate --to 2` after upgrading the binary but
  before starting the daemon. See docs/operations/upgrade.md §schema-2.
```

We will try very hard not to ship those. When we do, they get their
own section in this file documenting the steps.

## Connector compatibility

Connectors carry their own version field in their manifest. The
runtime checks compatibility at load time:

- Manifest `connector_abi_version` ≤ runtime's supported version → loads.
- Higher → won't load. Need to upgrade the daemon first.

A connector built against an older daemon should keep working unless
its specific dependency surface changed. Check
[CHANGELOG](../../CHANGELOG.md) for `**Connector ABI**` markers.

## Rollback

There is no automated downgrade. If a new version breaks something:

1. Stop the daemon.
2. Install the old binary (build from a previous git tag).
3. Restore the pre-upgrade backup. `travel restore --from …`. This
   replaces the data directory with the backed-up version.
4. Start the old daemon.

You'll lose whatever the daemon wrote between the backup and the bad
upgrade. There's no merge path — durable journalling for that level of
recovery is out of scope.

## Skipping versions

You can upgrade across multiple versions in one step. The schema-apply
logic runs every needed migration in order. So upgrading from a
hypothetical schema v1 → v3 runs the v1→v2 and v2→v3 migrations back
to back.

We don't test every multi-version skip. For long jumps (e.g. >6
months between upgrades), the safer path is:

```
v1 → v2 → cargo test --workspace → v3 → cargo test --workspace → ...
```

## Connector binary cache

Native connectors are baked into the daemon binary. WASM connectors
under `connectors/*.wasm` are content-addressed by hash in the
`wasm_binaries` table. After a daemon upgrade, hashes may not match if
the WASM ABI bumped; the daemon will re-fetch from the manifest URL on
next load. Plan for transient connector reload during major upgrades.

## Detecting "upgrade nag"

You can ask the daemon what version it's running:

```bash
springtale-cli --version
curl -s http://127.0.0.1:8080/health | jq .version
```

Comparing to the latest release on GitHub is up to you. The daemon
makes **zero outbound connections at idle** by design — there is no
auto-update check, no version-phone-home. You decide when to upgrade.
