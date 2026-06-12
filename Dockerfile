# Springtale daemon container.
#
# Hardened per NIST SP 800-190 (Application Container Security):
# - multi-stage build with minimal final layer
# - distroless final image (no shell, no package manager, no busybox)
# - non-root UID 65532 (matches distroless `nonroot` user)
# - `cargo auditable build` embeds dep graph in binary for post-build scan
# - `cargo --locked` for reproducible, lockfile-only builds
# - FROM lines pinned by multi-arch manifest-list digest. Tag is kept
#   for human readability; the @sha256 is the authoritative byte
#   identity. Dependabot's `docker` ecosystem (see
#   `.github/dependabot.yml`) opens weekly PRs that bump both
#   together so the pin tracks upstream patch releases without going
#   silently stale.
#
# Build-stage audit: no `apt-get install` needed.
# - rusqlite is `bundled` so libsqlite3-sys-mc's pkg-config branch is dead
#   code (gated behind `LIBSQLITE3_SYS_USE_PKG_CONFIG` env / loadable_extension
#   feature — neither active).
# - rustls uses ring (statically linked), not openssl-sys.
# - ca-certificates is already present in the Debian base used by
#   `rust:slim`, so cargo's TLS fetches against crates.io work out of
#   the box.

# ── Frontend stage ────────────────────────────────────────────────────────────
# Builds the dashboard SPA so `springtaled` can embed it via rust-embed
# (`src/api/dashboard.rs` -> `tauri/apps/dashboard/dist/`). Without this the
# release binary would only carry the build.rs placeholder. node:22-slim
# multi-arch manifest-list digest (2026-05-28); Dependabot's `docker`
# ecosystem keeps the pin current.
FROM node:22-slim@sha256:e21fc383b50d5347dc7a9f1cae45b8f4e2f0d39f7ade28e4eef7d2934522b752 AS frontend

# pnpm via corepack — version matches `packageManager` / CI `pnpm/action-setup`.
RUN corepack enable && corepack prepare pnpm@9 --activate

WORKDIR /build
COPY tauri/ tauri/
# `--frozen-lockfile` mirrors CI; lifecycle scripts are blocked by tauri/.npmrc
# (`ignore-scripts=true`), enforced by the hardening-check job. Install and
# build in one RUN (hadolint DL3059). `springtale-dashboard...` selects the
# dashboard plus its workspace deps (@springtale/ui, @springtale/types).
RUN pnpm -C tauri install --frozen-lockfile && \
    pnpm -C tauri --filter "springtale-dashboard..." run build

# ── Builder stage ─────────────────────────────────────────────────────────────
# rust:1.96-slim multi-arch manifest-list digest (2026-05-28). Matches
# the workspace's `rust-toolchain.toml` channel = "stable".
FROM rust:1.96-slim@sha256:082a5849a6870672b5f7a5bf4eddc71723fce38756fd834a0d734a5306a310ab AS builder

# cargo-auditable embeds the dep graph in the produced binary so post-build
# `osv-scanner sbom-from-binary`, `syft`, and `grype` can re-derive the
# SBOM without access to source.
RUN cargo install cargo-auditable --locked --version '~0.6'

WORKDIR /build
COPY . .
# Overlay the built dashboard SPA from the frontend stage so rust-embed bakes
# the real assets (not the build.rs placeholder) into the release binary.
COPY --from=frontend /build/tauri/apps/dashboard/dist/ tauri/apps/dashboard/dist/

RUN cargo auditable build --release --locked \
      --bin springtaled --bin springtale-cli

# ── Runtime stage ─────────────────────────────────────────────────────────────
#
# Distroless `cc` base — glibc + ca-certificates + nonroot user (UID 65532),
# no shell, no package manager. Smallest image that still supports a
# dynamically-linked Rust binary (rustls' ring backend links to libc).
# OCI image-index digest (2026-05); Dependabot tracks updates per the
# docker ecosystem entry in `.github/dependabot.yml`.
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:b0ae8e989418b458e0f25489bc3be523718938a2b70864cc0f6a00af1ddbd985 AS runtime

# OCI image annotations improve Trivy / Grype scan output + GitHub Packages
# display.
LABEL org.opencontainers.image.source="https://github.com/ScopeCreep-zip/Springtale"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.description="Springtale daemon — local-first, privacy-preserving automation"
LABEL org.opencontainers.image.vendor="Springtale Maintainers"
LABEL org.opencontainers.image.title="springtaled"
LABEL org.opencontainers.image.url="https://github.com/ScopeCreep-zip/Springtale"

# Copy binaries from the builder. Distroless has no `chown` utility, so we
# rely on the build context's permissions (root-owned, world-readable).
COPY --from=builder /build/target/release/springtaled /usr/local/bin/springtaled
COPY --from=builder /build/target/release/springtale-cli /usr/local/bin/springtale

# Run as nonroot UID 65532 per distroless convention.
USER nonroot:nonroot

ENV XDG_DATA_HOME=/data

EXPOSE 8080

# Container healthcheck via the `springtale healthcheck` CLI subcommand
# (no `wget` / `curl` available in distroless). The subcommand probes
# /health and exits 0 on 2xx.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/springtale", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/springtaled"]
