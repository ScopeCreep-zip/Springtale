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
  `crates/springtale-runtime/src/operations/recipes/` +
  `tauri/packages/ui/src/colony/Recipe*.tsx`). Curated automation
  cookbook surface — browse / favorite / fork / preflight / apply /
  preview / render. 16 HTTP endpoints under `/recipes/*`. UI overlays:
  RecipeLibraryOverlay, RecipeCard, RecipeQuickView, RecipeDeployPanel,
  RecipeAuthorPanel. TOML import/export for portable recipes.
  **Status:** built-in recipes ship in the binary; user-recipe
  storage table (W2.B) is wire-shaped but not yet persisted;
  community recipes (W3.A) wire-shape only. See
  [`docs/guide/recipes.md`](docs/guide/recipes.md).

- **External workspaces (D1)** (`crates/springtale-cooperation/src/mental_model/external_workspaces.rs`
  + `crates/springtale-runtime/src/operations/workspaces/` +
  `crates/springtale-connector/src/{mention,workspace_key}.rs` +
  `tauri/packages/ui/src/colony/WorkspaceTargetPicker.tsx`).
  Per-formation directory of discovered chat destinations
  (Telegram chats, Discord channels, Signal groups, etc.).
  Populated automatically by the universal mention harvester
  running on every dispatched event; each connector implements
  `MentionExtractor`. URI-shaped `WorkspaceKey` (e.g.
  `telegram://chat/12345`). Privacy-by-default: stores
  display name + kind + counts, never message bodies or
  member rosters. Gossip-replicated within a formation via
  chitchat. New schema: `mental_model_workspaces.sql`. See
  [`docs/guide/external-workspaces.md`](docs/guide/external-workspaces.md).

- **Executions log + drift detection (Phase B)**
  (`crates/springtale-runtime/src/operations/executions/` +
  `crates/springtale-cooperation/src/execution.rs` +
  `tauri/apps/desktop/src-tauri/src/commands/{executions,drift}.rs` +
  `tauri/packages/ui/src/colony/{ExecutionsPanel,DriftBadge}.tsx`).
  Per-chain-fire observability with cooperation envelope. ULID
  `ExecutionId` for index-friendly sorting. `executions` +
  `execution_steps` tables (separate from legacy
  `execution_results`). Privacy posture stricter than Apify/n8n
  — sizes only, no payload content, error categorisations are
  enum tags. Default 14-day retention. Drift detector classifies
  latency / success / refusal-rate trends per recipe and per rule
  (`DriftClass: Stable | Improving | Degrading | Volatile`).
  See [`docs/guide/executions-and-drift.md`](docs/guide/executions-and-drift.md).

- **Dedupe + Extract actions (Phase A)**
  (`crates/springtale-core/src/rule/action.rs` +
  `crates/springtale-store/src/schema/sql/dedupe.sql`). Two new
  `Action` variants make polling recipes practical.
  `Action::Extract` parses bytes (Readability / Css / JsonPath /
  Feed / Ical / LlmSchema / Passthrough). `Action::Dedupe`
  short-circuits the chain on seen keys. Plaintext keys never
  persisted — blake3 hex digest only. Formation-scoped
  (`formation_id NULL` = global; `NOT NULL` = per-formation
  instance). LRU prune at `history` entries (default 10,000) per
  bucket. See
  [`docs/guide/dedupe-and-extract.md`](docs/guide/dedupe-and-extract.md).

- **Recipe authoring tools (W1.D, W2.C)**
  - **Preflight (W1.D)** (`crates/springtale-runtime/src/operations/preflight/`).
    Live validation as the user fills the deploy form. Per-check
    statuses (Blocking / Warning / Verified / Pending). Backend
    owns the `deployable` decision; frontend renders.
  - **Preview / dry-run (W2.C)** (`crates/springtale-runtime/src/operations/preview.rs`).
    Throwaway `RuleEngine`, synthetic trigger, returns the chain's
    plain-language steps. No side effects.
  - **Test This Step (W2.C / Phase C)** (`crates/springtale-runtime/src/operations/test_step.rs`).
    Fires chain in `ExecutionMode::DryRun` up to a single step.
    Read arms run for real; side-effecting arms stubbed.
  - **Selector picker** (`tauri/apps/desktop/src-tauri/src/commands/selector_picker.rs`).
    Tauri webview overlay with `picker.js` for picking CSS
    selectors from a target URL during recipe authoring.
  
  See [`docs/guide/recipe-authoring-tools.md`](docs/guide/recipe-authoring-tools.md).

- **Sentinel action-impact classification** (`crates/springtale-sentinel/src/impact.rs`).
  `ActionImpact { ReadOnly, Reversible, Destructive }` — drives
  the destructive-action approval gate's classification step.

- **`ChannelApprovalGate`** (`crates/springtale-sentinel/src/approval.rs`).
  Alternative to `DefaultDenyApprovalGate` for surfaces that have
  a UI: sends `PendingApproval` over mpsc to a UI subscriber,
  honours a timeout. Tauri's `commands/approval.rs` is the UI
  dispatcher; the W1.F flow surfaces approval requests as
  Tauri events the frontend resolves via `respond_to_approval`.
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
