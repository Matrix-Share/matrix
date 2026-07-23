# syntax=docker/dockerfile:1
#
# Multi-stage build for Project Lifeline. Produces a single small image
# containing both binaries; `docker compose` selects which one to run.
#
#   docker build -t lifeline .
#   docker run --rm -p 7000:7000 lifeline lifeline-relay
#
# (For faster CI builds, swap the manual copy for cargo-chef dependency caching.)

FROM rust:1-slim AS builder
WORKDIR /build
# Build dependencies first would go here with cargo-chef; kept simple for clarity.
COPY . .
RUN cargo build --release -p lifeline-node -p lifeline-relay

FROM debian:bookworm-slim AS runtime
# Minimal runtime; no shell tools needed. Run as an unprivileged user.
RUN groupadd -r lifeline \
    && useradd -r -g lifeline -u 10001 lifeline \
    && mkdir -p /data \
    && chown -R lifeline:lifeline /data
COPY --from=builder /build/target/release/lifeline-node  /usr/local/bin/lifeline-node
COPY --from=builder /build/target/release/lifeline-relay /usr/local/bin/lifeline-relay

USER lifeline
ENV LIFELINE_DATA_DIR=/data \
    LIFELINE_NODE_ADDR=0.0.0.0:8080 \
    LIFELINE_RELAY_ADDR=0.0.0.0:7000 \
    RUST_LOG=info

# GUI/API (node) and relay hub, respectively.
EXPOSE 8080 7000

# Default to the node; compose overrides `command:` for the relay.
CMD ["lifeline-node"]
