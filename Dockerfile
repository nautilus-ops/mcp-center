FROM rust:1.89.0-slim AS builder

# Install necessary packages for OpenSSL and build tools
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    build-essential \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Set environment variables to use system OpenSSL
ENV OPENSSL_NO_VENDOR=1

WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./

COPY mc-booter ./mc-booter
COPY mc-common ./mc-common
COPY mc-db ./mc-db
COPY mc-loader ./mc-loader
COPY mc-registry ./mc-registry
COPY mc-service ./mc-service

RUN cargo build -p mc-service --release

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    openssl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/mc-service /app/mcp-center

COPY bootstrap.toml /app/
COPY mcp_servers.toml.example /app/mcp_servers.toml

CMD ["./mcp-center", "run", "--config", "bootstrap.toml"]
