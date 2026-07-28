# syntax=docker/dockerfile:1.7
#
# Image OCI pour `ainonymous-daemon`.
#
# Conformité opencontainers/runtime-spec : voir
# docs/OCI_RUNTIME_SPEC_COMPLIANCE.md pour la correspondance détaillée entre
# chaque choix ci-dessous et les sections concrètes de la spec (process,
# user, linux.resources, mounts, lifecycle/signaux).
#
# Build (depuis la racine du repo, le contexte doit être la racine car le
# binaire fait partie d'un workspace Cargo) :
#   docker build -f docker/ainonymous-daemon.Dockerfile -t ainonymous-daemon .

# ---- Stage 1 : build -------------------------------------------------------
FROM rust:1.80-slim-bookworm AS builder

# build-essential (cc/ld) + pkg-config + libssl-dev : requis par openssl-sys,
# tiré transitivement par `native-tls` dans l'arbre de dépendances de
# `reqwest` (workspace.dependencies ne fixe pas de backend TLS explicite,
# donc reqwest résout à la fois native-tls ET rustls — confirmé dans
# Cargo.lock). Sans ces paquets, le build échoue à la compilation du crate
# `openssl-sys`.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Le binaire fait partie d'un workspace Cargo (root Cargo.toml) : Cargo lit
# TOUS les manifests des membres du workspace pour la résolution de deps,
# même en ne buildant qu'un seul package avec `-p`. On copie donc le
# workspace complet plutôt que d'essayer d'isoler un sous-ensemble fragile.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release --locked -p ainonymous-daemon

# ---- Stage 2 : runtime ------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates : requis pour toute connexion TLS sortante (reqwest vers
#   llama-server local + éventuels appels HTTP du backend Holochain "static").
# libssl3 : lib partagée requise au runtime par le binaire lié à openssl-sys.
# wget : utilisé uniquement par HEALTHCHECK ci-dessous (endpoint /health
#   interne, cf. router.rs) — image encore minimale (~+2 Mo).
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        wget \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 ainonymous \
    && useradd --uid 10001 --gid ainonymous --no-create-home --shell /usr/sbin/nologin ainonymous \
    && mkdir -p /data/models /config \
    && chown -R ainonymous:ainonymous /data /config

COPY --from=builder /build/target/release/ainonymous-daemon /usr/local/bin/ainonymous-daemon

# Process non-root dès le lancement (OCI runtime-spec `process.user`) — pas
# de root à aucun moment dans ce conteneur, y compris pendant l'installation
# des paquets (faite dans le même RUN, avant le changement d'utilisateur).
USER ainonymous:ainonymous
WORKDIR /data

ENV AINON_CONFIG=/config/config.toml
ENV RUST_LOG=info

# Ports par défaut de DaemonConfig::default() (config.rs) :
#   9000/udp  QUIC data-plane (mTLS ed25519)      -> doit être joignable par les pairs
#   8890/tcp  REST interne (plan de contrôle)     -> ATTENTION : le code lie ce
#             port sur 127.0.0.1 en dur (main.rs), donc il n'est PAS joignable
#             depuis l'extérieur du conteneur même avec ce EXPOSE/un port-map
#             Docker. EXPOSE documente l'intention ; cf.
#             docs/OCI_RUNTIME_SPEC_COMPLIANCE.md pour le détail de cette
#             limite connue et la piste de correction (bind 0.0.0.0
#             configurable) si le REST doit un jour être exposé hors conteneur.
EXPOSE 9000/udp
EXPOSE 8890/tcp

# Healthcheck sur l'endpoint /health (ajouté à router.rs) — exécuté par le
# runtime Docker/containerd DANS le namespace réseau du conteneur, donc
# 127.0.0.1 est bien atteignable ici même si le port n'est pas exposé
# à l'extérieur (cf. remarque ci-dessus).
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --retries=3 \
    CMD wget -q -O- http://127.0.0.1:8890/health || exit 1

ENTRYPOINT ["/usr/local/bin/ainonymous-daemon"]
