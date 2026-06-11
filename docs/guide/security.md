# Security

Springtale is built for people whose safety depends on privacy — trans people, activists, IPV survivors, immigrants, and anyone facing surveillance, doxxing, or harassment. Security isn't a feature you can toggle. It's a constraint that shapes every decision.

For the full threat model, OWASP ASVS mapping, and MITRE ATT&CK mapping, see [`docs/current-arch/SECURITY.md`](../current-arch/SECURITY.md).

## 1. Defense in Depth

Eight independent layers. Compromise of any one doesn't cascade to the others.

```
  ┌─────────────────────────────────────────────────────────────┐
  │                                                             │
  │  ┌───────────────────────────────────────────────────────┐  │
  │  │                  Zero Telemetry                       │  │
  │  │  No analytics. No crash reports. Nothing leaves       │  │
  │  │  your device unless YOU configure an endpoint.        │  │
  │  ├───────────────────────────────────────────────────────┤  │
  │  │               Transport Encryption                    │  │
  │  │  rustls-tls exclusively. native-tls and OpenSSL       │  │
  │  │  banned at compile time via deny.toml + vendor stub.  │  │
  │  ├───────────────────────────────────────────────────────┤  │
  │  │                Vault Encryption                       │  │
  │  │  Secrets encrypted at rest: XChaCha20-Poly1305        │  │
  │  │  with key derived from passphrase via Argon2id.       │  │
  │  │  No plaintext credential files on disk. Ever.         │  │
  │  ├───────────────────────────────────────────────────────┤  │
  │  │             WASM Sandbox (community connectors)       │  │
  │  │  Wasmtime isolation: 10M instruction fuel budget,     │  │
  │  │  64MB memory limit, 30-second wall-clock timeout.     │  │
  │  ├───────────────────────────────────────────────────────┤  │
  │  │            Capability Model + Toxic Pairs             │  │
  │  │  Connectors declare what they need. Exact-host        │  │
  │  │  matching (no wildcards). Dangerous combos blocked.   │  │
  │  ├───────────────────────────────────────────────────────┤  │
  │  │              Manifest Signing (Ed25519)               │  │
  │  │  Verify before load. Verify on every subsequent       │  │
  │  │  load. Tampered manifests rejected instantly.         │  │
  │  ├───────────────────────────────────────────────────────┤  │
  │  │               Secret<T> Type Wrapper                  │  │
  │  │  Credentials wrapped at the type level. Cannot be     │  │
  │  │  logged, cloned, or serialized. Zeroed on drop.       │  │
  │  ├───────────────────────────────────────────────────────┤  │
  │  │              Supply Chain Hardening                    │  │
  │  │  cargo-deny (license + advisory audit),               │  │
  │  │  cargo-audit (RustSec), gitleaks (secrets detection). │  │
  │  └───────────────────────────────────────────────────────┘  │
  │                                                             │
  └─────────────────────────────────────────────────────────────┘
```

*Fig. 1. Defense-in-depth security stack. Each layer operates independently.*

---

## 2. Secrets Never Touch the Ground

Every credential in Springtale — API keys, OAuth tokens, webhook secrets — follows the same lifecycle:

```
  Config file                Secret<T>                  Call site              Drop
  (TOML parse)               (wrapped)                  (use)                  (freed)
       │                        │                          │                      │
       v                        v                          v                      v
  ┌──────────┐           ┌──────────────┐           ┌──────────────┐       ┌───────────┐
  │ Raw      │  parse    │   Secret<    │  .expose  │  Bare value  │  drop │  Memory   │
  │ string   │──────────>│   String>    │──────────>│  (scoped)    │──────>│  zeroed   │
  │ in TOML  │           │              │  _secret()│              │       │  via      │
  └──────────┘           │  - no Debug  │           │  Used ONLY   │       │  zeroize  │
                         │  - no Clone  │           │  at the HTTP │       └───────────┘
                         │  - no Display│           │  call site   │
                         │  - no Serde  │           │              │
                         │    Serialize │           │  // SECURITY:│
                         └──────────────┘           │  expose for X│
                                                    └──────────────┘
```

*Fig. 2. `Secret<T>` lifecycle. The raw value exists only briefly at the precise call site, annotated with a `// SECURITY:` comment explaining why. Memory is zeroed before deallocation.*

What this prevents:
- Secrets appearing in log output (no `Debug` or `Display`)
- Secrets in serialized API responses (no `Serialize`)
- Secrets surviving in freed memory (zeroize on drop)
- Accidental copies (no `Clone`)

Config structs derive `Deserialize` only — never `Serialize`. This is a compile-time guarantee, not a policy.

---

## 3. The Sandbox

Community connectors are untrusted code. They run inside a Wasmtime sandbox with hard resource limits:

```
  ┌─ Host Process (springtaled) ──────────────────────────────────┐
  │                                                                │
  │  ConnectorRegistry    CapabilityChecker    ManifestVerifier    │
  │        │                     │                    │            │
  │        v                     v                    v            │
  │  ┌─────────────────────────────────────────────────────────┐   │
  │  │                  Wasmtime Sandbox                       │   │
  │  │                                                         │   │
  │  │  ┌───────────────────────────────────────────────────┐  │   │
  │  │  │            community-connector.wasm               │  │   │
  │  │  │                                                   │  │   │
  │  │  │  Fuel:     10,000,000 instructions (then killed)  │  │   │
  │  │  │  Memory:   64 MB (1024 WASM pages, then OOM)      │  │   │
  │  │  │  Timeout:  30 seconds wall-clock (then killed)     │  │   │
  │  │  │                                                   │  │   │
  │  │  │  CAN:      call declared host functions only      │  │   │
  │  │  │  CANNOT:   access filesystem, network, or other   │  │   │
  │  │  │            connectors unless capability granted    │  │   │
  │  │  └───────────────────────────────────────────────────┘  │   │
  │  │                                                         │   │
  │  │  Host API: only capabilities declared in manifest       │   │
  │  │  are wired to the WASM module's imports.                │   │
  │  └─────────────────────────────────────────────────────────┘   │
  │                                                                │
  └────────────────────────────────────────────────────────────────┘
```

*Fig. 3. WASM sandbox boundary. The connector module can only reach the host through explicitly granted capability functions. Everything else is an empty import that traps.*

What a malicious WASM connector **cannot** do:
- Read files outside its declared `FilesystemRead` paths
- Make network requests to hosts not in its `NetworkOutbound` list
- Access another connector's data or state
- Execute shell commands without `ShellExec` approval
- Exceed its instruction budget (killed, not paused)
- Allocate more than 64MB (OOM, not swap)

---

## 4. Signing and Trust

Every connector ships with a TOML manifest. The manifest can be signed with Ed25519.

```
  Connector Author                    Springtale Runtime
       │                                     │
       │  1. create manifest.toml            │
       │  2. sign with Ed25519 key           │
       │  3. publish (manifest + signature)  │
       │                                     │
       │              install                │
       ├────────────────────────────────────>│
       │                                     │  4. verify Ed25519 signature
       │                                     │  5. parse capabilities
       │                                     │  6. check for toxic pairs
       │                                     │  7. prompt user for approval
       │                                     │  8. register in ConnectorRegistry
       │                                     │
       │                                     │  ... later, on every load ...
       │                                     │
       │                                     │  9. re-verify signature
       │                                     │  10. re-verify WASM binary hash
       │                                     │
```

*Fig. 4. Manifest signing and verification flow. Verification happens at install AND on every subsequent load — a tampered connector is caught even if the attack happens after installation.*

---

## 5. Toxic Pair Blocking

Some capability combinations are dangerous even if each is individually reasonable:

**TABLE I. BLOCKED CAPABILITY COMBINATIONS**

| Pair | Why it's dangerous |
|------|--------------------|
| `KeychainRead` + `NetworkOutbound` (different host) | Could exfiltrate stored credentials to an attacker's server |
| `FilesystemRead` + `NetworkOutbound` (different host) | Could exfiltrate file contents to an attacker's server |
| `ShellExec` + `NetworkOutbound` | Could execute arbitrary commands and exfiltrate results |
| `BrowserNavigate` + `KeychainRead` | Could steal credentials through a browser context |
| `FilesystemWrite` + `ShellExec` | Could write a script and then execute it |

These are blocked at install time. If a manifest declares a toxic pair, the install is rejected — no override, no "are you sure?" prompt.

### 5.1. ShellExec Always Requires a Human

`ShellExec` is special-cased beyond the toxic-pair table: it can never be
auto-granted under any capability policy. Every invocation parks in a
pending-approval queue (`crates/springtale-runtime/src/approval/`) and the
dispatching connector blocks until you approve or deny it — from the
desktop approval card, the in-app chat panel, or
`POST /approvals/{id}` on the management API. If nothing answers within
the timeout (default 60s), the gate falls back to **deny**: a dropped
connection never silently grants. Every decision lands in the audit
trail. This is the OpenClaw CVE-2026-25253 1-click-RCE class Springtale
exists to defeat.

### 5.2. AI Guardrails (OWASP LLM Top-10)

When a bot has an AI adapter plugged in, the adapter is wrapped in
`GuardrailAdapter` (`crates/springtale-ai/src/guardrail/`), which adds
safeguards orthogonal to the adapter itself:

| Guardrail | OWASP | What it does |
|---|---|---|
| Wall-clock timeout | LLM10 | Every call gets a `tokio` timeout fence on top of the transport timeout — a provider that holds the connection open can't stall a chain forever |
| Output size cap | LLM10 | Truncates oversized responses so a runaway provider can't pipe an unbounded body into the next chain step |
| Refusal-rate metric | LLM07 | Process-local counters (total calls / sanitiser blocks) surfaceable for visibility |
| Daily token quota | LLM10 | Per-bot daily cap via `[sentinel] daily_token_limit`. Unset = observability mode (usage recorded, nothing blocked); set = hard deny once a bot crosses the cap in a UTC day. Counters persist across restarts (`ai_token_usage` table) |
| Tool policy | LLM06 | `[bot] tool_policy` — by default the AI can only call `read_only` actions (zero side effects); mutating actions stay invisible unless `writes_with_approval` is on, which fronts each call with the blocking approval gate. Explicit `allow`/`deny` globs override; `deny` always wins |

The input/output `Sanitizer` (prompt-injection, credential, and PII
patterns) is exercised in CI against a 50-case red-team corpus
(`crates/springtale-ai/tests/redteam_corpus/`) that fails closed on any
case the sanitizer misses.

---

## 6. What's Honestly Out of Scope

No security model is complete. These are known limitations:

- **Flash storage wear leveling** — Even after zeroize, SSDs may retain old data in wear-leveled blocks. Full-disk encryption (LUKS, FileVault, BitLocker) is the real defense here. Springtale's vault encryption is a second layer, not a replacement. The **panic wipe** destroys the key material (making any residual ciphertext unreadable) but cannot physically clear all flash blocks.
- **Full-disk encryption** — Springtale encrypts its own vault and database, but can't encrypt the entire disk. Users in high-risk situations should enable OS-level FDE.
- **Side-channel attacks** — Timing attacks on crypto operations are mitigated (constant-time comparison for HMAC), but hardware-level side channels (cache timing, power analysis) are out of scope for a software project.
- **Physical device access with unlimited time** — Duress and panic features help, but a sufficiently resourced adversary with physical access and time will eventually prevail. The goal is to make it expensive, not impossible.

## 7. Safety Features

The following safety features are present:

| Feature | Where | What it does |
|---|---|---|
| **Duress passphrase** | `springtale vault duress-setup` | Secondary passphrase unlocks a decoy vault. Two AEAD-encrypted regions share one file with constant 131,152-byte size. Writing one region never touches the other. |
| **Panic wipe** | `springtale panic` | Random overwrite → fsync → unlink. Completes in <3 s on a 1 MB vault. Zeros key material in memory before file ops. |
| **Travel mode** | `springtale travel prepare --backup-to` / `travel restore --from` | Encrypt backup to external location, wipe local install, restore at destination. |
| **Ephemeral mode** | `ephemeral = true` in config | All state in memory, lost on exit. No file I/O for vault or database. |
| **Sentinel monitor** | always on | Toxic-pair capability detection at install time, writes to `audit_trail` table. |
| **Destructive-action approval gate** | sentinel `ApprovalGate` (G5b) | Sentinel's fourth check after circuit-breaker / rate-limit / dead-man. Destructive actions (per `impact::classify_impact`) route through an `ApprovalGate` before dispatch. `DefaultDenyApprovalGate` ships as the safe headless default — destructive actions are refused unless an explicit gate is wired. The desktop app wires its own gate that prompts the user. |
| **Disguise tray icon** | `POST /safety/disguise/profile` (G5f) | Tray icon ships in four profiles — `calculator`, `files`, `notes`, `springtale` — under `tauri/apps/desktop/src-tauri/icons/disguise/`. Built once at startup and swapped at runtime based on `SafetyConfig.disguise_icon_id`. The window title is configurable too — useful in environments where an over-the-shoulder observer should not see "Springtale" on a taskbar. |
| **Quick-hide shortcut** | `SafetyConfig.quick_hide_shortcut` (G5g) | OS-wide global hotkey (default `Ctrl+Shift+H`) that hides the window and locks the vault from anywhere on the desktop, not just when Springtale has focus. The shortcut is persisted in safety state and rebound on every app restart. |
| **Panic-tap count** | `POST /safety/panic_tap_count` | Number of taps required on the panic gesture before the wipe fires. Defaults to 3 to prevent accidental wipe; configurable per user threat model. |

---

## References

- [1] Full threat model and compliance mappings: [`docs/current-arch/SECURITY.md`](../current-arch/SECURITY.md)
- [2] Vulnerable user threat models: `docs/current-arch/ARCHITECTURE.md` §2.5-2.9
- [3] Audit findings: [`docs/current-arch/AUDIT-NOTES.md`](../current-arch/AUDIT-NOTES.md)
- [4] `secrecy` crate: `https://docs.rs/secrecy`
- [5] Wasmtime security model: `https://docs.wasmtime.dev/security.html`
