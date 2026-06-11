# Supply Chain Risk Management Plan

Published per [NIST SP 800-161r1][nist-800-161] (C-SCRM, SR control family in
SP 800-53), the [CISA Open Source Software Security Roadmap][cisa-oss-rm],
and [OWASP Top 10:2025 A03 Software Supply Chain Failures][owasp-a03].

Reviewed annually; next review 2027-05-13.

[nist-800-161]: https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-161r1-upd1.pdf
[cisa-oss-rm]: https://www.cisa.gov/sites/default/files/2024-02/CISA-Open-Source-Software-Security-Roadmap-508c.pdf
[owasp-a03]: https://owasp.org/Top10/2025/A03_2025-Software_Supply_Chain_Failures/

## Acceptance criteria for new dependencies

A new direct dependency (anything appearing as a top-level entry in any of
our `Cargo.toml`, `package.json`, or `pyproject.toml`) requires:

1. **License compatible** — listed in `deny.toml` `[licenses] allow`. Default
   set: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0,
   Unicode-DFS-2016, Zlib, BSL-1.0, MPL-2.0. Copyleft (GPL/AGPL/LGPL-3) is denied.
2. **No known advisories** — `cargo audit`, `pnpm audit`, `pip-audit` all green.
3. **No KEV match** — daily KEV check (`sca.yml` job `kev-check`) green.
4. **Active maintainer signal** — ≥1 commit in last 12 months OR explicit
   note in `supply-chain/audits.toml` accepting maintenance risk.
5. **OpenSSF Scorecard ≥ 5/10** for any new direct dep (advisory; we may
   accept lower with documented reason).
6. **`cargo vet` audit entry** — added to `supply-chain/audits.toml` either
   by us auditing or by importing from `mozilla`, `google`, or
   `bytecodealliance` audits.
7. **Build-script review** — any crate with a `build.rs` gets read before
   merge. `.npmrc ignore-scripts=true` blocks npm post-install scripts by
   default (see CISA [Shai-Hulud advisory][shai-hulud]).
8. **No new transitive `unsafe`** outside the existing C-binding boundary
   documented in `MEMORY-SAFETY.md`. Validated by `cargo-geiger` delta.

[shai-hulud]: https://www.cisa.gov/news-events/alerts/2025/09/23/widespread-supply-chain-compromise-impacting-npm-ecosystem

Justification lives in the ADR PR that introduces the dep.

## Update policy

| Channel | Cadence | Auto-merge? |
|---|---|---|
| Dependabot security updates | as published | No — maintainer review |
| Dependabot semver-compatible (patch/minor) | weekly | No — maintainer review |
| Dependabot major | as opened | No — usually requires code change |
| GitHub Actions | monthly (Dependabot config) | No — verify new SHA pin |
| Renovate cooldown | n/a (we use Dependabot) | n/a |

Manual rotation list — `dependabot.yml` `ignore` keeps these from auto-PRs
because they touch the wire format / sandbox / store:
- `wasmtime`
- `rusqlite`
- `rustls`
- `chitchat`
- `foca`
- `pyo3`

These get reviewed each release.

## Provenance and signing

Per NIST SP 800-218 PS.2.1 / PS.3.2 and the [CISA Secure-by-Demand
Guide][cisa-sbd]:

[cisa-sbd]: https://www.cisa.gov/sites/default/files/2024-08/SecureByDemandGuide_080624_508c.pdf

- Every release artifact (Tauri bundle, `springtaled`, `springtale-cli`,
  Python wheel) is signed by Sigstore cosign in keyless OIDC mode via the
  `provenance.yml` workflow. Bundle published alongside artifact on GitHub
  Releases.
- SLSA Build Level 3 provenance attestation emitted via
  `slsa-framework/slsa-github-generator`. Published to Sigstore Rekor
  transparency log.
- CycloneDX 1.7 SBOM (Rust + npm + Python merged) + SPDX 2.3 SBOM emitted
  per release. Both signed with cosign.
- Release tags are signed (Sigstore `gitsign` or GPG); see `CONTRIBUTING.md`
  for maintainer setup.

Users verify with:

```
cosign verify-blob \
  --bundle springtaled.cosign.bundle \
  --certificate-identity-regexp 'https://github.com/ScopeCreep-zip/Springtale/.*' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  springtaled
```

Full recipe in `docs/operations/verifying-releases.md`.

## Vendor and ecosystem assessments

| Ecosystem | Audit graph | Continuous monitor |
|---|---|---|
| Rust / crates.io | `cargo vet` with `mozilla`, `google`, `bytecodealliance` imports | `cargo-audit` (RustSec), `cargo-deny` advisories, `osv-scanner` |
| npm / pnpm | none today (no equivalent of vet); use `pnpm audit` + `npm-lockfile-scan` known-bad list | `pnpm audit`, daily KEV cross-ref |
| Python / PyPI (pyo3) | none today; pinned lockfile via uv when `pyproject.toml` lands | `pip-audit --strict` |
| GitHub Actions | explicit allowlist in `docs/security/CI-TRUST.md`; all actions SHA-pinned | `zizmor`, `actionlint`, GitHub Actions org policy (SHA pinning enforced) |
| Container base images | digest-pinned (no tag refs in production layers) | `trivy`, `grype`, `syft` SBOM |

## Withdrawn / excluded dependencies

| Dep | Reason | VEX statement |
|---|---|---|
| `matrix-sdk` (and `connector-matrix`) | `matrix-sdk 0.16` requires `rusqlite 0.37` which has [CVE-2025-70873][cve-rusqlite] (heap info disclosure). Store uses `rusqlite 0.39` (patched). Downgrading would expose vulnerable users to heap leaks. | `vex/connector-matrix-rusqlite-cve-2025-70873.json` |
| `rsa` | [RUSTSEC-2023-0071][rustsec-rsa] Marvin Attack timing sidechannel; no upstream fix. | `vex/rsa-rustsec-2023-0071.json` |

[cve-rusqlite]: https://nvd.nist.gov/vuln/detail/CVE-2025-70873
[rustsec-rsa]: https://rustsec.org/advisories/RUSTSEC-2023-0071

Accepted-with-mitigation entries live in `.cargo/audit.toml` with full
justification.

## VEX statement inventory

Every advisory we accept ships a machine-readable VEX statement under
`vex/`, mirrored by the ignore lists in `.cargo/audit.toml` and
`deny.toml`:

| VEX file | Advisory |
|---|---|
| `atomic-polyfill-rustsec-2023-0089.json` | `atomic-polyfill` unmaintained |
| `bincode-rustsec-2025-0141.json` | `bincode` advisory |
| `connector-matrix-rusqlite-cve-2025-70873.json` | `rusqlite` heap info disclosure (matrix deferral) |
| `instant-rustsec-2024-0384.json` | `instant` unmaintained |
| `lru-rustsec-2026-0002.json` | `lru` advisory |
| `number-prefix-rustsec-2025-0119.json` | `number-prefix` unmaintained |
| `rand-rustsec-2026-0097.json` | `rand 0.8.5` log-feature unsoundness |
| `rsa-rustsec-2023-0071.json` | Marvin Attack timing sidechannel |
| `rustls-pemfile-rustsec-2025-0134.json` | `rustls-pemfile` advisory |

Adding an ignore without a matching VEX file (or vice versa) should fail
review — the two lists are maintained together.

## CI secrets the pipeline needs

| Secret | Used by | Notes |
|---|---|---|
| `GITLEAKS_LICENSE` | `secrets.yml` (gitleaks job) | Required for org-owned repos per gitleaks-action licensing; the job fails without it |

## Reproducible builds

**What we attest:** every release artifact is produced by `cargo
auditable build --release --locked` from a fixed Git commit under a
fixed Rust toolchain. The same `(commit, toolchain)` pair, rebuilt
on a clean Ubuntu runner with the release commit's `SOURCE_DATE_EPOCH`
and `CARGO_INCREMENTAL=0`, produces byte-identical artifacts. The
`provenance.yml` `repro-check` job enforces this on every release: it
rebuilds `springtaled` and `springtale-cli` on a fresh runner, hashes
both copies, and fails the workflow if the SHA-256 differs. A
GitHub-native build-provenance attestation is then emitted for the
cross-runner-verified hashes (`actions/attest-build-provenance@v2`),
which a verifier can fetch via `gh attestation verify` to prove the
attestation came from a same-toolchain, same-commit rebuild rather
than from a single ad-hoc compile.

**What we do not attest:** binaries produced on a different rustc
release, with different host LLVM / glibc, or under a different
linker (lld vs gold) are NOT expected to match bit-for-bit. The
attestation predicate records the toolchain via the SLSA generator
metadata, so a verifier knows exactly which input set the
reproducibility claim covers.

**Knobs that enforce this:**

- `Cargo.toml [profile.release]` — `codegen-units = 1`, `lto = "fat"`,
  `panic = "abort"`, `strip = "symbols"`, and crucially
  `incremental = false`. Incremental compilation injects per-machine
  state that breaks bit-for-bit reproducibility.
- `.github/workflows/provenance.yml` — `SOURCE_DATE_EPOCH` from the
  release tag commit timestamp, `CARGO_INCREMENTAL=0` in the build
  env (belt-and-braces against future config drift), separate
  `Swatinem/rust-cache@v2` cache keys for `release-build` vs
  `repro-check` so a stale incremental cache can't fake a match.

**Verifying a release locally:** clone at the release tag, run
`cargo auditable build --release --locked` with
`SOURCE_DATE_EPOCH=$(git show -s --format=%ct HEAD)` and
`CARGO_INCREMENTAL=0`, then `sha256sum target/release/springtaled`
and compare against the value in the release's
`actions/attest-build-provenance` attestation.

## Incident response (supply chain compromise)

See `docs/security/INCIDENT-RUNBOOK.md` for the maintainer playbook covering:

- Compromised upstream package landing in a Dependabot PR
- Leaked npm/PyPI/crates.io publish token
- Unauthorized release tag
- Malicious GitHub Action

## Annual review

- Re-evaluate license allow-list and ban list
- Re-evaluate Scorecard threshold
- Refresh `cargo vet` imports list against Mozilla/Google/BA churn
- Refresh KEV-cross-ref job (rotate API endpoints if CISA URL pattern changes)
- Refresh manifest signing algorithm against NIST PQC progress
