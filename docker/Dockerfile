# syntax=docker/dockerfile:1.7
#
# Multi-arch (linux/amd64, linux/arm64) build for pipa-server.
#
#   docker buildx build \
#     --platform linux/amd64,linux/arm64 \
#     -t ghcr.io/fguisso/pipa:dev \
#     -f docker/Dockerfile .
#
# The runtime image is debian:bookworm-slim with `tini` as PID 1 and a
# non-root `pipa` user (UID/GID 10001). Listens HTTP on :8080 — put Caddy
# or another TLS-terminating proxy in front of it. See SECURITY.md and
# adr/0005-tls-termination.md.

# ─── Builder stage ──────────────────────────────────────────────────────────
FROM rust:1.94-bookworm AS builder

WORKDIR /src

# System deps the build needs (sqlx-macros, rust-embed for templates, etc.).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        pkg-config \
        ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Copy the workspace. We rely on .dockerignore to keep ./target and ./data
# out of the build context.
COPY . .

# Build with cached cargo registry + git db + per-target build cache so
# subsequent builds are fast. `--locked` keeps Cargo.lock authoritative.
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p pipa-server \
 && cp /src/target/release/pipa-server /usr/local/bin/pipa-server

# ─── Runtime stage ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# tini = PID 1 reaper. curl = healthcheck. ca-certificates = outbound HTTPS
# (webhooks, future OAuth). No build toolchain in the runtime image.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        ca-certificates \
        tini \
        curl \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 10001 pipa \
 && useradd  --system --uid 10001 --gid 10001 \
        --home-dir /var/lib/pipa \
        --shell /usr/sbin/nologin \
        pipa \
 && mkdir -p /data /pages /etc/pipa \
 && chown -R pipa:pipa /data /pages /etc/pipa

COPY --from=builder /usr/local/bin/pipa-server /usr/local/bin/pipa-server

USER pipa:pipa
WORKDIR /var/lib/pipa

# /data    — SQLite database + key material
# /pages   — per-page static bundles
# /etc/pipa/pages.toml — config file (bind-mount or build into your own image)
VOLUME ["/data", "/pages"]

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/health || exit 1

ENTRYPOINT ["tini", "--"]
CMD ["pipa-server", "--config", "/etc/pipa/pages.toml"]
