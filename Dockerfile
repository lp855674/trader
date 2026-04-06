# Multi-stage Dockerfile for the trader platform
# Stage 1: Builder
FROM rust:1.82-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build release binary (quantd is the main entry point)
RUN cargo build --release -p quantd

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime

WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/quantd /app/quantd

# Copy default config
COPY crates/config/ /app/config/

# Non-root user for security
RUN useradd -r -s /bin/false trader
USER trader

EXPOSE 8080 9090

ENTRYPOINT ["/app/quantd"]
