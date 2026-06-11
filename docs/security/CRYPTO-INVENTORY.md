# Cryptographic Algorithm Inventory

Published per [NSM-10][nsm10] (mandating an inventory of all asymmetric
cryptography in active use), [NIST IR 8547][nist-ir8547] (PQC transition
deadlines), and [CNSA 2.0][cnsa-2] (signing-algorithm procurement deadline).

Reviewed quarterly. Last review: 2026-05-13.

[nsm10]: https://www.whitehouse.gov/briefing-room/presidential-actions/2022/05/04/national-security-memorandum-on-promoting-united-states-leadership-in-quantum-computing-while-mitigating-risks-to-vulnerable-cryptographic-systems/
[nist-ir8547]: https://nvlpubs.nist.gov/nistpubs/ir/2024/NIST.IR.8547.ipd.pdf
[cnsa-2]: https://media.defense.gov/2025/May/30/2003728741/-1/-1/0/CSA_CNSA_2.0_ALGORITHMS.PDF

## Active algorithms

| Algorithm | Crate | Use | PQ status | Replacement | Deadline |
|---|---|---|---|---|---|
| XChaCha20-Poly1305 | `chacha20poly1305` | Vault AEAD; record encryption | PQ-safe (symmetric, 256-bit key) | none | n/a |
| ChaCha20-Poly1305 | `chacha20poly1305` | Transport AEAD (where used) | PQ-safe | none | n/a |
| Argon2id | `argon2` | Vault passphrase KDF (m=64 MiB, t=3, p=4) | PQ-safe (memory-hard, symmetric) | none | n/a |
| SHA-256 | `sha2` | Integrity hashes, HMAC | PQ-safe (NIST IR 8547 §3.2 — Grover halves bits, 128-bit residual is fine) | optional upgrade to SHA-384 for long-lived audit log entries | 2028 evaluate |
| HMAC-SHA256 | `hmac` + `sha2` | API bearer tokens, webhook signature verification | PQ-safe | none | n/a |
| Ed25519 | `ed25519-dalek` | Connector manifest signing; release signing (current) | **DEPRECATED after 2030, DISALLOWED after 2035** per NIST IR 8547 §4 | Hybrid `Ed25519+ML-DSA-65`; pure `ML-DSA-65` or `SLH-DSA` long-term. CNSA 2.0 prefers LMS/XMSS for code signing. | 2027 manifest schema, 2029 default-on, 2030 mandatory |
| X25519 | via `rustls` 0.23 | TLS 1.3 key exchange (mgmt API, connector outbound, Veilid future) | **DEPRECATED after 2030, DISALLOWED after 2035** | Hybrid `X25519MLKEM768` via `rustls-post-quantum` 0.2 (TLS draft-ietf-tls-ecdhe-mlkem) | 2026 Q4 enable by default |

## Crypto-agility design rules

Every signing and key-exchange interface in Springtale must carry an algorithm
identifier so a future migration is purely additive, never a forklift:

1. Connector manifests carry an explicit `alg` field. Default `"ed25519"`.
   Verifier dispatches on the field; refuses unknown values.
2. Vault file format carries an `alg` field for both KDF and AEAD.
3. Release-signing artifacts (cosign bundles) record the signing algorithm in
   the `.intoto.jsonl` predicate.
4. The `springtale-crypto::Signer` trait is object-safe; multiple impls live
   in `springtale-crypto::signers::*`.

## Hybrid-PQ rollout plan

| Phase | Status | Action |
|---|---|---|
| 2026 Q2 | DONE | Inventory published (this document). |
| 2026 Q2 | DONE | Hybrid X25519+ML-KEM-768 KEX on TLS 1.3 activated via `rustls_post_quantum::provider().install_default()` at every binary entry-point (`springtaled`, `springtale-cli`, `tauri-desktop`). Process-global install propagates to every `reqwest::Client` in the workspace — including all 15 connector clients — without per-builder code change. Delivered ahead of the 2026 Q4 target. |
| 2026 Q2 | DONE | `springtale_crypto::signature::SignatureAlgorithm` + `ConnectorManifest.signature_alg` field landed; verifier dispatches on the tag. Extension point for the 2027 Q1 hybrid signer. |
| 2026 Q2 | DONE | `springtale_crypto::vault::VaultPlaintext` envelope (AEAD + KDF + Argon2 params authenticated by AEAD tag). Vault format is crypto-agile end-to-end; both single-region and dual-vault paths use the envelope. |
| 2026 Q2 | DONE | CBOM emitted at `cbom/springtale.cbom.cdx.json` (CycloneDX 1.6, 7 cryptographic-asset components). Validated by `sbom.yml` `cbom` job. |
| 2027 Q1 | PLANNED | `springtale-crypto::signers::ed25519_mldsa65` hybrid signer behind feature flag. Manifest schema accepts `alg = "ed25519_mldsa65"`. |
| 2027 Q3 | PLANNED | Release signing migrates to LMS or hybrid; cosign-bundle predicate records algorithm. |
| 2029 Q1 | PLANNED | Hybrid path becomes default for new manifests. Pure-Ed25519 marked deprecated in signer factory. |
| 2030 Q1 | MANDATORY | NIST IR 8547 deprecation deadline. All new artifacts hybrid or pure-PQ. |
| 2034 Q4 | PLANNED | Remove pure-Ed25519 verify path. |
| 2035 Q1 | MANDATORY | NIST IR 8547 "disallowed" deadline. Pure-Ed25519 / pure-X25519 fully removed. |

## Banned / never-shipped algorithms

Enforced via `deny.toml` `[bans] deny` list:

- `rsa` crate — affected by [RUSTSEC-2023-0071][rustsec-rsa] (Marvin Attack
  timing side-channel) with no upstream fix.
- `md-5`, `sha-1` crates — broken hash functions.
- `rust-crypto` crate — abandoned, unaudited.
- `native-tls`, `openssl-sys`, `openssl` — Springtale uses `rustls`
  exclusively. A `[patch]` stub in workspace `Cargo.toml` makes `native-tls`
  fail-to-compile if anything tries to pull it in.

[rustsec-rsa]: https://rustsec.org/advisories/RUSTSEC-2023-0071

## CBOM emission

CycloneDX 1.6+ supports Cryptographic Bill of Materials. CI job `sbom.yml`
emits a CBOM stanza per algorithm above so downstream consumers can audit
algorithm posture without reading this file.

## Review cadence

Quarterly. Triggers for ad-hoc review:

- New algorithm added to any new dep.
- Upstream advisory against any algorithm we use.
- NIST publishes any draft / final SP that changes deadlines.
- Any HRR (hardware-resistant) implementation lands for any algorithm we use.
