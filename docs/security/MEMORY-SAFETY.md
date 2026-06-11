# Memory Safety Roadmap

Published per CISA "[The Case for Memory Safe Roadmaps][cisa-msr]" (Dec 2023),
the ONCD "[Back to the Building Blocks][oncd-bbb]" report (Feb 2024), and the
NSA/CISA CSI "[Memory Safe Languages: Reducing Vulnerabilities][csi-msl]"
(Jun 2025).

CISA expectation as of 2026-01-01: software manufacturers publish a memory
safety roadmap. This is ours.

[cisa-msr]: https://www.cisa.gov/case-memory-safe-roadmaps
[oncd-bbb]: https://www.whitehouse.gov/wp-content/uploads/2024/02/Final-ONCD-Technical-Report.pdf
[csi-msl]: https://media.defense.gov/2025/Jun/23/2003742198/-1/-1/0/CSI_MEMORY_SAFE_LANGUAGES_REDUCING_VULNERABILITIES_IN_MODERN_SOFTWARE_DEVELOPMENT.PDF

## Posture as of 2026-05-13

Springtale is 100% Rust at the application layer. The Tauri webview renders
SolidJS, which runs in WebKit's memory-safe JavaScript engine. Python bindings
exist via `pyo3` (Rust ↔ CPython) for the cooperation crate only.

There is no C, C++, Go, or other memory-unsafe code authored as part of
Springtale. The unsafe blocks we ship are limited, audited, and listed below.

## Unsafe inventory

Every crate carries one of:

- `#![forbid(unsafe_code)]` — unsafe physically cannot be written.
- `#![deny(unsafe_code)]` — unsafe is a hard error unless individually
  `#[allow(unsafe_code)]`-overridden with a `// SAFETY:` comment.

| Crate | Attribute | Unsafe blocks |
|---|---|---|
| `springtale-core` | `forbid(unsafe_code)` | 0 |
| `springtale-crypto` | `deny(unsafe_code)` | 3 (see below) |
| `springtale-transport` | `forbid(unsafe_code)` | 0 |
| `springtale-connector` | `deny(unsafe_code)` | 0 |
| `springtale-scheduler` | `forbid(unsafe_code)` | 0 |
| `springtale-store` | `forbid(unsafe_code)` | 0 |
| `springtale-ai` | `forbid(unsafe_code)` | 0 |
| `springtale-mcp` | `forbid(unsafe_code)` | 0 |
| `springtale-cooperation` | `forbid(unsafe_code)` | 0 |
| `springtale-runtime` | `forbid(unsafe_code)` | 0 |
| `springtale-bot` | `forbid(unsafe_code)` | 0 |
| `springtale-sentinel` | `forbid(unsafe_code)` | 0 |
| `springtale-wit` | `forbid(unsafe_code)` | 0 |
| `springtale-py` | `forbid(unsafe_code)` | 0 |
| `connectors/connector-*` | `forbid(unsafe_code)` (each) | 0 |
| `apps/springtaled` | `forbid(unsafe_code)` | 0 |
| `apps/springtale-cli` | `forbid(unsafe_code)` | 0 |

### The three unsafe blocks

All three live in `crates/springtale-crypto/src/mlock.rs` and exist to call
into `memsec` (cross-platform `mlock`/`munlock`/`VirtualLock`) and `libc`
(`madvise(MADV_DONTDUMP)`) to pin key material into physical RAM and exclude
it from core dumps. Each block carries a `// SAFETY:` comment documenting
the invariant — the pointer comes from a stable `SecretBox<[u8; 32]>` heap
allocation.

The motivation is forensic: an adversary who seizes a device should not be
able to recover key material from swap pages or crash dumps.

## C/C++ dependencies (transitive)

| Crate | Origin | Use | Mitigation |
|---|---|---|---|
| `libsqlite3-sys` (via `rusqlite` 0.39) | SQLite (C) | Vault storage, audit log | Bundled, version-pinned. WAL mode, `foreign_keys=ON`, `secure_delete=ON`. Patched fork `libsqlite3-sys-mc` to track CVE timelines independent of upstream. |
| `ring` / `aws-lc-rs` (via `rustls`) | BoringSSL (C) | TLS primitives | Reviewed upstream; access only through `rustls` which validates inputs. We do not link these directly. |
| `memsec` | libc `mlock` / Windows `VirtualLock` | Key memory pinning | 3-line wrapper. Reviewed. |
| `libc` | OS libc | `madvise(MADV_DONTDUMP)` only | Single syscall, no parsing path. |
| `pyo3` + CPython | CPython runtime | Python bindings for cooperation crate | **pyo3 is not a sandbox** ([rust-users thread][pyo3-not-sandbox]). Python code via pyo3 runs with daemon trust. Documented in `docs/security/RISK-REGISTER.md`. We do not load untrusted Python; untrusted plugins go through the Wasmtime sandbox. |

[pyo3-not-sandbox]: https://users.rust-lang.org/t/secure-arbitrary-code-execution-pyo3/106126

## WASM connector sandbox

Community-authored connectors run inside [Wasmtime][wasmtime-security] with
strict bounds:

- 10M instruction fuel limit per invocation
- 64MB memory cap (1024 pages)
- 30s wall-clock timeout
- Capability allow-list with exact host matching for `NetworkOutbound`
- Manifest must be Ed25519-signed; signature verified on every load

[wasmtime-security]: https://docs.wasmtime.dev/security.html

## Roadmap

| Quarter | Action |
|---|---|
| 2026 Q3 | `cargo-geiger` gate in CI: workspace unsafe count frozen at current baseline; any increase fails the build. |
| 2026 Q3 | Promote `springtale-crypto` and `springtale-connector` from `deny(unsafe_code)` to `forbid(unsafe_code)` if all unsafe can be encapsulated behind `#[allow]` at function granularity (currently the case). |
| 2026 Q4 | Audit `libsqlite3-sys-mc` fork for divergence; document patch policy. |
| 2027 Q2 | Evaluate pure-Rust SQLite alternatives (`rusqlite-bundled` alternatives, `limbo`). |
| 2027 Q4 | Track sandboxed-Python options for user plugins (RustPython, Wasmtime-hosted CPython). |
| Ongoing | Pyo3 boundary: never accept untrusted Python; only first-party cooperation API users. |

## Verification

```
cargo geiger --workspace --output-format Json
```

Emits one row per crate with unsafe expression counts. Run by CI in `sca.yml`
nightly; report uploaded as a workflow artifact.

## Fuzzing

Five `cargo-fuzz` targets under `fuzz/fuzz_targets/`, run nightly by
`fuzz.yml`, each aimed at a parser or boundary that consumes untrusted
bytes:

| Target | Surface |
|---|---|
| `fuzz_manifest_parse` | Connector manifest TOML parsing |
| `fuzz_capability_parse` | Capability declaration parsing |
| `fuzz_ipc_frame` | IPC frame decoding |
| `fuzz_path_canon` | Filesystem path canonicalisation (allow-list checks) |
| `fuzz_url_allowlist` | URL host allow-list matching |
