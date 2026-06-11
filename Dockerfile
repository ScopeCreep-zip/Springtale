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

# ── Builder stage ─────────────────────────────────────────────────────────────
# rust:1.96-slim multi-arch manifest-list digest (2026-05-28). Matches
# the workspace's `rust-toolchain.toml` channel = "stable".
FROM rust:1.96-slim@sha256:26abcef3d79b8d890c4ceb17093154573e1f6479cf6dd7c1450043b8458350f6 AS builder

# cargo-auditable embeds the dep graph in the produced binary so post-build
# `osv-scanner sbom-from-binary`, `syft`, and `grype` can re-derive the
# SBOM without access to source.
RUN cargo install cargo-auditable --locked --version '~0.6'

WORKDIR /build
COPY . .

RUN cargo auditable build --release --locked \
      --bin springtaled --bin springtale-cli

# ── Runtime stage ─────────────────────────────────────────────────────────────
#
# Distroless `cc` base — glibc + ca-certificates + nonroot user (UID 65532),
# no shell, no package manager. Smallest image that still supports a
# dynamically-linked Rust binary (rustls' ring backend links to libc).
# OCI image-index digest (2026-05); Dependabot tracks updates per the
# docker ecosystem entry in `.github/dependabot.yml`.
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:bd2899c12b335c827750ccf2359879eab09c09b206023dcebea408947d54127c AS runtime

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
