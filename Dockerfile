# ── Builder stage ──────────────────────────────────────────────────────────────
FROM rust:1.85-slim AS builder

WORKDIR /build
COPY . .

RUN cargo build --release --bin springtaled --bin springtale-cli

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Non-root user per architecture security audit
RUN groupadd -g 1000 springtale && \
    useradd -u 1000 -g springtale -m springtale

# Runtime dependencies (SQLite is bundled, so minimal)
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates wget && \
    rm -rf /var/lib/apt/lists/*

# Create data directory
RUN mkdir -p /data && chown springtale:springtale /data

# Copy binaries
COPY --from=builder /build/target/release/springtaled /usr/local/bin/springtaled
COPY --from=builder /build/target/release/springtale-cli /usr/local/bin/springtale

# Switch to non-root user
USER 1000:1000

# Default data directory
ENV XDG_DATA_HOME=/data

# Healthcheck via management API (same as docker-compose.yml)
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["wget", "-q", "--spider", "http://localhost:8080/health"]

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/springtaled"]
