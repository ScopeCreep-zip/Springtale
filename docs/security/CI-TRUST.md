# CI Trust Posture

Documents Springtale's CI/CD trust relationships per [NIST SP 800-204D][nist-204d]
(Strategies for the Integration of Software Supply Chain Security in DevSecOps
CI/CD Pipelines) and the [CISA tj-actions advisory][cisa-tjactions].

Last review: 2026-05-13.

[nist-204d]: https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-204D.pdf
[cisa-tjactions]: https://www.cisa.gov/news-events/alerts/2025/03/18/supply-chain-compromise-third-party-tj-actionschanged-files-cve-2025-30066-and-reviewdogaction

## Principles

1. **Least privilege per job.** Every workflow declares
   `permissions: contents: read` at top level and elevates per-job only where
   needed (e.g. `id-token: write` for OIDC signing jobs).
2. **SHA-pinned actions.** Target posture: every third-party action pinned by
   full commit SHA with a `# vX.Y.Z` trailing comment for human readability.
   **Current state: tag-pinned.** The SHA-pin sweep across all workflows is
   pending; until it lands, zizmor runs with persona `regular` (its `auditor`
   persona requires SHA pins and flips on once the sweep merges — see the
   note in `sast.yml`).
3. **Egress-controlled runners.** Every job's first step is
   `step-security/harden-runner` with `egress-policy: block` and a per-job
   `allowed-endpoints` list.
4. **Ephemeral runners only.** GitHub-hosted runners; no shared self-hosted
   runners for release jobs.
5. **OIDC over secrets.** Publishing identities (Sigstore, crates.io trusted
   publishing, npm trusted publishing, PyPI trusted publishing) use OIDC
   tokens. No long-lived PATs in secrets.
6. **`pull_request_target` never used.** Untrusted PR code never runs with
   write tokens. PR checks run in `pull_request` context with read-only token.

## Allowlisted GitHub Actions

Below is the canonical list. Org-level "Allow select actions and reusable
workflows" should mirror this. Every entry must be SHA-pinned in every
workflow that uses it.

| Action | Purpose | Provenance |
|---|---|---|
| `actions/checkout` | Source checkout | GitHub first-party |
| `actions/setup-node` | Node toolchain | GitHub first-party |
| `actions/cache` | Build cache | GitHub first-party |
| `actions/upload-artifact` | Artifact upload | GitHub first-party |
| `actions/download-artifact` | Artifact download | GitHub first-party |
| `actions/attest-build-provenance` | SLSA-style build attestations | GitHub first-party |
| `actions/attest` | Unified attestations (SBOM mode — replaces `actions/attest-sbom`) | GitHub first-party |
| `actions/setup-python` | Python toolchain | GitHub first-party |
| `github/codeql-action/*` | CodeQL SAST | GitHub first-party |
| `dtolnay/rust-toolchain` | Rust toolchain install | community, audited; alternative: `actions-rs/toolchain` (deprecated, do not use) |
| `Swatinem/rust-cache` | Cargo build cache | community, audited |
| `taiki-e/install-action` | nextest / cargo-tools installer | community, audited |
| `EmbarkStudios/cargo-deny-action` | `cargo deny` runner | vendor first-party |
| `rustsec/audit-check` | `cargo audit` runner | RustSec project first-party |
| `pnpm/action-setup` | pnpm install | pnpm first-party |
| `gitleaks/gitleaks-action` | Secrets detection | vendor first-party |
| `trufflesecurity/trufflehog` | Secrets detection | vendor first-party |
| `aquasecurity/trivy-action` | Container scan | vendor first-party |
| `anchore/scan-action` | Grype container scan | vendor first-party |
| `anchore/sbom-action` | Syft SBOM (image) | vendor first-party |
| `docker/setup-buildx-action` | BuildKit setup | Docker first-party |
| `docker/build-push-action` | Image build/push | Docker first-party |
| `hadolint/hadolint-action` | Dockerfile lint | upstream first-party |
| `returntocorp/semgrep` (container image) | Semgrep SAST — runs as a pinned container image, not an action | vendor first-party |
| `raven-actions/actionlint` | Workflow YAML lint | community wrapper around upstream `rhysd/actionlint`, audited |
| `woodruffw/zizmor-action` | GitHub Actions security audit | upstream first-party |
| `ossf/scorecard-action` | OpenSSF Scorecard | OpenSSF first-party |
| `google/osv-scanner-action` | Multi-ecosystem OSV scan | Google first-party |
| `step-security/harden-runner` | Egress policy on runners | StepSecurity first-party |
| `sigstore/cosign-installer` | cosign binary install | Sigstore first-party |
| `slsa-framework/slsa-github-generator/*` | SLSA L3 provenance | SLSA first-party |
| `softprops/action-gh-release` | GitHub Release publishing | community, audited |
| `zaproxy/action-baseline` | OWASP ZAP DAST | OWASP first-party |
| `mszostok/codeowners-validator` | CODEOWNERS lint | community, audited |

Any new action requires:

- ADR justifying inclusion
- Provenance verification (GitHub Verified, Sigstore-signed, or maintainer review)
- SHA pin in CI-trust doc + every workflow that uses it
- Update to org allowlist before first run

## OIDC trust relationships

| Relying party | Subject claim pattern | Purpose |
|---|---|---|
| Sigstore Fulcio | `repo:ScopeCreep-zip/Springtale:ref:refs/heads/main` | Release-job keyless signing |
| Sigstore Fulcio | `repo:ScopeCreep-zip/Springtale:ref:refs/tags/v*` | Tag-job signing |
| Rekor transparency log | (inherits from Fulcio) | Provenance entries |
| crates.io | (when crates.io trusted publishing GAs) | Publish first-party crates |
| PyPI trusted publishing | `repo:ScopeCreep-zip/Springtale:environment:pypi` | Publish `springtale-py` wheel |

## Runner egress policy

Per-workflow allow-endpoint lists (representative, not exhaustive):

### `ci.yml` jobs (build/test)
- `crates.io` (cargo)
- `static.crates.io` (cargo tarballs)
- `index.crates.io` (cargo registry)
- `registry.npmjs.org` (pnpm)
- `objects.githubusercontent.com` (GitHub asset blobs)
- `api.github.com`
- `github.com`

### `sca.yml` jobs
- All of `ci.yml` plus:
- `rustsec.org` (RustSec advisory DB)
- `osv.dev` (OSV scanner DB)
- `www.cisa.gov` (KEV catalog JSON feed)
- `github.com/advisories` (GH Advisory DB)

### `provenance.yml`
- `fulcio.sigstore.dev`
- `rekor.sigstore.dev`
- `oauth2.sigstore.dev`
- `tuf-repo-cdn.sigstore.dev`

### `container.yml`
- `ghcr.io` (image push)
- `mirror.gcr.io` (distroless pull)
- `gcr.io`
- DB endpoints for Trivy: `ghcr.io/aquasecurity/trivy-db`

`harden-runner` `audit` mode is used in any new workflow for one merge cycle
before flipping to `block`, so we can verify the allow-list is complete.

## Manual repo settings (operator checklist)

Branch protection on `main`:
- [x] Required reviews: 1 from CODEOWNERS, dismiss stale on push
- [x] Required status checks (full list in `.github/repo-config.yml` `branches[main].protection.required_status_checks.contexts`)
- [ ] Required signed commits — INTENTIONALLY OFF per the anonymous-contribution policy in `CONTRIBUTING.md` (a contributor under threat may not be able to tie a key to a real identity). Maintainer release tags are separately signed via Sigstore `gitsign`; see `docs/security/SUPPLY-CHAIN.md` and `docs/operations/verifying-releases.md`.
- [x] Linear history
- [x] No force-push
- [x] No deletion
- [x] No admin override (`enforce_admins: true`)

Repo features:
- [x] Secret scanning + push protection ON
- [x] Dependabot security updates ON (alerts ON via `.github/dependabot.yml`)
- [x] CodeQL ON for JavaScript-TypeScript (Rust pending stable beta)
- [x] Private vulnerability reporting ON
- [x] GitHub Advisories enabled (CNA proxy via GitHub)

Org-level Action policy:
- [x] Allow select actions (the list above)
- [x] Require SHA pinning (GitHub Actions org policy, Aug 2025)
- [x] Default `GITHUB_TOKEN` permissions: `contents: read`
- [x] Disallow `pull_request_target` writes for community PRs

## Incident response

If any allowlisted Action is compromised:
1. Revert workflows that consume it on `main` immediately.
2. Rotate any tokens that ran during the compromise window.
3. Open VEX statement in `vex/` for impact assessment.
4. Notify via GitHub Security Advisory if any production artifact was built
   with the compromised path.

See `docs/security/INCIDENT-RUNBOOK.md`.
