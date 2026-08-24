FROM rust:1.91-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
       cmake clang libclang-dev libdbus-1-dev libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY tools ./tools
RUN cargo build --locked --release --package hybridnode-daemon

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl libdbus-1-3 libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10002 hybridnode \
    && useradd --uid 10002 --gid 10002 --create-home --shell /usr/sbin/nologin hybridnode

COPY --from=builder /build/target/release/hybridnode-daemon /usr/local/bin/hybridnode-daemon

USER 10002:10002
WORKDIR /home/hybridnode
EXPOSE 9338/tcp

ENTRYPOINT ["/usr/local/bin/hybridnode-daemon"]
