# Risk Register

Published per [NIST CSF 2.0 GV.RM][csf2-gvrm] and [OWASP Top 10:2025 A06
Insecure Design][owasp-a06]. Cross-referenced from
`docs/current-arch/SECURITY.md` (the as-built threat model) and
`docs/threat-model-faq.md`.

Reviewed each release. Last review: 2026-05-13.

[csf2-gvrm]: https://nvlpubs.nist.gov/nistpubs/CSWP/NIST.CSWP.29.pdf
[owasp-a06]: https://owasp.org/Top10/2025/A06_2025-Insecure_Design/

## Adversary model

Springtale's threat model assumes adversaries up to and including a nation-state
ISP-level network observer plus a well-resourced commercial harasser. The
hardest cases (rising authoritarianism, IPV survivors, deplatforming targets,
forensic device seizure) drive the design defaults.

Out of scope by design (already mitigated by general OS posture or impossible
to defend at this layer): an attacker with kernel-level root on the user's
device.

## Risk inventory

Each row keys to a crate boundary. STRIDE labels: S=Spoofing, T=Tampering,
R=Repudiation, I=Information Disclosure, D=Denial of Service, E=Elevation.

| ID | Boundary | Risk | STRIDE | Mitigation | Owner |
|---|---|---|---|---|---|
| R-001 | `springtale-crypto` vault | Passphrase brute force | S | Argon2id m=64 MiB, t=3, p=4; rate-limited unlock attempts; duress passphrase support | crypto |
| R-002 | `springtale-crypto` vault | Key material in swap or core dump | I | `memsec::mlock` + `madvise(MADV_DONTDUMP)` on Linux | crypto |
| R-003 | `springtale-crypto` vault | Decrypted key persistence past lock | I | `zeroize` on drop; `SecretBox` wrapping; explicit lock op on idle timeout | crypto |
| R-004 | `springtale-connector` manifest | Forged/modified manifest | T | Ed25519 signature; hash re-check on every load; manifest schema `deny_unknown_fields` | connector |
| R-005 | `springtale-connector` capability | Capability over-grant | E | Explicit allow-list per `NetworkOutbound` host; `ShellExec` triggers blocking approval | connector |
| R-006 | `springtale-connector` WASM | Sandbox escape | E | Wasmtime fuel (10M instr) + memory cap (64MB) + wall-clock (30s); `forbid(unsafe_code)` in connector | connector |
| R-007 | `springtale-transport` TLS | Wire eavesdropping | I | `rustls-tls` exclusively; TLS 1.3 only; PQ hybrid X25519+ML-KEM (planned 2026 Q4) | transport |
| R-008 | `springtale-transport` mTLS | Peer impersonation | S | mTLS with pinned CA per formation | transport |
| R-009 | `springtaled` HTTP API | Token forgery | S | HMAC-SHA256 bearer; constant-time compare; bind 127.0.0.1 by default | apps |
| R-010 | `springtaled` HTTP API | Replay | T | Timestamp + nonce in HMAC scope; ≤5min skew | apps |
| R-011 | `springtaled` HTTP API | DoS | D | `tower-http::limit` rate limit; body size cap; per-IP concurrency cap | apps |
| R-012 | Tauri IPC | Untrusted webview content executing IPC | E | Strict CSP; Trusted Types; Isolation Pattern; capability allow-list per command | desktop |
| R-013 | Tauri IPC | OS-shell injection via command argv | E | No `bash -c`; argv-array form only; allow-listed binary paths | desktop |
| R-014 | `springtale-store` SQLite | Vault file disclosure on multi-user host | I | Vault file mode 0600; XDG_DATA_HOME default | store |
| R-015 | `springtale-store` SQLite | SQL injection | T | All queries through `springtale-store`; `sqlx::query_as!` macros + `rusqlite::params!`; CI grep deny of inline SQL `format!` | store |
| R-016 | `springtale-sentinel` audit log | Trail tampering | T | Append-only table; row hash chain; vault-encrypted at rest | sentinel |
| R-017 | `springtale-sentinel` audit log | Pre-auth read | I | Audit log inside encrypted vault store; key not held by API layer | sentinel |
| R-018 | `springtale-ai` adapter | Prompt injection | T | External-context tagging; output schema validation; runtime never trusts model for authorization | ai |
| R-019 | `springtale-ai` adapter | Secret exfiltration via prompt | I | Input redaction pass (`AiGuardrail`); no secrets in system prompt | ai |
| R-020 | `springtale-ai` adapter | Unbounded token consumption | D | Per-bot daily token quota; per-request size cap; 30s wall-clock | ai |
| R-021 | `springtale-ai` adapter | Provider impersonation | S | Pinned host + cert validation; OAuth credentials in vault | ai |
| R-022 | `springtale-py` pyo3 boundary | Untrusted Python execution | E | pyo3 is not a sandbox. Untrusted Python is not loaded. Pyo3 only exposes the cooperation crate. Documented in `MEMORY-SAFETY.md`. | py |
| R-023 | Connector OAuth tokens | Token theft from vault dump | I | Vault encrypted; `Secret<String>` wrapping; `zeroize` on drop | connector |
| R-024 | Webhook receive | Replay / spoof | S/T | HMAC verify; timestamp ≤5min; idempotent processor | connector |
| R-025 | Panic wipe | Incomplete wipe / recoverable artifacts | I | Cryptographic erasure (vault key zeroize); WAL truncation; explicit OS-level file zero-fill | crypto |
| R-026 | Panic wipe | Duress vault detectable as duress | I | Duress vault format identical to main vault; constant-time decryption attempt | crypto |
| R-027 | Onboarding | Default-on telemetry | I | **Zero telemetry, ever.** No path to add; enforced by project rule and CI grep. | runtime |
| R-028 | CI/CD | Compromised GitHub Action | E | Allowlist of actions; SHA-pinned; `harden-runner` egress block; least-privilege `permissions` | infra |
| R-029 | CI/CD | Stolen publish token | T | OIDC keyless signing only; no long-lived publish PATs | infra |
| R-030 | Connector marketplace (future) | Malicious community connector | T/E | Manifest signing required; capability allow-list enforced at runtime; user approval for `ShellExec`; sandbox limits | connector |
| R-031 | Awareness gossip | Cross-formation leakage of hidden bots | I | Formation membership encrypted; gossip carries no plaintext identifiers; tested in `springtale-cooperation` | cooperation |
| R-032 | Veilid P2P (Phase 3) | E2E key compromise | I | Deferred until P3; will use hybrid X25519+ML-KEM | transport |

## Mitigation status

| State | Count |
|---|---|
| Implemented | 24 |
| In flight (2026 Q3) | 4 (R-018 to R-021, AI guardrails) |
| Planned (2026 Q4) | 1 (R-007 PQ hybrid) |
| Deferred (P3) | 1 (R-032 Veilid) |
| Out-of-scope (design intent) | 2 (R-027 telemetry, R-022 untrusted Python) |

## Threat-modeling cadence

- New crate boundary or new connector type → new STRIDE pass before merge.
- Every release → re-walk the register; close obsolete rows, add new ones.
- Security advisory affecting a dep → ad-hoc review of any row it touches.
- Tooling: STRIDE worksheets in `docs/threat-models/` (Threat Dragon JSON or
  pytm Python source). CI rebuilds diagrams on PR.
