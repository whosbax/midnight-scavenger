# syntax=docker/dockerfile:1

# ====== Build stage ======
FROM rust:1.90-slim-bullseye AS builder
WORKDIR /usr/src/app

# Dépendances build
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copier manifest pour cache de build
COPY Cargo.toml Cargo.lock* ./

COPY . .

# Build release final
RUN cargo build --release

# ====== Runtime stage ======
FROM debian:bullseye-slim AS runtime
WORKDIR /usr/local/bin

# Installer ca-certificates pour HTTPS
RUN apt-get update && apt-get install -y ca-certificates && update-ca-certificates && rm -rf /var/lib/apt/lists/*

# Copier binaire compilé
COPY --from=builder /usr/src/app/target/release/scavenger_miner .

# Créer utilisateur non-root
RUN useradd -m appuser

# Créer répertoire config et donner droits à appuser
RUN mkdir -p /usr/local/bin/config && chown -R appuser:appuser /usr/local/bin/config /usr/local/bin/scavenger_miner

USER appuser

CMD ["./scavenger_miner"]
