# Changelog

All notable changes to Springtale are listed here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project
will follow [SemVer](https://semver.org) once it cuts a 1.0.

The architecture-level changelog (audit drift between `intended-arch`
and `current-arch`) lives at [`docs/current-arch/CHANGELOG.md`](docs/current-arch/CHANGELOG.md).
This file is the **user-facing** changelog.

## [Unreleased]

### Cooperation gap-closure pass (2026-06-10)

- **Consensus loop closed end-to-end (§11).** Votes now carry a typed
  `DecisionSubject` and resolutions are *applied*, not just logged: an
  approved destructive action mints a one-shot execution permit, a deny or
  timeout removes the task (timeout is ALWAYS a denial for destructive
  subjects — no-quorum silence never executes anything). A pending-vote
  guard stops the same task re-opening a vote every tick.
- **Formation self-governance wired (§5.5).** At Fever tier a formation can
  vote to change its own intent (`ProposeIntentChange` + `CastVote`
  commands, `POST /formations/{id}/propose-intent` and
  `/formations/{id}/votes/{vote_id}` API routes). Anchored in Joint
  Intention Theory: the joint goal changes by mutual belief, i.e. a vote.
- **All intent writes flow through one chokepoint**
  (`orchestrator::intent::apply_intent`): user command, intervention,
  colony commander, and consensus resolution — each resets the §7 momentum
  run and rebroadcasts the formation context. The cadence bus is now a
  pure metronome (`Tick` no longer carries an intent).
- **Pacing completed (§22).** L4D Director constants implemented
  (30s intensity decay, 0.99 relax threshold, 3–5s sustain-peak); pacing
  clocks now run on true wall-time (previously 4× fast); per-formation
  tick rates modulate via the pacing divider (Peak 30 Hz → Recovery 5 Hz).
- **Commit `Countdown` phase live (§12)** via
  `CommitBarrier::with_countdown` — Ready → Countdown → Execute with
  observable transitions.
- **Rally falloff implemented (§A.4.2)** — WH3's 70/×1.5 aura mapped onto
  snapshot Age-of-Information: stale gossip influences morale less,
  fading to zero at 3s.
- **`TickId` newtype** replaces raw `u64` tick sequences across the
  workspace (deterministic-simulation practice; the sweep caught a real
  tick-as-payload-hash bug in the task-runner example).
- **Legacy orchestrator deleted** — `recursive.rs`/`subagent.rs` had zero
  callers; formations are the only coordination model.

### Security (CI / supply-chain hardening pass — 2026-05-13)

- **Memory-safety roadmap published** (`docs/security/MEMORY-SAFETY.md`) per
  CISA's January 2026 expectation. Full `unsafe` inventory, C/C++ dep list,
  pyo3 trust-boundary disclosure.
- **Crypto inventory + PQ migration plan** (`docs/security/CRYPTO-INVENTORY.md`)
  per NIST IR 8547. Ed25519 / X25519 marked for hybrid migration; deadlines
  2026 Q4 (TLS hybrid), 2027 (manifest signing), 2030 (mandatory).
- **Hybrid post-quantum TLS** — `rustls-post-quantum` 0.2 wired as the
  process-wide rustls crypto provider in `springtaled`, `springtale-cli`,
  and the Tauri desktop app. Negotiates X25519MLKEM768 with PQ-capable peers.
- **Supply-chain plan + audit graph** (`docs/security/SUPPLY-CHAIN.md`,
  `supply-chain/{audits,config,imports.lock}.toml`). `cargo-vet` configured
  to import Mozilla / Google / Bytecode Alliance audits.
- **VEX statements** for `connector-matrix` rusqlite CVE,
  `rsa` RUSTSEC-2023-0071, `atomic-polyfill` unmaintained, `rand 0.8.5`
  log-feature unsoundness — all `not_affected` with documented justifications.
- **Strict Tauri CSP** in `tauri.conf.json`: `default-src 'none'`,
  `require-trusted-types-for 'script'`, `withGlobalTauri: false`.
  Trusted Types default policy installed at frontend boot in both desktop
  and dashboard apps.
- **`.npmrc ignore-scripts=true`** at repo root and `tauri/` — neutralises
  npm lifecycle-script supply-chain attacks (CISA Sep-2025 Shai-Hulud
  advisory, Apr-2026 Axios advisory).
- **deny.toml tightening**: license allow-list expanded (MPL-2.0,
  CDLA-Permissive-2.0, CC0-1.0); banned `md-5`, `sha-1`, `rust-crypto`,
  and the telemetry crates (`sentry`, `opentelemetry-otlp`,
  `datadog-tracing`); `unknown-git = "deny"`; advisory ignores mirror
  `.cargo/audit.toml` with VEX-doc references.
- **OpenSSL stub crates** in `vendor/` so any transitive `openssl` or
  `openssl-sys` pull becomes a compile-time error (was previously only
  caught by `cargo deny`).
- **gitleaks rule expansion** for Anthropic / OpenAI / Slack / Discord /
  Telegram / npm / PyPI / crates.io / GitHub PAT / age secret patterns.
- **clippy.toml workspace lints** — `disallowed-methods` for
  `ExposeSecret::expose_secret`, raw `reqwest::Client::new`,
  `std::env::var` in library crates, raw `Command::new` outside the
  connector-shell shim.
- **`Dockerfile` rewritten** to multi-stage `cargo auditable build` →
  distroless `gcr.io/distroless/cc-debian12:nonroot`. OCI labels added.
- **`docker-compose.yml`** gains `pids_limit`, `mem_limit`, `cpus` caps;
  healthcheck switched to the new `springtale healthcheck` subcommand
  (distroless has no `wget`).
- **CI workflows split out** by domain: lint+test (`ci.yml`), supply-chain
  (`sca.yml` with cargo-audit/deny/vet/geiger, pnpm-audit, pip-audit,
  osv-scanner, daily KEV check, npm-lockfile poisoning scan), SAST
  (`sast.yml` with CodeQL, Semgrep, actionlint, zizmor, hadolint),
  secrets (`secrets.yml` with gitleaks + trufflehog), Scorecard, container
  (`container.yml` with Trivy/Grype/Syft), SBOM (`sbom.yml` CycloneDX +
  SPDX), SLSA provenance + cosign keyless (`provenance.yml`), nightly
  fuzz (`fuzz.yml` with 5 targets), DAST (`dast.yml` ZAP baseline), LLM
  red-team corpus (`llm-redteam.yml`), CODEOWNERS validation.
- **LLM red-team corpus** at `crates/springtale-ai/tests/redteam_corpus/`
  — 50 attack cases (prompt-injection variants, credential leaks for a
  dozen providers, PII, suspicious encoding, content-too-long) wired into
  the existing `Sanitizer`. Integration test runs in CI, fails closed on
  any case the sanitizer doesn't flag.
- **Security FAQ** (`docs/security/SECURITY-FAQ.md`) — Secure-by-Demand
  answers to the five CISA acquirer questions.
- **Incident runbook** (`docs/security/INCIDENT-RUNBOOK.md`) — playbooks
  for dep compromise, leaked publish credential, Action compromise,
  user-safety vulnerability, KEV match, prompt-injection bypass.
- **CI trust posture** (`docs/security/CI-TRUST.md`) — allowlisted Actions
  with provenance notes, OIDC trust relationships, runner egress policy.
- **Risk register** (`docs/security/RISK-REGISTER.md`) — 32-row STRIDE
  inventory keyed to crate boundaries.
- **RFC 9116 `security.txt`** at `tauri/apps/dashboard/public/.well-known/`.
- **Pre-commit hook framework** (`.pre-commit-config.yaml`) — gitleaks,
  cargo fmt, typos, actionlint, zizmor, hadolint, prettier.
- **`springtale healthcheck`** CLI subcommand — used by container
  `HEALTHCHECK` since the distroless image has no shell tools.

### Added

- **OpenCode connector** (`connectors/connector-opencode/`). Wraps a
  locally-running `opencode serve` daemon (default
  `http://127.0.0.1:4096`) so a bot can hand off agentic coding tasks
  ("fix this bug", "add tests") and get the agent's reply back. Actions:
  `run_task`, `continue_session` — both `read_only: false`, fronted by
  the chat-approval gate. Optional basic-auth password as
  `Secret<String>`, optional model/agent routing.
- **In-app chat surface** (`apps/springtaled/src/api/chat.rs` +
  `tauri/packages/ui/src/colony/{ChatPanel,ChatDock}.tsx`). Talk to your
  bot without any external chat platform: `POST /chat` injects a message
  via the synthetic `"in-app"` connector, `GET /chat/stream` pushes bot
  replies over SSE. Desktop gets the same surface via the `chat` Tauri
  command.
- **Conversational task setup** (`crates/springtale-bot/src/conversation/`).
  Deterministic plain-language intent → recipe deploy ("send me the
  weather in Tucson every morning") with ZERO AI in the base path
  (NoopAdapter parity): catalog projection, deterministic NLU, slot-filling
  dialogue persisted in the session, varied NLG, deploy port. AI, when
  configured, only augments ranking/extraction.
- **ShellExec blocking approval over the API**
  (`crates/springtale-runtime/src/approval/` +
  `apps/springtaled/src/api/approvals.rs`). `ShellExec` grants always
  park in `pending_approval` regardless of policy; dispatch awaits
  `GET /approvals` / `POST /approvals/{id}` (approve/deny, HMAC bearer
  auth) and falls back to **deny** on timeout (default 60s).
  `ChatApprovalGate` surfaces the same requests in chat. Decisions land
  in the sentinel audit trail; checkpointed tool loops resume after
  approval (`crates/springtale-bot/src/tool_runner/resume.rs`, new
  `approvals.sql` schema: `pending_approvals` + `tool_loop_checkpoints`).
- **AI guardrails** (`crates/springtale-ai/src/guardrail/`).
  `GuardrailAdapter<A>` wraps any `AiAdapter` with OWASP LLM Top-10
  middleware: wall-clock timeout fence, output size cap, refusal-rate
  metric, and a per-bot daily token quota behind the `TokenQuota` trait.
  `SqliteTokenQuota` (`crates/springtale-runtime/src/quota/`) persists
  counters across restarts in the new `ai_token_usage` table.
- **Colony commander — strategic AI layer**
  (`crates/springtale-bot/src/colony/` +
  `runtime/tick_steps/orchestrate_step.rs`). Third layer of the AI
  command hierarchy: reviews ALL formations every 30 cadence ticks and
  proposes per-formation intent moves. AI-optional — without an
  `ai:colony` adapter it runs a deterministic de-escalation policy;
  guarded formations are never auto-touched.
- **GitHub write actions** (`connectors/connector-github/src/actions/`):
  `create_branch`, `commit_file`, `create_pr` — enough for a bot to open
  a reviewable PR end-to-end. All `read_only: false`.
- **Workspace discovery actions** — `discover_destinations` on Slack,
  Signal, Nostr, and Bluesky feeds the D1 external-workspaces directory
  on demand (the 🔍 Scan affordance), complementing the passive mention
  harvester.
- **Shared trigger lifecycle** (`crates/springtale-runtime/src/triggers/`).
  `ConnectorEvent` subscription wiring (`activate_rule` /
  `deactivate_rule` / `TriggerRegistry`) shared by daemon and desktop so
  event-triggered rules attach/detach identically on both surfaces.
- **Audit chain verification** (`crates/springtale-sentinel/src/audit/verify.rs`
  + `audit_chain.sql`). Hash-chained audit rows with a verification pass
  that detects truncation or tampering.
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
