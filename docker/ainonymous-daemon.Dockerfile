FROM rust:1.88-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
       cmake clang libclang-dev libdbus-1-dev libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY tools ./tools
RUN cargo build --locked --release --package ainonymous-daemon

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
       ca-certificates curl libdbus-1-3 libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 ainonymous \
    && useradd --uid 10001 --gid 10001 --create-home --shell /usr/sbin/nologin ainonymous \
    && install -d -o ainonymous -g ainonymous -m 0750 /data/models

COPY --from=builder /build/target/release/ainonymous-daemon /usr/local/bin/ainonymous-daemon

USER 10001:10001
WORKDIR /data
EXPOSE 8890/tcp 9000/udp

ENTRYPOINT ["/usr/local/bin/ainonymous-daemon"]
