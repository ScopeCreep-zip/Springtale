# Architecture Audit Notes
> Internal working document — findings from audit of intended-arch docs
> Audited: 2026-03-28
> Last updated: 2026-03-28 (external validation complete)

## Research Sources

### OpenClaw (Phase 2 Target)
- 250K+ GitHub stars, 40K+ public instances, 2M weekly visitors
- CVE-2026-25253: Critical RCE via gatewayUrl parameter (CVSS 8.8)
- CVE-2026-32025: Auth bypass on WebSocket loopback deployments
- CVE-2026-24763, CVE-2026-25157: Command injection vulnerabilities
- CVE-2025-64496: Open WebUI SSE injection, account takeover (CVSS 7.3)
- Cisco AI Defense: 800+ malicious skills in ClawHub (~20% of registry)
- ClawHavoc: Coordinated supply chain attack — infostealers disguised as productivity tools
- Attack techniques: prompt injection in skill descriptors, hidden reverse shells, credential exfil
- SecurityScorecard: 40K+ public instances exposed
- Competitors: LobeChat (74K stars), LibreChat (35K stars), Open WebUI (128K stars)

### NosytLabs (Phase 1a Target)
- Small operation: 7 org repos, max 27 stars (openclaw-droid)
- KickMCP and presearch-search-api-mcp appear to have been taken down or moved
- presearch-search-skill (2 stars, Python) is an OpenClaw skill, not standalone MCP
- All TypeScript/Python — no Rust, no sandboxing, no manifest signing
- Primary products are OpenClaw skills/extensions, not standalone tools
- "employee-md" and "ai-empire" repos suggest focus on AI hype/hustle, not security
- Low adoption risk but validates Phase 1a approach of building better alternatives

### Veilid (Phase 3 Foundation)
- Active development on GitLab (gitlab.com/veilid/veilid)
- Written in Rust, runs on Linux/macOS/Windows/Android/iOS/WASM
- Community maintained via Veilid Foundation (veilid.org)
- Need to verify current version and API stability before Phase 3 planning

### MCP Protocol (Phase 1a Compatibility)
- CVE-2025-6514: Critical RCE in mcp-remote (CVSS 9.6)
- 2025 Postmark MCP supply chain breach: npm backdoor blind-copied all emails
- 5 of 7 MCP clients don't implement static validation
- CoSAI/OWASP concerns: tool poisoning, rug pulls, over-permissioning, confused deputy
- Springtale's approach (connector sandbox IS the security layer) is architecturally correct

### Wasmtime (Sandbox Foundation)
- CVE-2026-24116: Buffer overflow on x86-64 AVX (fixed 41.0.1)
- CVE-2026-27572: wasi:http header DoS (fixed 41.0.4)
- CVE-2026-27195: call_async Future DoS (fixed 41.0.4)
- CVE-2026-27204: Resource exhaustion DoS via WASI host interfaces
- Current latest: 42.0.0
- **ACTION: Update minimum pin from >=41.0.4 to >=42.0.0 to cover CVE-2026-27204**

---

## Critical Gaps Found in intended-arch

### GAP 1: No Threat Model for Most Vulnerable Users
The architecture mentions "privacy" broadly but never models threats specific to:
- Trans people facing coordinated doxxing campaigns
- POC activists under government surveillance
- People in abusive relationships with tech-savvy partners
- Immigrants/travelers facing device seizure at borders
- People in jurisdictions criminalizing their identity

**Recommendation:** Add a "Vulnerable User Threat Model" section to SECURITY.md

### GAP 2: No Duress/Panic Features
No mention of:
- Duress password (unlocks decoy profile)
- Panic wipe (emergency data destruction)
- Hidden vault / plausible deniability
- Quick-switch to innocuous app appearance
- Dead-man switch for user safety (not just bot monitoring)

**Recommendation:** Add duress features to Phase 2b (Tauri shell) and Phase 3 (Veilid)

### GAP 3: No Device Seizure Protection
The vault uses Argon2id + XChaCha20-Poly1305, which is strong, but:
- No guidance on what happens when a device is seized
- No plausible deniability for vault existence
- No discussion of full-disk encryption dependencies
- No "travel mode" to minimize data exposure at borders

**Recommendation:** Add device seizure scenarios to threat model

### GAP 4: Intimate Partner Threat Model Missing
Stalkerware and intimate partner surveillance is a primary threat for trans people:
- Partner who has physical access to device
- Partner who knows the passphrase
- Dual-use monitoring apps
- Shared household network monitoring

**Recommendation:** Add IPV (intimate partner violence) threat model

### GAP 5: Social Graph Protection Incomplete
Rekindle's HKDF pseudonyms protect cross-community identity, but:
- Group membership in Springtale bot context leaks social graph
- Connector activity patterns could reveal identity
- Rule evaluation timing could be correlated
- Bot response patterns could fingerprint users

**Recommendation:** Add social graph protection analysis

### GAP 6: NosytLabs Analysis Needs Updating
The architecture references NosytLabs' "KickMCP" and "presearch-search-api-mcp" but:
- These repos appear to have been removed or made private
- NosytLabs is primarily an OpenClaw skill author, not an MCP server publisher
- Their "presearch-search-skill" is a Python OpenClaw skill (SKILL.md), not an MCP server
- The competitive framing should be updated to reflect their actual products

**Recommendation:** Update NosytLabs competitive analysis with current facts

### GAP 7: Wasmtime Version Pin Outdated
- Intended-arch pins >=41.0.4
- CVE-2026-27204 (resource exhaustion DoS) affects 41.x
- Wasmtime 42.0.0 is current latest
- **ACTION: Update pin to >=42.0.0**

### GAP 8: OpenClaw Scale Underestimated
- Intended-arch says "310K+ GitHub stars" — current count is 250K+ (number fluctuates)
- 40K+ public instances (not 135K as cited from SecurityScorecard — may have been pre-rebrand)
- 2M weekly visitors
- ClawHub malicious skills now 800+ (arch doc says 824+ which is close but the number is growing)
- AWS now offers managed OpenClaw on Lightsail — this is a significant legitimacy boost

**Recommendation:** Update competitive analysis numbers

### GAP 9: No Accessibility Considerations
For a platform targeting marginalized communities:
- No mention of accessibility (screen readers, high contrast, etc.)
- No mention of internationalization/localization
- No consideration of low-bandwidth/intermittent connectivity scenarios
- No consideration of older/cheaper devices

**Recommendation:** Add accessibility section

### GAP 10: Migration Path Underspecified
The OpenClaw migration path (§1.3 in SECURITY.md) is ambitious but vague:
- "Read OpenClaw SKILL.md → generate connector.toml" needs more detail
- How to handle the 2,800+ legitimate skills in ClawHub
- How to handle user data migration from plaintext Markdown
- Timeline and tooling for the migration

---

## What's Strong (Keep)

1. **Transport abstraction** — clean, well-designed, phase-appropriate
2. **Connector sandbox model** — WASM + signing + capabilities is genuinely better than everything else
3. **NoopAdapter philosophy** — AI-optional is the right call for longevity
4. **Secret<T> type system** — compile-time prevention of secret leakage
5. **Rekindle integration plan** — Phase 3 connector-rekindle is architecturally sound
6. **CRDT governance** — flat, no-coordinator model is correct for the target community
7. **Recursive pipeline** — Clicky-derived fuel metering is well-designed
8. **Security whitepaper** — OWASP ASVS, MITRE ATT&CK/ATLAS, supply chain audit are thorough
9. **Phase discipline** — clear boundaries, stubs for future work

## What Needs Work (Update in current-arch)

1. ~~Add vulnerable user threat model~~ ✅ Added §2.5-2.9
2. ~~Add duress/panic/plausible deniability features~~ ✅ Added §2.6
3. ~~Add device seizure protection~~ ✅ Added §2.7
4. ~~Update NosytLabs competitive analysis~~ ✅ Updated §13.7 + SECURITY.md §1.5
5. ~~Update OpenClaw numbers and CVE list~~ ✅ Updated ARCH §3 + SECURITY.md §1.1
6. ~~Update wasmtime pin~~ ✅ Pinned to "42" (bounded)
7. ~~Add accessibility considerations~~ ✅ Added §16
8. ~~Strengthen social graph protection~~ ✅ Added §2.9
9. ~~Add travel mode / border crossing scenario~~ ✅ Added §2.6
10. ~~Add MCP supply chain breach context~~ ✅ Added SECURITY.md §1.5

## Fixes Applied from §1-6 Technical Audit

11. ✅ Fixed connector layout mismatch (sandbox/ → connector/ + native/ + wasm/)
12. ✅ Fixed duplicate export.rs in sentinel layout
13. ✅ Fixed unbounded >= version pins for wasmtime and sqlx
14. ✅ Fixed misleading sentinel integration in core eval flow (cycle risk)
15. ✅ Added missing springtale-core dependency to springtale-ai
16. ✅ Added key revocation mechanism to manifest signing flow
17. ✅ Added flash storage limitation to panic wipe honest limitations
18. ✅ Fixed session timeout phase inconsistency (1a/2b → 1a with 2b UI)
19. ✅ Added missing backend::sqlite and backend::postgres to store layout
20. ✅ Added missing regex crate to workspace dependencies

## Fixes Applied from §7-15 Technical Audit

- Fixed jco/WASM toolchain description (TS→JS→jco componentize, not direct compile)
- Fixed WASM target: wasm32-wasi → wasm32-wasip2 (WASI Preview 2)
- Fixed Node.js 24 → 22 (24 not released yet, would break Nix flake)
- Fixed Transport trait: anyhow → TransportError (thiserror), added cancel-safety note
- Fixed registry migration: "PostgreSQL" → "local database (SQLite/PostgreSQL)"

## Fixes Applied from SECURITY.md Audit

- Fixed language contradiction in comparison table (JavaScript → Python/FastAPI)
- Fixed supply chain row (npm → PyPI + npm)
- Fixed "zero gaps" overstatement → honest Phase 2 completion framing
- Fixed duplicate §1.3 heading → renumbered to §1.4
- Added aspirational-vs-architectural disclaimer at top of doc

## Fixes Applied from Rekindle Architecture Audit

- Fixed Channel list CRDT rule (broken set-difference → proper LWW per channel_id)
- Fixed Ban/Unban CRDT rule (contradictory UNION+LWW → pure LWW per pseudonym)
- Fixed missing tiebreak on role assignments (added lowest pseudonym key)
- Fixed MEK generation tiebreak (specified lowest rotator pseudonym)
- Fixed duplicate MemberPresence lines (copy-paste artifact)

## Known Issues Not Yet Fixed (tracked for implementation)

### From §7-15 Audit (53 findings: 14 errors, 18 inconsistencies, 21 improvements)
- Ed25519-to-X25519 key conversion for DM ECDH not mentioned
- connector-whatsapp (Baileys) likely needs NativeConnector, not WasmConnector
- Management API missing /scheduler and /bot routes
- Sentinel initialized at Phase 1 startup but documented as Phase 2a
- Event loop uses ? propagation (kills bot on single message failure)
- Fuel division formula (parent/4) undefined for varying child counts
- Tauri CSP wildcard port on 127.0.0.1 too broad
- Tauri API references are Tauri 1 style (tauri::api::dialog → tauri_plugin_dialog)
- rustup in Nix shell is anti-pattern (should use fenix/rust-overlay)
- Missing WIT interface definition for TypeScript SDK

### From SECURITY.md Audit
- ASVS 5.0 may still be draft — verify release status
- HMAC bearer token generation/revocation mechanism underspecified
- DAST-in-CI (ZAP) requires full staging deployment — may be flaky
- Missing IAST tool, missing incident response plan, missing responsible disclosure policy
- Ring in [bans.skip] but should be in [bans.deny] if intent is elimination
- Some CVE IDs need NVD link citations

### From Rekindle Audit (15 errors, 16 inconsistencies, 11 improvements)
- Dedup cache (1024 FIFO) may be too small — needs SQLite message_id backstop
- Gossip TTL default not specified
- Genesis bootstrap has privilege escalation path (first writer unchecked)
- Governance subkey overflow handling underspecified
- Slot reclamation mechanism missing (departed members permanently consume slots)
- Voice SFU relay selection grindable by malicious initiator
- Voice SafetySelection::Unsafe contradicts "Full privacy" in transport matrix
- DM layer: Signal Protocol (§8) vs bare ECDH (§27) contradiction
- Cross-doc: Transport::send(to, msg) is point-to-point but Rekindle is pub/sub
- Cross-doc: "zero business logic changes" for Phase 3 is overstated for connector-rekindle
- Cross-doc: Bot SDK exposes slot_seed (allows impersonating any member)
- Cross-doc: springtale-crypto needs HKDF pseudonym derivation for Phase 3
- Cross-doc: Plate gate slot range math conflates logical vs per-record indices
- Effort estimates for CRDT engine and DMs are significantly underestimated

---

## External Validation (verified against live ecosystem data 2026-03-28)

### Dependency Versions — Confirmed Correct
- `secrecy` 0.10 — CONFIRMED (latest is 0.10.3, released 2024-10-09)
- `figment` 0.10 — CONFIRMED (latest is 0.10.19)
- `garde` 0.22 — CONFIRMED (latest is 0.22.0, 1.1M downloads)
- `notify` 7 — OUTDATED: latest stable is 8.2.0 (2025-08-03), 9.0.0-rc.2 in pre-release. **Should update to `"8"`.**
- Rust edition 2024 — CONFIRMED stable since Rust 1.85.0 (2025-02-20)

### Technology Feasibility Risks

**HIGH RISK: rmcp crate version mismatch.**
Our arch specifies `rmcp = { version = "0.2" }`. The official Rust MCP SDK
(github.com/modelcontextprotocol/rust-sdk) is now at 0.16.0+, and API has
changed significantly. The rmcp 0.2 version is stale. Also: rmcp now requires
Rust edition 2024 (nightly was needed for a time, now stable).
**ACTION: Update rmcp version spec to current.**

**MEDIUM RISK: jco componentize is still "experimental."**
Bytecode Alliance released jco 1.0, but `componentize-js` (which bundles
SpiderMonkey into WASM) has been called experimental with potential breaking
changes. Each connector WASM binary includes a JS engine (~3-5MB). This is
feasible but adds significant load time and memory per connector.
**Mitigation:** Documented in arch. Community connectors are the primary
use case; first-party connectors are NativeConnector (Rust, no jco).

**MEDIUM RISK: Veilid maturity for Phase 3.**
VeilidChat is at v0.4.8-0.4.9 with known issues (auto-away broken, no
notification bubbles, Linux build failures). Veilid is under active
development but still in beta. No production apps at scale beyond VeilidChat.
SMPL record performance metrics are not publicly benchmarked.
**Mitigation:** Phase 3 is explicitly gated on "when rekindle-protocol is
production-stable." The architecture correctly doesn't promise a timeline.
But we should add a Veilid maturity assessment to the arch doc.

**LOW RISK: Tauri 2 mobile still maturing.**
Tauri 2 is stable for desktop (released Oct 2024). Mobile (iOS/Android) has
gaps in plugin support and documentation. iOS integration has been reported
as "moderate success." This affects Phase 2b.
**Mitigation:** Phase 2b is later. Mobile ecosystem will mature. Desktop
ships first. Server-pairing mode (phone as thin client) provides a fallback.

**LOW RISK: SolidJS + Tauri 2.**
SolidJS is an officially supported template in create-tauri-app. Multiple
production templates exist (Quantum, tauri-start-solid). No feasibility risk.

### Competitive Landscape Confirmed
- OpenClaw: 250K+ stars, CVEs confirmed in search results, ClawHub malicious skills confirmed via Cisco blog
- NosytLabs: 7 repos confirmed, KickMCP/presearch repos gone, small operation confirmed
- LobeChat: 74K stars, LibreChat: 35K stars — neither has sandboxing or signing
- MCP ecosystem: CVE-2025-6514 confirmed, supply chain breaches confirmed
