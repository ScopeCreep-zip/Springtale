# Changelog

All notable changes to Springtale are listed here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project
will follow [SemVer](https://semver.org) once it cuts a 1.0.

The architecture-level changelog (audit drift between `intended-arch`
and `current-arch`) lives at [`docs/current-arch/CHANGELOG.md`](docs/current-arch/CHANGELOG.md).
This file is the **user-facing** changelog.

## [Unreleased]

### Added

- **Cross-formation gossip bus** (`crates/springtale-cooperation/src/gossip/`).
  `FormationView` snapshots broadcast between sibling formations every
  cooperation tick. UI fed by `tauri/apps/dashboard/src/api/cooperation.ts`
  and the `cooperation.rs` Tauri command.
- **Persistent cooperation memory** (`crates/springtale-cooperation/src/memory/`).
  Per-formation durable shared memory sitting between the ephemeral
  blackboard and the mental-model store.
- **Destructive-action approval gate** (`crates/springtale-sentinel/src/approval.rs`).
  Sentinel's fourth check after circuit-breaker / rate-limit / dead-man.
  Ships with `DefaultDenyApprovalGate` as the safe headless default.
- **Disguise tray icon** (G5f). Four icon profiles (`calculator`,
  `files`, `notes`, `springtale`) swappable at runtime via
  `POST /safety/disguise/profile`.
- **Quick-hide global hotkey** (G5g). OS-wide keyboard shortcut hides
  the window and locks the vault from anywhere on the desktop.
- **Connector hot-reload** (G4). `POST /connectors/{name}/reload` swaps
  a connector's running instance without daemon restart.
- **Cooperation SSE stream** (`apps/springtaled/src/api/cooperation_stream.rs`),
  served at `GET /cooperation/events`. Live formation lifecycle, momentum,
  rally, and interference events pushed to the dashboard without polling.
- **Recipes subsystem** (`apps/springtaled/src/api/recipes.rs` +
  `tauri/packages/ui/src/colony/Recipe*.tsx`). Curated automation
  cookbook surface — browse / favorite / fork / preflight / apply /
  preview / render. 16 HTTP endpoints under `/recipes/*`. UI overlays:
  RecipeLibraryOverlay, RecipeCard, RecipeQuickView, RecipeDeployPanel,
  RecipeAuthorPanel. TOML import/export for portable recipes.
- **Python bindings** (`crates/springtale-py`, G3). pyo3 facade for the
  cooperation model. Build with `maturin`.
- **WIT world** (`crates/springtale-wit`, G3) for WASM Component Model
  hosts.
- **Visual rule builder overlay** (`tauri/packages/ui/src/colony/RuleBuilderOverlay.tsx`).
  Basic version shipped; i18n and a11y still in progress.
- **`data import` CLI** — replay a previous `data export` JSON archive.
- **Seven new cooperation deep-dive guides** under `docs/guide/`:
  consensus, cross-formation, intervention, mental-model, pacing,
  sacrifice, troubleshooting-cooperation.

### Changed

- **Store schema** is now declarative (`crates/springtale-store/src/schema/sql/`
  + `schema/apply.rs`) instead of incremental migrations. Single
  `SCHEMA_VERSION` constant; mismatch is a hard error.
- **Cooperation crate** extracted from `springtale-bot` to its own
  crate (`crates/springtale-cooperation/`, 40+ modules, zero internal
  Springtale deps). 14-step formation tick now lives in `springtale-bot::runtime::event_loop::handle_cadence_tick`.
- **Bot runtime tick** decomposed into per-step modules under
  `crates/springtale-bot/src/runtime/tick_steps/`. Each step is
  independently unit-testable.
- **Connector tier system** (`crates/springtale-connector/src/tier.rs`)
  unifies how native and WASM connectors declare trust/sandbox tier.
- **AI streaming on OpenAI-compat adapter** now ships full SSE
  streaming (previously stub).

### Security

- **`foca` pinned** with explicit `default-features = false` + only the
  features we need (`std`, `tracing`, `bincode-codec`). Reduces transitive
  attack surface.
- **`ring` 0.17 added** to the workspace for the cooperation gossip
  signing path. Audited via `cargo audit` on every CI run.

### Notes

- `connector-matrix` remains deferred until `matrix-sdk` updates its
  pinned `rusqlite` past CVE-2025-70873.
- `VeilidTransport` remains a stub. Every method returns
  `TransportError::NotConnected`. Phase 3 work not started.

## [0.1.0] — initial scaffolding

Foundation. Single-binary daemon, CLI, rule engine, crypto vault, WASM
sandbox, seven baseline connectors (kick, presearch, bluesky, github,
filesystem, shell, http), MCP bridge. SQLite via SQLite3MultipleCiphers
with WAL mode.

---

[Unreleased]: https://github.com/ScopeCreep-zip/Springtale/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ScopeCreep-zip/Springtale/releases/tag/v0.1.0
