# Build stage
FROM rust:1.80-slim-bullseye AS builder
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Build dependencies cache (dummy src to cache layer)
RUN mkdir -p src && touch src/lib.rs src/main.rs && \
    cargo build --release 2>/dev/null || true

# Copy actual source
COPY src ./src
COPY migrations ./migrations

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl-dev && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/spo-api /app/spo-api
COPY --from=builder /app/migrations /app/migrations

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["/app/spo-api"]