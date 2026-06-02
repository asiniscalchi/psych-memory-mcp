# Build the wrapper as a static-ish release binary, then run it on a slim base.
FROM rust:1-bookworm AS builder
WORKDIR /app
# Copy manifests first for better layer caching of dependency builds.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
RUN cargo build --release --bin psych-memory-mcp

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/psych-memory-mcp /usr/local/bin/psych-memory-mcp

# Exposed MCP endpoint; MEMORY_BASE_URL points at the internal memory-service.
ENV HTTP_BIND=0.0.0.0:8000
EXPOSE 8000
ENTRYPOINT ["psych-memory-mcp"]
