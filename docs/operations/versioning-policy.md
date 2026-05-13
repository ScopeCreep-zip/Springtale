# Versioning policy

Springtale is pre-1.0 software targeting people whose safety depends
on privacy. The version number is a promise about what kind of change
we're allowed to ship without surprising you.

## Where we are

- **Pre-1.0** — anything can change. We try not to break things, but
  we don't owe you that no breaks happen.
- **The CHANGELOG is the contract.** If we change something visible,
  it's in [`CHANGELOG.md`](../../CHANGELOG.md). If it's not in the
  changelog, you can argue it was an unannounced break and we'll fix
  it forward.

## What "1.0" will mean

When Springtale cuts 1.0 (no firm timeline; estimated post-Phase-2b),
that signals:

- The HTTP API surface is stable. Endpoints don't disappear or change
  shape without a deprecation cycle.
- The CLI surface is stable. Subcommands don't change meaning.
- The connector manifest format is stable. Existing signed manifests
  continue to load.
- The configuration TOML schema is stable. Field renames go through a
  deprecation cycle.
- The vault file format is stable across versions within the major.

Specifically, 1.0 does **not** promise:

- That the internal Rust crate APIs are stable. Library consumers
  should pin to specific versions.
- That the database schema is stable forever. Schema migrations
  continue between major versions.
- That `VeilidTransport` is anything other than a stub.

## Semver mapping (post-1.0)

| Bump | Means | Examples |
|---|---|---|
| Patch (`1.2.3 → 1.2.4`) | Bug fix, no behaviour change | Fix a sentinel verdict, tighten a clippy lint, doc fix |
| Minor (`1.2.x → 1.3.0`) | New feature, additive | New connector, new CLI subcommand, new API endpoint, new cooperation module |
| Major (`1.x.y → 2.0.0`) | Breaking change | Removed API endpoint, renamed CLI flag, vault file format change, connector ABI bump |

## Deprecation

When something is going away in a future major:

1. The replacement is added in a minor version.
2. The old surface gains a deprecation warning at the same time.
3. The old surface is removed in the next major.

Minimum **2 minor versions** of overlap between the deprecation warning
and the removal. That's the contract.

Example:

```
v1.3.0:  POST /rules/new added.  POST /rules/create still works,
         logs a deprecation warning.

v1.4.0:  Same.

v1.5.0:  Same.

v2.0.0:  POST /rules/create removed.  Only POST /rules/new exists.
```

If you read the CHANGELOG and act on deprecation warnings, you'll
never be surprised by a removal.

## Connector ABI

Native connectors are part of the same crate graph as the daemon — if
the daemon compiles, the connector works. No separate ABI concern.

**WASM connectors** are different. They link against the WIT world
defined in `crates/springtale-wit`. The WIT carries its own version
number, separate from the daemon's semver:

```
WIT v0.x — pre-stabilisation, can break with any release
WIT v1.x — stable.  Bumps follow the WASM Component Model conventions.
```

A daemon at v2.x might still support WIT v1.x. We will document
the support matrix in the CHANGELOG when it matters.

## Vault format

The vault file (`vault.bin`) currently lives at format version 1. The
header byte at offset 8 records the format version. If we change the
layout — e.g. moving to a new KDF, splitting into more regions, adding
a third decoy — we bump that byte and write a migration.

Format migrations happen at boot. The daemon detects an older format,
prompts for the passphrase, decrypts to memory, re-encrypts in the new
format, atomically swaps the file. **You will be prompted explicitly**;
this is never silent.

## Database schema

See [`upgrade.md`](upgrade.md). Schema versions are independent of the
semver — they bump when the SQL changes, regardless of which
release brings the change.

## API stability tiers

When 1.0 lands, every API endpoint will have a stability tag. Until
then, every endpoint is implicitly **experimental** and can change.

Future tags:

- **stable** — semver applies. Removal/break requires major bump.
- **experimental** — explicitly marked; can break in a minor.
- **internal** — for Tauri IPC and other in-process callers. Not for
  external consumers. Will not be in the public docs.

The `/health` and `/ready` endpoints will always be stable.

## What "we'll never break" means

Some things we commit to even pre-1.0:

- **No telemetry.** Ever, in any version. There is no path to add it.
- **No outbound connections at idle.** Doctor and `springtale-cli doctor`
  won't call home in a release where we promised not to.
- **`Secret<T>` discipline.** Credentials never appear in logs, error
  messages, panics, or `Debug` output.
- **`rustls` only.** No `native-tls`, no OpenSSL, ever.
- **The vault file format never silently changes.** Format-version
  byte at offset 8 is the contract.

These predate 1.0 and they survive past it.

## Reading the CHANGELOG

Every release maps to:

- **Added** — new features (minor/major).
- **Changed** — behaviour changes (minor/major; breaking changes major only post-1.0).
- **Deprecated** — slated for removal (minor; removal major post-1.0).
- **Removed** — actually removed (major post-1.0).
- **Fixed** — bug fixes (patch).
- **Security** — security fixes (any level). Always read these.

Pre-1.0, **Changed** can be a breaking change. Read it.
