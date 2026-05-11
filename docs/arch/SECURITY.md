# Springtale — Security (As-Built)

> Audit of the security posture implemented in the working tree. For
> threat-modelling philosophy, competitive analysis, and compliance
> mappings see `docs/current-arch/SECURITY.md`.

---

## Contents

1. [Vault & Crypto](#1-vault--crypto)
2. [Duress & Panic Wipe](#2-duress--panic-wipe)
3. [SQLite Encryption at Rest](#3-sqlite-encryption-at-rest)
4. [`Secret<T>` Discipline](#4-secrett-discipline)
5. [`unsafe` Audit](#5-unsafe-audit)
6. [WASM Sandbox](#6-wasm-sandbox)
7. [API Authentication & Transport](#7-api-authentication--transport)
8. [TLS Policy](#8-tls-policy)
9. [Manifest Signing](#9-manifest-signing)
10. [Sentinel & Dispatch](#10-sentinel--dispatch)
11. [Release Profile](#11-release-profile)
12. [Dependency Advisories](#12-dependency-advisories)
13. [Compliance Matrix](#13-compliance-matrix)

---

## 1. Vault & Crypto

`crates/springtale-crypto/src/vault/`.

### 1.1 Key derivation

Argon2id with OWASP minimum profile.

| Parameter | Value |
|---|---|
| Memory cost | 64 MiB |
| Iterations | 3 |
| Parallelism | 4 |
| Output length | 32 B (256-bit) |
| Salt | 16 B random, per-vault |

Source: `vault/kdf.rs`. Derived keys are returned in `SecretBox<[u8; 32]>` and pinned into physical RAM via `memsec::mlock` where the OS permits.

```
  passphrase                         random salt (16 B)
      │                                     │
      └──────────┬──────────────────────────┘
                 ▼
       ┌───────────────────┐
       │  Argon2id         │    memory=64 MiB
       │                   │    iterations=3
       │                   │    parallelism=4
       └─────────┬─────────┘
                 ▼
        SecretBox<[u8; 32]>           ─── memsec::mlock     (page pinned)
              │                       └─── madvise DONTDUMP  (no core dumps)
              ▼
        XChaCha20-Poly1305 key
```

*Fig. 2. KDF pipeline. The 32-byte key never leaves `SecretBox` except inside audited `expose_secret()` call sites at the exact AEAD invocation.*

### 1.2 Symmetric encryption

XChaCha20-Poly1305 (authenticated AEAD).

| Parameter | Value |
|---|---|
| Nonce size | 24 B |
| Nonce generation | fresh `OsRng` per save |
| Tag | Poly1305 MAC authenticates salt + nonce + ciphertext |

Key exposure sites are annotated `// SECURITY: expose needed for AEAD {encrypt,decrypt}`. See §4 for the full count.

### 1.3 On-disk format

```
Single region (legacy):

  0           16          40                       end
  ┌───────────┬───────────┬─────────────────────────┐
  │  salt 16  │ nonce 24  │ ciphertext (AEAD body)  │
  └───────────┴───────────┴─────────────────────────┘

Dual region (duress):

  0                40                        65576                      131152
  ┌────────────────┬────────────────────────┬────────────────┬────────────────────────┐
  │  header_A 40   │   ct_A 65536 bytes     │  header_B 40   │   ct_B 65536 bytes     │
  └────────────────┴────────────────────────┴────────────────┴────────────────────────┘
   ▲ salt + nonce                            ▲ salt + nonce
   ▲ real passphrase decrypts this           ▲ duress passphrase decrypts this
   ▲ writing this never touches the other region

  Total file size: 131,152 bytes — constant regardless of contents.
```

*Fig. 1. Vault file layout. No magic bytes, no headers — statistically indistinguishable from random without the passphrase.*

---

## 2. Duress & Panic Wipe

### 2.1 Duress vault (`vault/duress.rs`)

Two AEAD-encrypted regions, each 64 KiB + 40 B header. The **inactive** region is preserved byte-for-byte on every save — writing the real vault does not touch the decoy region and vice versa.

| Passphrase | Region | Contents |
|---|---|---|
| Real | 0 | Full identity, connectors, rules, memory |
| Duress | 1 | Decoy profile — minimal config, no history |

Tests verify constant file size, asymmetric isolation between regions, and indistinguishability from random without the correct passphrase.

### 2.2 Panic wipe (`vault/store/` + `backend/wipe.rs`)

Single-pass random overwrite in 4 KiB chunks, `fsync`, unlink. Completes in under 3 seconds on a 1 MB vault. Ephemeral vaults skip file I/O.

**Limitation:** on SSDs with wear levelling, overwriting does not guarantee erasure of residual ciphertext in the flash translation layer. Panic wipe destroys the **key material**, which is sufficient to make any surviving ciphertext unreadable; full-disk encryption is the only way to physically remove the bytes. This is documented in `docs/guide/security.md §6`.

---

## 3. SQLite Encryption at Rest

The database is encrypted with SQLite3MultipleCiphers (sqlite3mc) — the same full-database encryption approach as Signal/SQLCipher, but with pure-C ChaCha20 and no OpenSSL.

### 3.1 Patch mechanism

`Cargo.toml` patches `libsqlite3-sys` via a local crate at `crates/libsqlite3-sys-mc/` wrapping the sqlite3mc amalgamation. Every `rusqlite` opener in the workspace transparently gains cipher support.

### 3.2 Cipher + key

- Cipher: `chacha20` via sqlite3mc.
- Key: 32 raw bytes, hex-encoded, supplied as `PRAGMA key = "x'{hex}'"` before schema apply or WAL setup (`crates/springtale-store/src/backend/sqlite/mod.rs`).
- Derivation (`apps/springtaled/src/runtime/boot/crypto.rs`):

  ```
  db_key = HMAC-SHA256(key=b"springtale-db-encryption-v1", msg=passphrase)
  ```

  The context string differs from the API token derivation, so the two keys are independent.

- The derived hex key flows through the runtime as `RuntimeStoreConfig.encryption_key_hex`. It is **not** a user-facing config field; it is populated at boot from the vault passphrase.

### 3.3 Ephemeral mode

When `ephemeral = true`, the store is in-memory SQLite with no file and no key. All state is lost on exit.

---

## 4. `Secret<T>` Discipline

Springtale uses `secrecy::SecretBox<T>`. `SecretBox` compile-time forbids `Debug`, `Display`, `Clone`, and `Serialize`, and zeroizes on drop.

### 4.1 Audit

| Crate / module | `expose_secret()` sites | Notes |
|---|---|---|
| `springtale-crypto::vault::kdf` | 0 | Keys wrapped in `SecretBox`, never unwrapped |
| `springtale-crypto::vault::store` | 3 | AEAD encrypt / decrypt / backup |
| `springtale-crypto::vault::duress` | 3 | Per-region AEAD |
| `springtale-crypto::mlock` | 2 | Pointer for `memsec::mlock` / `munlock` |
| `springtale-ai::anthropic` | 1 | HTTP `x-api-key` header |
| `springtale-ai::openai` | 1 | HTTP `Authorization` header |
| `springtale-ai::voice::tts` | 1 | ElevenLabs `xi-api-key` header |
| Connectors | varies | HTTP header + webhook signature verification |

Convention: `// SECURITY: expose needed for X`. Every site carries a justification. No `Secret<T>` value reaches an API response, log line, or error message.

### 4.2 Type-level guarantees

- Config structs derive `Deserialize` only — never `Serialize`. Prevents secrets being round-tripped through config dumps.
- `AiRequest` is a closed enum with concrete `String` fields. Secrets cannot be statically typed into an AI request.
- `#[deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` on all library crates blocks `unwrap` on `Option<Secret>`.

---

## 5. `unsafe` Audit

### 5.1 Crate status

| Crate | Attribute |
|---|---|
| `springtale-core` | `#![forbid(unsafe_code)]` |
| `springtale-transport` | `#![forbid(unsafe_code)]` |
| `springtale-scheduler` | `#![forbid(unsafe_code)]` |
| `springtale-store` | `#![forbid(unsafe_code)]` |
| `springtale-ai` | `#![forbid(unsafe_code)]` |
| `springtale-mcp` | `#![forbid(unsafe_code)]` |
| `springtale-bot` | `#![forbid(unsafe_code)]` |
| `springtale-sentinel` | `#![forbid(unsafe_code)]` |
| `springtale-runtime` | `#![forbid(unsafe_code)]` |
| `springtale-crypto` | `#![deny(unsafe_code)]` (3 audited blocks in `mlock.rs`) |
| `springtale-connector` | `#![deny(unsafe_code)]` |

### 5.2 Audited unsafe blocks

`crates/springtale-crypto/src/mlock.rs` contains three audited `unsafe` blocks, all operating on a 32-byte heap allocation owned by a `SecretBox<[u8; 32]>`:

1. `memsec::mlock(ptr, 32)` — pin the page into physical RAM, preventing swap.
2. `libc::madvise(ptr, 32, MADV_DONTDUMP)` — exclude the page from Linux core dumps.
3. `memsec::munlock(ptr, 32)` — release the page lock before drop.

Safety invariant on all three: `ptr` is a stable heap address owned by `SecretBox`, never reallocated, `len == 32`. The FFI calls are the minimum required to request the relevant kernel hints. Every block carries a `// SAFETY:` comment explaining the invariant. No other `unsafe` appears in the workspace.

---

## 6. WASM Sandbox

`crates/springtale-connector/src/wasm/`. Built on Wasmtime with the Component Model.

### 6.1 Engine config (shared across all invocations)

`wasm/runtime.rs`:

| Setting | Value |
|---|---|
| `consume_fuel` | `true` |
| `epoch_interruption` | `true` |
| `cranelift_opt_level` | `OptLevel::Speed` |
| Component Model | enabled (WASI P2) |

An eternal tokio task increments the engine epoch every 1 s from `springtale-runtime/src/init.rs`, driving the wall-clock timeout.

### 6.2 Per-invocation limits

Fresh `Store` per `execute()` call (`wasm/connector.rs`). Limits enforced via `StoreLimitsBuilder`:

| Limit | Default |
|---|---|
| Memory | 64 MB (1024 pages) |
| Fuel | 10,000,000 instructions |
| Epoch deadline | 30 s wall clock |
| Instance cap | 10 |
| Table cap | 10 |
| Memory cap | 2 |

Each invocation gets a fresh `Store` — no cross-call state leakage.

### 6.3 WASM binary integrity

Before module load (`wasm/connector.rs`), the host computes SHA-256 over the wasm bytes and compares against `manifest.wasm_hash`. Mismatch aborts loading.

### 6.4 Host function gating

All host functions gate through `CapabilityChecker::check()` before performing any real work. Currently:

| Function | Gate |
|---|---|
| `http_request(url, method)` | `NetworkOutbound { host: parsed(url).host }` |

Return codes: `0` = allowed, `-1` = invalid input, `-2` = denied by capability.

---

## 7. API Authentication & Transport

`apps/springtaled/src/api/`.

### 7.1 Token scheme

```
passphrase  ──HMAC-SHA256(key=passphrase, msg="springtale-api-token")──▶  token[32B]
                                                                              │
                                                                              ▼
Request:  Authorization: Bearer <hex(token)>                              hex token
```

- 32 B token, hex-encoded.
- Comparison uses `subtle::ConstantTimeEq` — timing-attack resistant.
- **SSE fallback**: `EventSource` cannot set custom headers, so `/events/stream` and `/canvas/stream` accept `?token=...`. Safe because the daemon binds `127.0.0.1:8080` by default.
- There is no separate API key. Rotating the API token requires rotating the vault passphrase.

### 7.2 Middleware stack

In outside-in composition order (`api/mod.rs`):

```
   incoming request
          │
          ▼
  ┌──────────────────────────────────────────────────────┐
  │ 1. TraceLayer                 HTTP trace span        │
  ├──────────────────────────────────────────────────────┤
  │ 2. SetResponseHeaderLayer ×5  security headers §7.3  │
  ├──────────────────────────────────────────────────────┤
  │ 3. RequestBodyLimitLayer      1 MiB                  │
  ├──────────────────────────────────────────────────────┤
  │ 4. HandleErrorLayer           rate-limit err → 429   │
  ├──────────────────────────────────────────────────────┤
  │ 5. BufferLayer (256)          fronting rate limiter  │
  ├──────────────────────────────────────────────────────┤
  │ 6. RateLimitLayer             100 req/s default      │
  ├──────────────────────────────────────────────────────┤
  │ 7. TimeoutLayer               30 s → 503             │
  ├──────────────────────────────────────────────────────┤
  │    require_auth               Bearer / ?token=       │
  │    ValidatedPath              segments ≤ 256 bytes   │
  └──────────────────────┬───────────────────────────────┘
                         ▼
                      handler
```

*Fig. 3. Middleware stack. Applied to every route; SSE endpoints also honour `?token=` as an auth fallback.*

Webhook routes (`/webhook/{connector}/{trigger}`) go through the same `require_auth` layer as every other authenticated endpoint. The per-connector `Connector::verify_webhook()` method additionally checks a per-sender signature (HMAC-SHA256, RSA, etc.) on the body.

### 7.3 Response headers

Five headers are set on every response:

| Header | Value |
|---|---|
| `X-Frame-Options` | `DENY` |
| `Content-Security-Policy` | `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; frame-ancestors 'none'` |
| `X-Content-Type-Options` | `nosniff` |
| `Referrer-Policy` | `no-referrer` |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=(), accelerometer=(), gyroscope=()` |

**HSTS is deliberately omitted.** RFC 6797 §8.1 forbids sending `Strict-Transport-Security` over plaintext HTTP, and the daemon defaults to `127.0.0.1` without TLS. An operator terminating TLS in front of the daemon (reverse proxy, mesh sidecar) should set HSTS at that layer.

### 7.4 Bind address

- Default: `127.0.0.1:8080` (loopback only).
- `0.0.0.0` binds emit a warning at boot.
- CSP `connect-src` restricts the dashboard to `'self'` and `http://127.0.0.1:*`.

---

## 8. TLS Policy

`rustls` exclusively. `native-tls` is banned through a vendor stub at `vendor/native-tls-stub/` wired via `[patch.crates-io]` in the workspace root. Any transitive pull of `native-tls` fails to compile.

### 8.1 Workspace pins (`Cargo.toml`)

```toml
reqwest       = { version = "0.12", default-features = false,
                  features = ["rustls-tls", "json", "stream"] }
rustls        = "0.23"
tokio-rustls  = "0.26"
rustls-pemfile = "2"
axum-server   = { version = "0.8", features = ["tls-rustls"] }
```

### 8.2 Connector-specific TLS

| Connector | TLS feature |
|---|---|
| `connector-discord` (twilight-gateway, twilight-http) | `rustls-webpki-roots` |
| `connector-slack` (tokio-tungstenite) | `rustls-tls-webpki-roots` |

TLS certificate validation is **never** disabled in any code path.

---

## 9. Manifest Signing

Ed25519 over canonical JSON.

### 9.1 Verification flow (`manifest/verify.rs`)

```
verify_manifest_signature(manifest, author_public_key):
  1. hex-decode manifest.signature (expect 64 B)
  2. strip `signature` field from manifest → canonical JSON
  3. springtale_crypto::signature::verify::verify_canonical_json(
        &canonical_bytes, &signature, &public_key)
  4. on mismatch → ConnectorError::SignatureInvalid
```

Canonical JSON is produced deterministically (sorted keys, no whitespace), so a byte-identical manifest always hashes the same.

### 9.2 Structural validation

- `name`, `version`, `author` must be non-empty.
- `NetworkOutbound.host` must be non-empty.
- `NetworkOutbound.host` may not contain `*` (wildcard rejected).

### 9.3 When verification runs

- On `connector install` (`operations/connectors/install.rs`).
- Unit tests cover: valid signature, tampered manifest, wrong key, missing signature.
- Re-verification on every load is the intended behaviour but not confirmed in the scoped audit — tracked in [`AUDIT-NOTES.md §5`](AUDIT-NOTES.md).

---

## 10. Sentinel & Dispatch

`crates/springtale-sentinel/` + `crates/springtale-runtime/src/dispatch.rs`.

### 10.1 Always on

The behavioural monitor is always initialised at runtime. If no config is provided, it runs with the defaults in §9 of the configuration reference. There is no "disable sentinel" mode.

### 10.2 Dispatch integration

Every action flows through `dispatch_action(&action, &registry, &sentinel)`. The first step of the inner dispatch function calls `sentinel.evaluate(action, connector_name)`, which returns one of:

```
               Action (from rule match or bot handler)
                          │
                          ▼
                ┌───────────────────┐
                │ sentinel.evaluate │
                └─────────┬─────────┘
                          │
         ┌────────────────┼────────────────┬────────────────┐
         ▼                ▼                ▼                ▼
       Go          Throttle(d)       Pause(reason)  Quarantine(reason)
         │                │                │                │
         ▼                ▼                ▼                ▼
     per-action       sleep d,         dispatch         dispatch
      branch         then retry        error +          error +
         │                                 audit row       audit row
         ▼                                                    │
   RunConnector → registry → CapabilityChecker                │
   WriteFile   → path validation, ≤10 MiB                     │
   RunShell    → logged, deferred to approval flow            │
   Chain       → recursive dispatch, depth ≤15                │
   …                                                          │
         │                                                    │
         └──────────────┬─────────────────────────────────────┘
                        ▼
                sentinel.report(action, outcome)
```

*Fig. 4. Dispatch flow. The sentinel gate runs before the per-action branch and again on completion to record the outcome.*

| Verdict | Effect |
|---|---|
| `Go` | Proceed to the per-action branch |
| `Throttle(duration)` | Delay then retry |
| `Pause(reason)` | Return a dispatch error |
| `Quarantine(reason)` | Return a dispatch error and write an audit row |

Sentinel checks run in this order per action:

1. Circuit breaker (per-connector failure threshold)
2. Rate limiter (per-connector actions/minute)
3. Dead-man switch (global action count without user interaction)
4. Destructive action gate (`RunShell` and `WriteFile` to new paths)

After the action completes, `sentinel.report(action, outcome)` records the result.

### 10.3 Toxic pairs

`Sentinel::check_toxic_pairs()` runs at **manifest install time**, not dispatch time. It rejects capability combinations that are safe individually but dangerous in combination:

- `KeychainRead` + `NetworkOutbound` → credential exfiltration
- `FilesystemRead` + `NetworkOutbound` → file exfiltration
- `ShellExec` + `NetworkOutbound` → command execution + exfiltration
- `FilesystemWrite` + `ShellExec` → write-then-execute
- `BrowserNavigate` + `KeychainRead` → credential theft via browser

No override. If a manifest declares a toxic pair, the install fails.

### 10.4 Audit trail

Every sentinel verdict is written to the `audit_trail` table (`schema/sql/audit.sql`) with timestamp, connector, action summary, verdict, and reason. Append-only. Retention is governed by `[sentinel] audit_retention_days` (default 90 days) and a background purge task.

---

## 11. Release Profile

`Cargo.toml`:

```toml
[profile.release]
overflow-checks = true
```

Disables silent integer wrapping in release builds. Critical for fuel metering, size limits, bounded queries, and retention windows — all places where a silent overflow would be a resource-accounting exploit. CPU cost is negligible.

---

## 12. Dependency Advisories

### 12.1 `rand` 0.8 pin (RUSTSEC-2026-0097)

`rand` is pinned at 0.8 with `default-features = false` in the workspace root. Only `std`, `std_rng`, and `getrandom` are enabled.

The unsoundness in RUSTSEC-2026-0097 requires `rand`'s `log` feature. With `default-features = false`, that feature is not compiled into the binary, so the UB code path is absent.

The workspace cannot upgrade to `rand` 0.9 until RustCrypto ships stable releases of `ed25519-dalek` 3.x and `chacha20poly1305` 0.11.x against `rand_core` 0.9.

### 12.2 `.cargo/audit.toml` exceptions

Five RUSTSEC IDs are explicitly ignored with written rationale:

| ID | Reason |
|---|---|
| RUSTSEC-2023-0071 | RSA Marvin attack. Signature **verification** only, never decryption. Public key only. Not exploitable. |
| RUSTSEC-2023-0089 | `atomic-polyfill` unmaintained. Transitive via `garde → phonenumber → postcard → heapless`. No-op on x86_64/aarch64. |
| RUSTSEC-2025-0119 | `number_prefix` unmaintained. CLI table formatting only, cosmetic. |
| RUSTSEC-2025-0134 | `rustls-pemfile` unmaintained. Merged into `rustls` core; still used for boot-time cert loading. |
| RUSTSEC-2026-0097 | `rand` 0.8.5. Mitigated by `default-features = false` — see §12.1. |

### 12.3 `deny.toml`

Bans `native-tls`, `openssl`, `openssl-sys` at compile time. License allow-list: MIT, Apache-2.0, BSD-2/3, ISC, Unicode.

### 12.4 `.gitleaks.toml`

Scans commits for credentials. Allowlist: stopwords (`"abc123"`, `"XChaCha20-Poly1305"`) and `docs/` paths. Custom rule for the Discord token example in `connector-discord` config.

---

## 13. Compliance Matrix

Claims from `.claude/rules/backend/security.md` matched against code:

| Claim | Observed |
|---|---|
| 9 crates `forbid(unsafe_code)` | core, transport, scheduler, store, ai, mcp, bot, sentinel, runtime |
| 2 crates `deny(unsafe_code)` | crypto (3 audited blocks in `mlock.rs`), connector |
| Ed25519 manifest signing | `manifest/verify.rs` via `springtale_crypto::signature` |
| 10 M fuel per WASM invocation | `wasm/connector.rs` per-invocation store |
| 64 MB memory per WASM instance | `StoreLimitsBuilder`, 1024 pages |
| 30 s wall-clock timeout | engine epoch interruption + 1 s ticker |
| Exact-match `NetworkOutbound` hosts | `manifest/verify.rs` rejects wildcards |
| `ShellExec` holds pending user approval | `capability/grant.rs` interactive policy |
| Argon2id with 3 iter, 64 MiB, 4 parallelism | `vault/kdf.rs` |
| XChaCha20-Poly1305 vault encryption | `vault/store/`, `vault/duress.rs` |
| HMAC-SHA256 bearer tokens + constant-time compare | `api/auth.rs` via `subtle` |
| Default bind `127.0.0.1` | `ApiConfig::default` |
| `tower-http` rate limiting | 100 req/s via `RateLimitLayer` |
| `rustls` exclusively, `native-tls` banned via patch | `vendor/native-tls-stub` compile-error shim |
| Every `expose_secret` site annotated | `// SECURITY: expose needed for X` |
| Duress passphrase yields constant file size | 131,152 bytes, verified by `duress.rs` tests |
| Panic wipe completes in < 3 s on 1 MB target | `wipe.rs` single-pass random overwrite |
| SQLite encryption at rest | sqlite3mc ChaCha20 via `libsqlite3-sys-mc` patch |
| `overflow-checks = true` in release profile | `Cargo.toml` |
| 5 response security headers | X-Frame-Options, CSP, X-Content-Type-Options, Referrer-Policy, Permissions-Policy |
| Sentinel evaluates every action | `dispatch.rs` calls `sentinel.evaluate` before the per-action branch |

### Open items

See [`AUDIT-NOTES.md`](AUDIT-NOTES.md) for tracked gaps (manifest re-verification on restart, partial cooperation-module wiring, OpenAI streaming, job queue persistence).

No critical or high-severity gaps identified.
