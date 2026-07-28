# syntax=docker/dockerfile:1.7
#
# Image OCI pour `hybridnode` (crate hybridnode-daemon).
#
# Conformité opencontainers/runtime-spec : voir
# docs/OCI_RUNTIME_SPEC_COMPLIANCE.md.
#
# Build (depuis la racine du repo) :
#   docker build -f docker/hybridnode-daemon.Dockerfile -t hybridnode-daemon .

# ---- Stage 1 : build -------------------------------------------------------
FROM rust:1.80-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Feature par défaut = mock-sdwan (cf. hybridnode-daemon/Cargo.toml). Pour
# une image en conditions réelles avec un contrôleur SD-WAN, passer
# --build-arg FEATURES=vmanage et adapter la ligne cargo build ci-dessous.
RUN cargo build --release --locked -p hybridnode-daemon

# ---- Stage 2 : runtime ------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        wget \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10002 hybridnode \
    && useradd --uid 10002 --gid hybridnode --no-create-home --shell /usr/sbin/nologin hybridnode \
    && mkdir -p /config \
    && chown -R hybridnode:hybridnode /config

COPY --from=builder /build/target/release/hybridnode /usr/local/bin/hybridnode

USER hybridnode:hybridnode
WORKDIR /config

ENV RUST_LOG=info

# Endpoint Prometheus (ObservabilityConfig::default(), hybridnode-core/config.rs)
# lié sur 0.0.0.0:9338 par défaut -> réellement joignable depuis l'extérieur
# du conteneur, contrairement au REST d'ainonymous-daemon.
EXPOSE 9338/tcp

HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --retries=3 \
    CMD wget -q -O- http://127.0.0.1:9338/metrics || exit 1

ENTRYPOINT ["/usr/local/bin/hybridnode"]
# Chemin par défaut du CLI (Cli::config dans main.rs) ; à monter via un
# volume ou à surcharger avec `docker run ... --config /config/other.yaml`.
CMD ["--config", "/config/ainonymous.hybridnode.yaml"]
