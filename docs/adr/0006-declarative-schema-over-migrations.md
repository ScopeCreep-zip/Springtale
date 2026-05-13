# ADR 0006: Declarative schema with `PRAGMA user_version`, not incremental migrations

**Status:** Accepted
**Date:** 2026-05-01

## Context

Through April 2026 the store used 11 incremental migrations
(`migrations/001_init.sql` through `migrations/011_cooperation.sql`)
plus a `migrations/mod.rs` runner. Pattern: each migration is the
diff from the previous one; the runner applies them in order based on
`PRAGMA user_version`.

The pattern has been the SQLite + Rust default for a decade. After 11
migrations of cumulative drift we hit several problems:

1. **No single source of truth for the schema.** To know what the
   `formations` table looked like *today*, you read 11 files and
   composed them mentally (or wrote a doc that immediately drifted).
2. **Migration order coupling.** Migration 8 referenced a table
   created in migration 5 with a column added in migration 7. PR
   reviewers had to hold all of that in their head.
3. **No way to test "fresh install" cleanly.** A new contributor's
   database ran through 11 migrations on first boot. A long-running
   user's database had run through them at different points in
   different orders (we'd reordered a couple early on).
4. **Cooperation's schema specifically.** Migration 011 added 30+
   tables for the cooperation framework in one giant file. Reading
   it as a diff was useless; the cooperation schema was effectively
   a fresh subsystem, not an incremental change.

## Decision

Replace incremental migrations with a **declarative schema**.

- `crates/springtale-store/src/schema/sql/` holds one `.sql` file per
  domain (12 files: audit, bot, connectors, cooperation, events,
  execution, formations, jobs, rules, runtime_config, safety, wasm).
- `crates/springtale-store/src/schema/apply.rs` brings any database
  to `SCHEMA_VERSION` (currently 1) in one pass:
  1. Read `PRAGMA user_version`.
  2. If it equals `SCHEMA_VERSION`, proceed.
  3. If it's 0 (fresh database), apply every `.sql` file in order.
  4. If it's lower than `SCHEMA_VERSION` but >0, run any registered
     migration steps from that version to current.
  5. If it's higher than `SCHEMA_VERSION`, refuse to start (downgrade
     attempt).
  6. Write `PRAGMA user_version = SCHEMA_VERSION` on success.

Future schema changes happen by editing the relevant `.sql` file
plus registering a migration step that brings older databases to the
new shape. The declarative `.sql` is the source of truth; the
migration step is the bridge.

## Consequences

Positive:

- "What does the schema look like" is answerable by reading 12 files,
  each ~50 lines. No mental composition.
- Reviewing a schema change PR shows you the actual change: a diff
  against `connectors.sql` is the column being added, not "the
  combined effect of migrations 5, 7, and 11".
- Fresh installs run the same code path long-running databases do
  (after bringing the old database forward).
- Cooperation's 30 tables live in one file (`cooperation.sql`) that
  reads as a self-contained subsystem schema.
- Schema-version mismatch is now a hard error rather than an attempt
  to re-apply. Catches downgrade attempts.

Negative:

- We can't trivially recreate the historical state of the database
  at, say, the v0.0.4 release. The `.sql` is "current"; the historical
  shape lived in git history of `migrations/`. Mitigated: `git log
  -- crates/springtale-store/src/migrations/` still shows the
  pre-refactor migrations through 011.
- "Add a NOT NULL column with a default" is a two-step move: edit
  the `.sql` to add the column, register a migration step that does
  `ALTER TABLE … ADD COLUMN … DEFAULT …`. Slightly more ceremony
  than appending one new migration file.

Locks in:

- Schema-version semantics: bumps are deliberate, not automatic. A
  new column with a default doesn't bump the version; a column rename
  or table restructure does.
- Forward-only migrations. Reverting requires restoring a backup, as
  noted in `docs/operations/upgrade.md`.

## Alternatives considered

### Option A — Declarative schema + apply.rs (picked)

Pros and cons enumerated above.

### Option B — Keep incremental migrations

Pros: well-trod path. Lots of Rust crates do this.
Cons: every problem listed in Context.

Why we didn't pick it: we were drowning in the very problems this
pattern claims to prevent.

### Option C — A schema-management tool (sqlfluff + a migration runner)

Pros: external tool with its own ecosystem.
Cons: more dependencies, build complexity, contributor onboarding
cost. Most of the value of such tools is in a multi-developer Postgres
shop, which Springtale isn't.

Why we didn't pick it: too much tooling for too little payoff.

### Option D — Reset migrations periodically (squash on each major release)

Pros: bounded migration count.
Cons: still has the "no single source of truth between resets" problem.
Long-running users have to be supported across releases. We'd be
adding a release-cadence concern we don't otherwise need.

Why we didn't pick it: we'd be reinventing declarative schema in
slow motion.

## References

- `crates/springtale-store/src/schema/sql/` — current schema
- `crates/springtale-store/src/schema/apply.rs` — applier
- `crates/springtale-store/src/schema/tests.rs` — idempotency tests
- `docs/operations/upgrade.md` — user-facing upgrade flow
- Related: ADR 0001 (rusqlite choice) — the schema-apply machinery is
  rusqlite-flavoured
- Related: ADR 0005 (cooperation extraction) — the cooperation schema
  arrived as one chunk
