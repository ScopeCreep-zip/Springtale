# Springtale — Security Whitepaper
> **Status:** Active · **Updated:** 2026-03-28 · **Companion to:** [ARCHITECTURE.md](ARCHITECTURE.md)
> **Changes from v2.0:** Updated OpenClaw CVE list and adoption numbers,
> added NosytLabs competitive analysis, updated wasmtime advisory,
> added MCP supply chain breach context.

This document validates the security and privacy architecture defined in
`ARCHITECTURE.md`. It contains competitive analysis, compliance mappings,
CI pipeline specifications, and dependency supply chain audits. The
architectural constraints that affect implementation (per-component security
audits, `Secret<T>` usage, sandbox limits, autonomy levels) live in the
architecture doc alongside the components they constrain.

**Important: Design vs. Implementation.** Springtale is in active design
phase. No source code has been written yet. Every security control described
in this document is an architectural intent, not a verified implementation.
This whitepaper describes what the system WILL do when built correctly.
Implementation verification will be added as code ships — each phase will
be validated against the controls specified here before release.

---

## Table of Contents

1. [Competitive Analysis: OpenClaw](#1-competitive-analysis-openclaw)
2. [AppSec Tooling & CI Pipeline](#2-appsec-tooling--ci-pipeline)
3. [OWASP ASVS 5.0 Mapping](#3-owasp-asvs-50-mapping)
4. [MITRE ATT&CK Technique Mapping](#4-mitre-attck-technique-mapping)
5. [MITRE ATLAS Mapping](#5-mitre-atlas-mapping)
6. [Tanya Janca MVS Checklist](#6-tanya-janca-mvs-checklist)
7. [OWASP Agentic Top 10 Mapping](#7-owasp-agentic-top-10-mapping)
8. [Privacy by Design Principles Mapping](#8-privacy-by-design-principles-mapping)
9. [Dependency Supply Chain Audit](#9-dependency-supply-chain-audit)

---

## 1. Competitive Analysis: OpenClaw

OpenClaw (formerly Clawdbot/Moltbot) is the direct Phase 2 target. This
section documents its architecture and the specific weaknesses Springtale
exploits.

### 1.1 OpenClaw Architecture Summary

| Component | Technology | Risk |
|---|---|---|
| Runtime | Python (FastAPI) + SvelteKit frontend | Python supply chain, no memory safety |
| Skill system | Markdown files + shell scripts + Python functions | No sandboxing, full machine access |
| Skill distribution | ClawHub registry | Unvetted: 800+ malicious skills (~20% of registry). Cisco found credential exfiltration, reverse shells, infostealers (ClawHavoc campaign). |
| Chat channels | Baileys (WhatsApp), grammY (Telegram), discord.js | Centralized platform dependency |
| Memory | Markdown files + JSONL on disk | No encryption at rest, no typed queries |
| Security model | Allowlist of shell commands | No capability isolation, no manifest signing |
| AI integration | Claude Code / OpenAI / Ollama | AI is the foundation, not a plugin |
| Default binding | `0.0.0.0:18789` | 40K+ public instances. AWS Lightsail offers managed deployment. |

### 1.2 Springtale Advantages

| Concern | OpenClaw | Springtale |
|---|---|---|
| **Language safety** | Python (FastAPI) + SvelteKit (no memory safety) | Rust (memory safe, compile-time guarantees) |
| **Connector isolation** | None — skills run with full access | Wasmtime WASM sandbox, 10M fuel, 64MB memory cap |
| **Manifest integrity** | No signing | Ed25519 signed manifests, capability allow-list |
| **Supply chain** | PyPI + npm (frontend) | Rust crates with cargo-deny + cargo-audit |
| **Secret handling** | Env vars, plaintext config | `Secret<T>` types, zeroize on drop, vault encryption |
| **TLS** | Node.js default (OpenSSL) | rustls-tls exclusively, native-tls banned |
| **Storage** | Markdown files on disk | sqlx PgPool, compile-time verified SQL, encrypted payloads |
| **AI dependency** | Core to operation | Plugin with NoopAdapter default — works without AI |
| **Transport** | Centralized platforms only | Swappable: Local → HTTP → Veilid P2P |
| **Privacy** | Dependent on platform privacy | Phase 3: Veilid private routes, HKDF pseudonyms |
| **Bot model** | Monolithic agent with shell access | Pipeline stages, recursive sub-agents, capability-scoped |
| **Cost** | API keys required (~$30-150/mo) | Free with local models, API optional |

### 1.3 Feature Parity — No Reason to Stay on OpenClaw

Every core OpenClaw capability has a Springtale equivalent that is equal
or superior in security, privacy, and architectural quality.

| OpenClaw Feature | Springtale Equivalent | Phase | Notes |
|---|---|---|---|
| 22+ chat channels | 9 connectors (Telegram, Discord, Signal, WhatsApp, Matrix, IRC, Slack, Nostr, Browser) + community WASM connectors | 2 | Remaining niche channels (Feishu, LINE, Zalo, Synology, Tlon, WeChat) ship as community connectors via TypeScript SDK. iMessage via `connector-shell` with sandboxed `imsg` binary. Google Chat and MS Teams via `connector-http` webhook integration. |
| HEARTBEAT.md proactive wake | `springtale-scheduler` heartbeat module (typed rules, sandboxed checks) | 2 | Safer: each heartbeat check runs through connector sandbox with capability enforcement. OpenClaw's heartbeat gives AI full machine access. |
| Cron jobs | `springtale-scheduler` cron executor (cron expressions, tokio timer) | 1 | Already in Phase 1. Typed, not Markdown-driven. |
| Webhooks inbound | `springtaled` management API: `POST /webhook/{connector}/{trigger}` | 1 | Already in Phase 1. HMAC-SHA256 verified before processing. OpenClaw webhooks have no signature verification by default. |
| Browser automation | `connector-browser` (headless Chromium, domain allow-list) | 2 | Sandboxed: user approves every domain. OpenClaw's Playwright has unrestricted access. |
| Voice STT/TTS | `springtale-ai` voice modules (Whisper STT, ElevenLabs/Piper TTS) | 2 | Voice is a transport concern — AI adapter always works with text. |
| Multi-model (Claude, GPT, Gemini, etc.) | `OpenAiCompatAdapter` + `AnthropicAdapter` + `OllamaAdapter` | 2 | Single `OpenAiCompatAdapter` covers any OpenAI-format API (GPT, Gemini, Kimi, DeepSeek, OpenRouter). |
| SOUL.md persona | `springtale-bot` identity module (`BotId` + configurable persona rules) | 2 | Typed persona config, not raw Markdown. |
| MEMORY.md + daily logs | `springtale-bot` memory module (SQLite-backed, encrypted) | 2 | Structured, queryable, encrypted at rest. OpenClaw uses plaintext Markdown on disk. |
| Context compaction (/compact) | `springtale-bot` memory compaction module | 2 | Automatic summarization when context exceeds threshold. |
| Sub-agents | `springtale-bot` recursive pipeline orchestration | 2 | Capability-scoped children with fuel budgets. OpenClaw sub-agents inherit parent's full access. |
| Skills (13K+ in ClawHub) | Connector registry (signed manifests, sandboxed execution) | 2 | Quality over quantity. Signed, sandboxed, capability-declared. OpenClaw's ClawHub has no vetting (Cisco exfiltration finding). |
| Self-improving (agent writes skills) | Connector generation via AI adapter + manifest template | 2 | AI can generate connector code, but the result must be signed and sandboxed before loading. No self-elevation. |
| Web dashboard (Control UI) | `springtale-dashboard` SolidJS SPA on management API | 2 | Binds `127.0.0.1` by default (OpenClaw binds `0.0.0.0`). HMAC auth required. Shared component library with Tauri app. |
| Canvas / A2UI | Tauri Canvas window — agent pushes structured data, SolidJS renders reactively | 2 | Native Tauri window, not a browser tab. Same security boundary as the main app. |
| iOS + Android companion apps | Tauri 2 mobile build from same codebase | 2 | Single codebase → macOS, Windows, Linux, iOS, Android. Swift + Kotlin plugins for device features (camera, NFC, biometric, push notifications). |
| Camera / screen recording | Tauri 2 camera plugin (iOS + Android) | 2 | QR scanning for device pairing and connector install. Screen recording via platform APIs. |
| Images / audio / documents in/out | Multimedia pipeline: typed `Attachment` in springtale-core, vision + STT routing in springtale-ai | 2 | Connectors send/receive rich media natively. Image analysis via AI vision models. Audio transcription via STT. |
| Smart home (Hue, HA) | Community connectors via TypeScript SDK | 2+ | Home Assistant, Hue ship as community connectors. Same sandbox guarantees. |
| **P2P encrypted AI chat** | **Not possible.** All OpenClaw chat goes through centralized services (Telegram, WhatsApp, Discord). Server sees plaintext. Phone number required. | **3** | **`connector-rekindle`: E2E encrypted DM with your AI agent over Veilid P2P. No server. No phone number. No metadata. The conversation exists only on your device and the bot's device.** |

**Springtale covers every OpenClaw feature by Phase 2 completion with equal
or superior security, privacy, and architectural quality. Niche connectors
(Feishu, LINE, Zalo, smart home) ship as community connectors via the
TypeScript SDK — the framework enables coverage without first-party effort.**

**Phase 3 adds a capability OpenClaw cannot match:** `connector-rekindle`
provides E2E encrypted P2P AI chat over Veilid — no centralized service,
no phone number, no metadata leakage, no server that can be subpoenaed.
This is architecturally impossible for OpenClaw because all of its chat
channels (Telegram, WhatsApp, Discord) require a centralized intermediary.

### 1.4 Migration Path for OpenClaw Users

Phase 2 ships with a `connector-openclaw-compat` adapter that reads
existing OpenClaw skill directories and wraps them as sandboxed connectors:

1. Read OpenClaw `SKILL.md` → generate `connector.toml` manifest
2. Wrap skill shell commands as `connector-shell` actions with allow-list
3. Import OpenClaw memory (MEMORY.md + daily logs) → `springtale-store` migration
4. Map OpenClaw channel configs → Springtale connector configs
5. Run in compatibility mode during transition, then migrate to native connectors

---

## 1.5 Competitive Analysis: NosytLabs

NosytLabs is the Phase 1a target — Springtale's connector framework makes
their approach (ad-hoc, per-service MCP servers) obsolete.

### 1.5.1 NosytLabs Product Catalog (as of 2026-03-28)

| Product | Status | Technology | Security |
|---|---|---|---|
| KickMCP | Repos no longer publicly accessible | TypeScript | No sandboxing, no signing, no capabilities |
| presearch-search-api-mcp | Repos no longer publicly accessible | TypeScript | No sandboxing, no signing, no capabilities |
| presearch-search-skill | 2 stars, Python | OpenClaw SKILL.md format | Inherits OpenClaw's zero-sandbox model |
| openclaw-droid | 27 stars, Shell | OpenClaw for Android Termux | Same security issues as OpenClaw |

### 1.5.2 Why the Framework Beats Ad-Hoc Servers

| Concern | NosytLabs Approach | Springtale Approach |
|---|---|---|
| **New service integration** | Write new MCP server from scratch | Write connector manifest + impl Connector trait |
| **Security model** | Each server handles its own | Framework enforces: sandbox, signing, capabilities |
| **Code review burden** | Review entire server per service | Review connector code only; framework is trusted |
| **Supply chain** | npm packages, no vetting | Ed25519 signed manifests, WASM sandbox, capability allow-list |
| **Secret handling** | Environment variables, config files | `Secret<T>` types, vault encryption, zeroize on drop |
| **MCP compatibility** | Native MCP servers | Any Connector auto-becomes MCP server via `springtale-mcp` |

### 1.5.3 MCP Protocol Security Context

The MCP ecosystem has demonstrated real supply chain risk:
- **CVE-2025-6514** (CVSS 9.6): Critical RCE in `mcp-remote` — arbitrary
  OS command execution when clients connect to untrusted servers.
- **2025 Postmark MCP breach**: npm backdoor in an MCP server package
  blind-copied every outgoing email to attackers.
- **5 of 7 evaluated MCP clients** do not implement static validation of
  tool descriptions (Pillar Security research, 2025).

Springtale's connector sandbox (WASM isolation + Ed25519 signed manifests +
capability allow-list) prevents this entire attack class by design.

---

## 2. AppSec Verification & Compliance

Springtale is designed to pass OWASP ASVS Level 2 verification, defend against
MITRE ATT&CK techniques relevant to agent frameworks, and follow Tanya Janca's
Minimal Viable Security principles. This section documents the tooling,
controls, and verification approach.

### 2.1 Security Tooling Stack (CI Pipeline)

All tools run in CI. No PR merges without all passing. This follows Tanya Janca's
minimum viable security stack: SAST + SCA + DAST + secrets scanning.

**Static Analysis (SAST):**

| Tool | Scope | Purpose |
|---|---|---|
| `cargo clippy` | Rust source | Lint-level security checks, `#![deny(clippy::unwrap_used)]`, pedantic warnings-as-errors |
| `semgrep` | Rust + TypeScript | Pattern-based SAST with OWASP rules, custom rules for `Secret<T>` misuse, `expose_secret()` audit |
| `cargo-geiger` | Rust dependencies | Count `unsafe` blocks in dependency tree, fail if threshold exceeded |

**Software Composition Analysis (SCA):**

| Tool | Scope | Purpose |
|---|---|---|
| `cargo deny` | Rust crates | License policy (deny copyleft in linked code), RustSec advisory DB, duplicate detection |
| `cargo audit` | Rust crates | RUSTSEC advisory database, fail-on-known-vulnerability |
| `trivy` | Container images | OS package CVEs, Rust binary CVEs, Dockerfile misconfiguration |
| `pnpm audit` | TypeScript SDK | npm advisory database for connector-sdk-ts only |

**Dynamic Analysis (DAST):**

| Tool | Scope | Purpose |
|---|---|---|
| OWASP ZAP | `springtaled` management API | Automated scan of all HTTP endpoints, auth bypass detection, CORS misconfiguration |
| OWASP ZAP | `springtale-dashboard` | XSS, CSRF, clickjacking, Content-Security-Policy validation |
| `cargo-fuzz` | Crypto, codec, parser crates | libFuzzer-based fuzzing for `springtale-crypto` (vault, signing) and connector manifest parsing |

**Secrets Detection:**

| Tool | Scope | Purpose |
|---|---|---|
| `gitleaks` | Git history | Detect leaked API keys, tokens, passwords in all commits |
| `trufflehog` | Git history + filesystem | High-entropy string detection, verified credential checking |

**Supply Chain & SBOM:**

| Tool | Scope | Purpose |
|---|---|---|
| `cargo-sbom` | Rust workspace | Generate CycloneDX SBOM for all crates and dependencies |
| `syft` | Container images | Generate SBOM for Docker images (OS + Rust binaries) |
| SLSA provenance | CI artifacts | Build provenance attestation for signed releases |

**Infrastructure / Container:**

| Tool | Scope | Purpose |
|---|---|---|
| `hadolint` | Dockerfiles | Dockerfile best-practice linting |
| `dockle` | Built images | CIS Docker Benchmark checks |
| `cosign` | Release artifacts | Sigstore signing for container images and binaries |


## 3. OWASP ASVS 5.0 Mapping (Level 2 Target)

Springtale targets ASVS Level 2 (standard security for apps handling sensitive data).
Below maps each ASVS domain to specific Springtale controls.

| ASVS Domain | Springtale Controls | Verification |
|---|---|---|
| **V1: Architecture & Threat Modeling** | Threat model in §2.1, capability grant model §2.3, defense-in-depth (WASM sandbox + signing + capability checks). STRIDE threat model maintained as living document. | Manual review, threat model updates per release |
| **V2: Authentication** | `springtaled` management API: HMAC bearer tokens, no JWT. Tauri shell: OS-level auth for vault unlock. Connector OAuth2: PKCE flow only, no implicit grant. | ZAP auth bypass scan, manual review of auth flows |
| **V3: Session Management** | Management API: stateless HMAC tokens, no server-side sessions. Dashboard: short-lived tokens, no localStorage (SolidJS stores only). | ZAP session fixation scan |
| **V4: Access Control** | Capability grant model: connectors declare permissions, user approves. `ShellExec` requires blocking modal. No privilege self-elevation from sandbox. | Manual review, fuzzing of capability enforcement in `host_api.rs` |
| **V5: Input Validation** | `garde` derive-based validation on all API inputs. Connector manifest: canonical JSON with schema validation. No raw SQL (`sqlx::query_as!` only). | Semgrep rules for unvalidated input, ZAP injection scans |
| **V6: Cryptography** | Ed25519 signing (`ed25519-dalek`), XChaCha20-Poly1305 AEAD (vault), Argon2id KDF, rustls-tls exclusively. No OpenSSL. No deprecated ciphers. | `cargo-geiger` for unsafe in crypto path, manual review of `expose_secret()` sites |
| **V7: Error Handling & Logging** | `tracing` structured logging with `Secret<T>` redaction filter. `thiserror` typed errors in libraries, `anyhow` in apps only. No stack traces in API responses. | Semgrep rules for `Debug` on types containing secrets |
| **V8: Data Protection** | `Secret<T>` wrapper on all credentials. `zeroize` on drop. Vault encrypted at rest (XChaCha20-Poly1305). Database payloads encrypted at app layer. No secrets in logs, URLs, or error messages. | `gitleaks` + `trufflehog` in CI, Semgrep rules for Secret misuse |
| **V9: Communication Security** | `rustls-tls` only (`native-tls` banned via Cargo patch). TLS cert validation never disabled. mTLS for Phase 2 `HttpTransport`. Phase 3: Veilid private routes. | `cargo deny` verifies no `native-tls` in dependency tree |
| **V10: Malicious Code** | Connector sandboxing (Wasmtime fuel + memory limits). Ed25519 manifest signing. `cargo deny` + `cargo audit` for supply chain. No eval/exec in Rust. TypeScript only in WASM sandbox. | SCA pipeline, manifest signature verification tests |
| **V11: Business Logic** | Pipeline stages are deterministic. Rule conditions are typed enums (no arbitrary code execution). Retry logic has max attempts + exponential backoff with jitter. | Unit tests for rule evaluation edge cases |
| **V12: Files & Resources** | Connector filesystem access: path allow-list in manifest. No symlink following. Upload size limits. Temporary files in sandboxed directory. | Fuzzing of filesystem connector path validation |
| **V13: API & Web Services** | Management API: typed extractors (axum), HMAC auth, rate limiting (tower-http), CORS explicit origin list. No wildcard CORS. No JSONP. | ZAP full scan of API surface |
| **V14: Configuration** | `figment` layered config (TOML + env). No secrets in config files (use vault or env). Debug mode disabled in release. Dashboard binds `127.0.0.1` by default. | `hadolint` for Dockerfile, manual review of default configs |


## 4. MITRE ATT&CK Technique Mapping

Relevant ATT&CK techniques for an agent framework and their mitigations:

| ATT&CK ID | Technique | Springtale Mitigation |
|---|---|---|
| T1195.002 | Supply Chain: Software Supply Chain Compromise | Rust for all trusted code. `cargo deny` + `cargo audit`. Ed25519 signed connector manifests. TypeScript runs in WASM sandbox only. SBOM generation via `cargo-sbom`. |
| T1059 | Command and Scripting Interpreter | `connector-shell` requires `ShellExec` capability + user approval modal. Command allow-list enforced at runtime. No dynamic command construction. Timeout on all exec. |
| T1055 | Process Injection | Rust memory safety. `#![forbid(unsafe_code)]` on 7 of 9 crates. `#![deny(unsafe_code)]` on crypto/connector crates with audited `unsafe` blocks. WASM sandbox prevents guest memory access to host. |
| T1003 | OS Credential Dumping | `Secret<T>` + `zeroize`: credentials zeroed on drop. No credentials in process memory longer than needed. Vault encrypted at rest with Argon2id-derived key. |
| T1071 | Application Layer Protocol (C2) | All connector network access declared in manifest. `network:outbound` requires exact host, no wildcards. Phase 3: Veilid private routes prevent IP correlation. |
| T1027 | Obfuscated Files or Information | Connector WASM binaries are inspectable (`wasm-validate`). Manifest is canonical JSON, human-readable. No minified/obfuscated code in trusted path. |
| T1098 | Account Manipulation | Connector registry: author pubkey pinned at registration. No pubkey rotation without governance action (Phase 3 DHT). Capability grants non-transferable. |
| T1496 | Resource Hijacking | Wasmtime fuel metering (10M instructions per call). Memory limit 64 MiB per connector. Recursive pipeline fuel inheritance (parent/4). |
| T1530 | Data from Cloud Storage | No cloud storage in architecture. All data local (PostgreSQL, SQLite, vault). Phase 3: Veilid DHT (replicated but encrypted). |
| T1567 | Exfiltration Over Web Service | Connector `network:outbound` declares exact hosts. No wildcard allowed. Cisco demonstrated this attack on OpenClaw — Springtale's capability model prevents it by design. |
| T1204 | User Execution via Malicious Skill (ClawHavoc) | ClawHub's 824+ malicious skills (Atomic Stealer, keyloggers) are impossible in Springtale: (1) connectors cannot instruct users to download external binaries — WASM sandbox has no shell access unless `ShellExec` explicitly declared and user-approved via blocking modal, (2) Ed25519 manifest signing prevents post-publish tampering, (3) connector WASM binaries scanned by `trivy` + `wasm-validate` before registry publication, (4) `connector-shell` command allow-list prevents `curl \| bash` patterns. |
| T1659 | Content Injection / Indirect Prompt Injection (Zenity) | Zenity demonstrated: malicious Google Doc → agent creates rogue Telegram integration → attacker gains C2. Impossible in Springtale: (1) connector installation requires signed manifest + user approval modal — AI adapter cannot autonomously install connectors, (2) `AiRequest` closed enum prevents raw document content from reaching AI as instructions, (3) connector output is typed and sanitized, never passed as raw prompt, (4) new connector registration requires `springtale-cli connector install` with manifest verification — no hot-loading from conversation context. |
| — | Persistent Memory Poisoning (Palo Alto "stateful delayed-execution") | Palo Alto described attacks persisting in OpenClaw's Markdown memory files across sessions. Springtale mitigations: (1) memory is SQLite-backed with app-layer encryption, not plaintext Markdown, (2) memory write operations go through `springtale-bot` memory module with typed schemas — no arbitrary string injection, (3) memory compaction summarizes via AI adapter but output is validated before storage, (4) `springtale-cli memory audit` command allows users to inspect and purge memory entries. |
| — | Credential Theft via Infostealer (RedLine/Lumma/Vidar/AMOS) | Infostealers specifically target `~/.openclaw/` plaintext files. Springtale mitigations: (1) all secrets in encrypted vault (XChaCha20-Poly1305 + Argon2id KDF) — no plaintext credential files, (2) `Secret<T>` + `zeroize` ensures credentials zeroed from memory on drop, (3) vault unlock requires passphrase (not stored on disk), (4) no `~/.springtale/` directory contains any credential material in recoverable form. |
| — | WebSocket Hijack / ClawJacked (Oasis Security, CVE-2026-25253) | OpenClaw's localhost WebSocket assumed local = trusted. Springtale has no WebSocket gateway. Management API is HTTP REST with HMAC bearer tokens, `tower-http` rate limiting, and `127.0.0.1` default binding. The ClawJacked attack vector does not exist. |
| — | Default Network Exposure (135K+ instances, SecurityScorecard) | OpenClaw binds `0.0.0.0:18789` by default. Springtale: `springtaled` and `springtale-dashboard` both bind `127.0.0.1` by default. Config validation warns if binding to `0.0.0.0`. Remote access requires explicit `--bind` flag or SSH tunnel / Tailscale / VPN. |


## 5. MITRE ATLAS Mapping (AI-Specific Threats)

| ATLAS ID | Technique | Springtale Mitigation |
|---|---|---|
| AML.T0043 | Prompt Injection | `AiRequest` is a closed enum — AI backend never receives raw connector output. Structured typed requests only. `NoopAdapter` eliminates this entirely. |
| AML.T0040 | Model Poisoning | Local models only by default (`OllamaAdapter`). Remote API calls use `rustls-tls`. No model weight download from untrusted sources. |
| AML.T0048 | Exfiltration via AI API | AI adapter requests contain only typed `AiRequest` data. No raw user secrets cross the AI boundary. Connector output is sanitized before AI ingestion. |
| AML.T0051 | Supply Chain: ML Model | Model selection is user-configured. No auto-download of models. Ollama models verified via digest. Remote API calls go to user-configured endpoints only. |


## 6. Tanya Janca — Minimal Viable Security Checklist

Based on SheHacksPurple's "AppSec Poverty Line" framework and
"Alice and Bob Learn Application Security" principles:

| Practice | Springtale Implementation |
|---|---|
| **Train devs to write secure code** | `CONTRIBUTING.md` includes security guidelines. `Secret<T>` usage is enforced by type system, not by convention. Clippy security lints are errors, not warnings. |
| **Threat model before you code** | §2.1 threat model maintained as living document. Updated per phase. STRIDE analysis for each new connector and transport. |
| **SAST in every build** | `cargo clippy` + `semgrep` in CI. No PR merges without passing. Custom Semgrep rules for Springtale-specific patterns. |
| **SCA for dependencies** | `cargo deny` + `cargo audit` + `trivy`. Dependency updates tracked. `Cargo.lock` committed. No transitive `native-tls`. |
| **DAST on running app** | OWASP ZAP scans `springtaled` API and `springtale-dashboard` in CI against staging environment. |
| **Secrets scanning** | `gitleaks` + `trufflehog` on every commit. Pre-commit hook available. `Secret<T>` prevents accidental exposure in code. |
| **Security headers** | `tower-http` middleware: strict CSP, X-Frame-Options DENY, X-Content-Type-Options nosniff, HSTS. Dashboard: SameSite=Strict cookies. |
| **Never hard-code secrets** | Enforced by type system: `Secret<T>` created at config parse time only. `figment` reads from env/vault, not hardcoded strings. |
| **No person/app should speak directly to DB** | All DB access through `springtale-store` crate. `sqlx::query_as!` compile-time verified. No raw SQL strings. No direct DB connections from connectors. |
| **Input validation everywhere** | `garde` derive macros on all API input types. Connector manifest: JSON Schema validation. Pipeline context: typed, never raw strings. |
| **Logging without leaking** | `tracing` subscriber with `Secret<T>` redaction. No stack traces in API responses. Error types are opaque to callers. |
| **Encrypt at rest and in transit** | Vault: XChaCha20-Poly1305. DB payloads: app-layer encryption. Transit: rustls-tls everywhere. Phase 3: Veilid E2E. |


---

## 7. OWASP Agentic Top 10 (ASI01-ASI10) — Full Mapping

See ARCHITECTURE.md §15 (Proactive Architecture) for the runtime implementations
(autonomy levels, sentinel, toxic combination engine, agent identity, memory provenance).

| ASI | Risk | Springtale Proactive Control |
|---|---|---|
| **ASI01** | Agent Goal Hijack | `AiRequest` closed enum — AI never receives raw external content as instructions. Behavioral monitor (`springtale-sentinel`) scores action trajectories against declared intent. Anomaly score > threshold → automatic pause + user notification. |
| **ASI02** | Tool Misuse & Exploitation | Toxic combination policy engine: capability pairs that create exfiltration paths (e.g., `KeychainRead` + `NetworkOutbound` to different host) are blocked at install time. Runtime: tool invocation sequence logging with anomaly detection on call patterns. |
| **ASI03** | Identity & Privilege Abuse | Agents get their own `AgentIdentity` (Ed25519 keypair), separate from user identity. Agents never inherit user sessions or cached tokens. Credential scope is per-connector, per-task, with automatic expiry. `springtale-crypto` issues short-lived `CapabilityToken` grants that expire after task completion. |
| **ASI04** | Supply Chain Vulnerabilities | Ed25519 signed manifests. WASM sandbox. `cargo deny` + `cargo audit`. Connector WASM scanned by `trivy` + `wasm-validate`. No runtime loading of unsigned components. Registry rejects unsigned uploads. |
| **ASI05** | Unexpected Code Execution | No `eval()`, no dynamic code generation in host. `connector-shell` requires `ShellExec` capability + blocking approval modal + command allow-list. WASM sandbox prevents arbitrary host code execution. AI-generated code goes through manifest signing before it can be loaded as a connector. |
| **ASI06** | Memory & Context Poisoning | Memory provenance: every write records `(author, timestamp, source_connector, content_hash)`. Write scanning: new entries validated against schema before commit. Memory segmented per context (per-connector, per-conversation). Unverified data expires after configurable TTL. Versioned snapshots with rollback via `springtale-cli memory rollback <version>`. No self-reinforcing loops (agent's own output is tagged and down-weighted on re-read). |
| **ASI07** | Insecure Inter-Agent Communication | Phase 2: sub-agent messages signed with parent's `AgentIdentity` + HMAC. Phase 3: Veilid private routes with Ed25519 signed envelopes. Zero-trust between all agents — every message verified regardless of origin. No implicit trust based on localhost or same-process. |
| **ASI08** | Cascading Failures | Circuit breakers at every pipeline stage: 3 consecutive failures → stage disabled, pipeline halted, user notified. Blast radius containment: child pipeline failures cannot propagate to parent (read-only context snapshots). Dead-man's switch: if `springtale-sentinel` detects > N actions/minute without user interaction, all pipelines pause. Automatic rollback: failed pipeline stages emit compensating actions where possible. |
| **ASI09** | Human-Agent Trust Exploitation | Consent separation: high-stakes actions (send email, delete files, financial transactions, create new integrations) require out-of-band approval via Tauri native modal — NOT the chat interface. The agent cannot approve its own requests. Confidence scoring: AI adapter responses include a confidence field; low-confidence actions require human review. Action impact classification: each connector action tagged as `read-only`, `reversible`, or `destructive`. Destructive actions always require approval. |
| **ASI10** | Rogue Agent Detection | `springtale-sentinel` behavioral baseline per agent: tracks tool invocation patterns, data access patterns, action frequency, reasoning chain length. Deviation from baseline triggers graduated response: alert → throttle → pause → quarantine. Quarantined agents continue logging but cannot execute actions. Admin review required to restore. |


---

## 8. Privacy by Design — Cavoukian 7 Principles


| Principle | Springtale Implementation |
|---|---|
| **1. Proactive, not reactive** | `springtale-sentinel` runtime behavioral monitor. Threat model maintained per phase. Toxic combination policy engine blocks attacks at install time, before any code runs. |
| **2. Privacy as default** | Local-first architecture. All data in local SQLite/vault. No cloud dependency. `127.0.0.1` default binding. NoopAdapter default (no data sent to AI providers unless user opts in). L1 autonomy default (suggest only, no autonomous action). |
| **3. Privacy embedded in design** | `Secret<T>` type system — secrets cannot be logged, displayed, or cloned by accident. Veilid private routes (Phase 3) — no IP addresses leak. HKDF pseudonyms — cross-community identity unlinkability. Memory encryption at rest. Transport encryption in transit. |
| **4. Full functionality** | Privacy doesn't reduce capability. Platform works fully with `NoopAdapter` + local-only operation. Capability model enables fine-grained feature access without all-or-nothing privacy tradeoffs. |
| **5. End-to-end security** | Data lifecycle management: collection → storage (encrypted) → processing (sandboxed) → retention (TTL) → deletion (`springtale-cli data purge`). Full audit trail from creation to deletion. |
| **6. Visibility & transparency** | Privacy dashboard in `springtale-dashboard`: per-connector data collection disclosure, user data inventory, export (`springtale-cli data export`), deletion (`springtale-cli data purge`). Audit trail queryable by user. |
| **7. Respect for user privacy** | Data minimization: connectors declare what data they collect in manifest (required field). Purpose limitation: data collected for declared purpose only. Consent records: every high-stakes action logged with user approval timestamp. Right to erasure: `springtale-cli data purge --connector <name>` removes all data from a specific connector. |


---

## 9. CI Security Pipeline & Dependency Supply Chain

### 9.1 CI Security Pipeline (Complete)

```
# ── Phase: Pre-commit ──────────────────────────────────────────────────────
gitleaks protect --staged               # secrets in staged changes

# ── Phase: Build ───────────────────────────────────────────────────────────
cargo fmt --check                       # formatting
cargo clippy --workspace --all-targets -- -D warnings  # SAST (Rust lints)
semgrep --config=p/owasp-top-ten --config=.semgrep/  # SAST (pattern rules)
cargo geiger --all-features             # unsafe audit

# ── Phase: Test ────────────────────────────────────────────────────────────
cargo nextest run --workspace           # unit + integration tests
cargo test --doc                        # doc tests
cargo fuzz run fuzz_manifest -- -max_total_time=120   # fuzz manifest parser
cargo fuzz run fuzz_vault -- -max_total_time=120      # fuzz vault encrypt/decrypt

# ── Phase: SCA ─────────────────────────────────────────────────────────────
cargo deny check                        # licenses + advisories + bans
cargo audit                             # RustSec advisory DB
cargo vet                               # supply chain audit — all deps reviewed
pnpm -C sdk/connector-sdk-ts audit      # npm advisories (SDK only)
trufflehog git file://. --only-verified # secrets in full git history

# ── Phase: Database ────────────────────────────────────────────────────────
sqlx migrate run && cargo sqlx prepare --check  # migration + query verification

# ── Phase: Container ───────────────────────────────────────────────────────
hadolint Dockerfile                     # Dockerfile linting
docker compose build --no-cache
trivy image springtale:latest --severity HIGH,CRITICAL  # CVE scan
dockle springtale:latest                # CIS Docker Benchmark

# ── Phase: DAST (staging) ──────────────────────────────────────────────────
zap-cli quick-scan http://localhost:8080 --self-contained  # springtaled API
zap-cli quick-scan http://localhost:8081 --self-contained  # springtale-dashboard

# ── Phase: SBOM + Signing ──────────────────────────────────────────────────
cargo auditable build --release --locked  # embed dep info in binary
cargo sbom > sbom.cdx.json              # CycloneDX SBOM
syft springtale:latest -o cyclonedx-json > container-sbom.cdx.json
cosign sign --key cosign.key springtale:latest  # Sigstore image signing

# ── Phase: TypeScript SDK ──────────────────────────────────────────────────
pnpm -C sdk/connector-sdk-ts run check  # tsc + biome lint
pnpm -C sdk/connector-sdk-ts run test   # vitest
```

No PR merges without all phases passing. No exceptions.

### 9.2 Dependency Supply Chain Security Audit (as of 2026-03-27) Dependency Supply Chain Security Audit (as of 2026-03-27)

Every dependency is attack surface. This section audits each workspace
dependency against RustSec advisories, CVE databases, maintenance status,
and cryptographic trust chain.

**Cryptography stack:**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `ed25519-dalek` | 2 | RUSTSEC-2022-0093 **fixed in 2.0**. No active advisories. | Low | Double-pubkey oracle attack fixed in our minimum version. Maintained by dalek-cryptography team. |
| `chacha20poly1305` | 0.10 | No advisories. | Low | RustCrypto project. Pure Rust. No `unsafe`. Audited by NCC Group (2019). |
| `argon2` | 0.5 | No advisories. | Low | RustCrypto project. OWASP recommended KDF. |
| `sha2` / `hmac` | 0.10 / 0.12 | No advisories. | Low | RustCrypto project. Pure Rust. |
| `rand` | 0.9 | No advisories. | Low | Uses OS CSPRNG via `getrandom`. |

**TLS chain (critical path — all network traffic flows through this):**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `rustls` (transitive via reqwest) | 0.23+ | No active advisories. | **Medium** | Default crypto backend switched from `ring` to `aws-lc-rs` in 2024. FIPS-validated crypto primitives. |
| `ring` (transitive) | — | RUSTSEC-2025-0007: **maintainer quit**. Ownership transferred to rustls team. Security-only maintenance. | **Medium** | We use `aws-lc-rs` backend (rustls default), not `ring`. Ring may appear as transitive dep from older crates. `cargo deny` policy: warn on `ring` transitive pulls. |
| `aws-lc-rs` (transitive via rustls) | 1.x | No advisories. | Low | AWS-maintained. FIPS 140-3 validated. C backend (AWS-LC/BoringSSL fork). |
| `reqwest` | 0.13 | No active advisories. | Low | Uses rustls with aws-lc-rs backend. We specify `rustls-tls` feature — no OpenSSL path. |

**WASM sandbox (critical — contains untrusted connector code):**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `wasmtime` | **42** | CVE-2026-24116: buffer overflow on x86-64 AVX, fixed 41.0.1. CVE-2026-27572: wasi-http header DoS, fixed 41.0.4. CVE-2026-27195: call_async Future DoS, fixed 41.0.4. **CVE-2026-27204: resource exhaustion via WASI host interfaces, fixed 42.0.0.** CVE-2025-62711: component-model trampoline crash, fixed 38.0.3. | **High (if unpinned)** | Pin updated to ≥42.0.0 to cover CVE-2026-27204 (guest-controlled resource exhaustion). Bytecode Alliance maintains. 24/7 OSS-Fuzz. |
| `wasmtime-wasi` | **42** | Same as wasmtime. | **High (if unpinned)** | Same pin requirement. |

**Database:**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `sqlx` | **≥0.8.3** | RUSTSEC-2024-0363: binary protocol misinterpretation (truncating casts), presented at DEFCON 32. Fixed 0.8.3. | **Medium (if unpinned)** | We pin ≥0.8.3. All queries use `query_as!` macro (compile-time verified). No raw SQL construction. Protocol-level injection mitigated by prepared statements. |
| `rusqlite` | 0.32 | No active advisories. | Low | Bundled SQLite (no system dependency). Bot memory only — not on network path. |

**Web framework:**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `axum` | 0.8 | No advisories. | Low | tokio-rs maintained. `#![forbid(unsafe_code)]`. Type-safe extractors prevent injection classes. |
| `tower-http` | 0.6 | No advisories. | Low | Middleware for security headers, CORS, rate limiting. |

**Serialization & config:**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `serde` / `serde_json` | 1.x / 1.x | No advisories. | Low | De facto standard. Heavily audited. |
| `schemars` | 1.0 | No advisories. | Low | JSON Schema generation. No unsafe. |
| `figment` | 0.10 | No advisories. | Low | Config layering. Supports `Secret<T>` natively. |
| `toml` | 0.8 | No advisories. | Low | TOML parser. |

**Secret management:**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `secrecy` | 0.10 | No advisories. | Low | Prevents `Debug`/`Display`/`Clone` on secret values. `zeroize`-on-drop. |
| `zeroize` | 1.x | No advisories. | Low | Zeroes memory on drop. Used by secrecy, ed25519-dalek, argon2. |

**Concurrency & scheduling:**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `tokio` | 1 (LTS 1.47.x) | No active advisories. | Low | LTS until Sep 2026. MSRV 1.70. |
| `dashmap` | 6 | No advisories. | Low | Concurrent HashMap. No unsafe in public API. |
| `cron` | 0.13 | No advisories. | Low | Cron expression parser. No network, no IO. |

**Validation & identity:**

| Crate | Version | Advisory Status | Risk | Notes |
|---|---|---|---|---|
| `garde` | 0.22 | No advisories. | Low | Derive-based validation. Pure Rust. |
| `uuid` | 1.x | No advisories. | Low | v4 random UUIDs. Uses `getrandom`. |
| `chrono` | 0.4 | RUSTSEC-2020-0159 (localtime_r unsoundness): **fixed in 0.4.20+**. | Low | We use 0.4 latest. Fixed. |

**`cargo deny` policy (enforced in CI):**

```toml
# deny.toml

[advisories]
vulnerability = "deny"       # fail on any known vulnerability
unmaintained = "warn"         # warn on unmaintained crates (ring)
yanked = "deny"               # fail on yanked crates
notice = "warn"

[licenses]
unlicensed = "deny"
copyleft = "deny"             # no GPL/AGPL in linked code
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0"]

[bans]
multiple-versions = "warn"
deny = [
  { name = "native-tls" },   # force rustls everywhere
  { name = "openssl-sys" },  # no OpenSSL in dependency tree
  { name = "openssl" },
]

[bans.skip]
# ring may appear as transitive from older deps — tracked, not blocked
# aws-lc-rs is the active crypto backend
```

**Proactive dependency security policy (beyond reactive scanning):**

The tools above (`cargo audit`, `cargo deny`, `trivy`) are reactive — they catch
known vulnerabilities at build time. The following policies prevent vulnerable
or malicious dependencies from entering the codebase in the first place.

**1. `cargo-vet` — supply chain audit trail (every dependency change reviewed):**

Every new dependency and every dependency version update requires an audit
before merge. `cargo vet` enforces this in CI — builds fail if any dependency
lacks an audit, exemption, or trust declaration. We import audit opinions
from Google and Mozilla. Audits are NOT imported transitively — we trust
specific orgs, not their trust chains. New crypto-related dependencies
require `safe-to-deploy` review from a team member.

**2. Dependency introduction gate (new crates require PR review):**

Adding a new direct dependency requires a dedicated review before merge:
- Purpose justification: why can't this be done with existing deps?
- Maintenance check: last commit date, release frequency, bus factor (>1 maintainer?)
- `unsafe` audit: `cargo geiger` output included in PR. High `unsafe` count requires justification.
- Transitive impact: `cargo tree -i <new-dep>` output included in PR.
- License + RustSec: `cargo deny check` must pass.

**3. `cargo-auditable` — embed dependency info in release binaries:**

All release builds use `cargo auditable build --release`. This embeds the full
dependency list in the binary itself. Post-deployment auditing:
`rust-audit-info springtaled | cargo audit --json` scans a deployed binary
for vulnerabilities without access to the source tree.

**4. Trusted Publishing for Springtale crates (no API tokens):**

When we publish `springtale-*` crates to crates.io, we use Trusted Publishing
(OIDC-based, GitHub Actions → crates.io). Enforce-only mode: traditional API
token publishing disabled in crate settings. Prevents leaked tokens from
publishing poisoned versions.

**5. Continuous advisory monitoring (not just at build time):**

- Dependabot configured for security-only PRs within 24 hours of new RustSec advisories.
- GitHub Advisory Database alerts enabled on repository.
- SBOM published as release artifact — downstream consumers correlate against
  live advisory feeds (OSV, RustSec, NVD).

**6. Build reproducibility:**

- `cargo build --release --locked` in CI (fail if lockfile doesn't match)
- `Cargo.lock` committed to repository
- Release binaries signed with `cosign` (Sigstore)
- Container images built with SBOM attestation (`docker buildx --attest type=sbom`)

**7. Transitive dependency monitoring:**

`ring` (RUSTSEC-2025-0007) taught us that critical deps can lose maintenance
overnight. Policy: quarterly review of all direct dependencies for last release
date, open security issues, maintainer count. Flagged crates get migration plan.
`cargo tree --duplicates` in CI detects outdated transitive pulls.

---


---

*End of Security Whitepaper*
