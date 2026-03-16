# ── Stage 1: Build ────────────────────────────────────────────────────────────
FROM rust:1.85-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies separately from source code
COPY Cargo.toml Cargo.lock ./
COPY limiter-core/Cargo.toml  limiter-core/Cargo.toml
COPY limiter-server/Cargo.toml limiter-server/Cargo.toml
COPY limiter-client/Cargo.toml limiter-client/Cargo.toml

RUN mkdir -p limiter-core/src limiter-server/src limiter-client/src \
    && echo "fn main() {}" > limiter-server/src/main.rs \
    && echo "fn main() {}" > limiter-client/src/main.rs \
    && echo "" > limiter-core/src/lib.rs

RUN cargo build --release -p limiter-server 2>/dev/null || true

COPY limiter-core/src     limiter-core/src
COPY limiter-core/benches limiter-core/benches
COPY limiter-server/src   limiter-server/src
COPY limiter-server/build.rs limiter-server/build.rs
COPY proto proto

RUN touch limiter-core/src/lib.rs limiter-server/src/main.rs \
    && cargo build --release -p limiter-server

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --no-create-home --shell /bin/false limiter
USER limiter

COPY --from=builder /app/target/release/limiter-server /usr/local/bin/limiter-server

EXPOSE 50051

ENV REDIS_URL=redis://redis:6379
ENV LISTEN_ADDR=0.0.0.0:50051
ENV DEFAULT_CAPACITY=100
ENV DEFAULT_REFILL_RATE=10.0

ENTRYPOINT ["/usr/local/bin/limiter-server"]
