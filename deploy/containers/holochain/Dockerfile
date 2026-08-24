FROM debian:bookworm-slim

ARG HOLOCHAIN_VERSION=0.7.0
ARG HOLOCHAIN_SHA256
ARG HC_SHA256

RUN test -n "$HOLOCHAIN_SHA256" && test -n "$HC_SHA256" \
    && apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl socat \
    && curl --fail --location --silent --show-error \
       "https://github.com/holochain/holochain/releases/download/holochain-${HOLOCHAIN_VERSION}/holochain-x86_64-unknown-linux-gnu" \
       --output /usr/local/bin/holochain \
    && echo "${HOLOCHAIN_SHA256}  /usr/local/bin/holochain" | sha256sum --check --strict \
    && curl --fail --location --silent --show-error \
       "https://github.com/holochain/holochain/releases/download/holochain-${HOLOCHAIN_VERSION}/hc-x86_64-unknown-linux-gnu" \
       --output /usr/local/bin/hc \
    && echo "${HC_SHA256}  /usr/local/bin/hc" | sha256sum --check --strict \
    && chmod 0755 /usr/local/bin/holochain /usr/local/bin/hc \
    && apt-get purge --yes --auto-remove curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid 10000 holochain \
    && useradd --uid 10000 --gid 10000 --create-home --shell /usr/sbin/nologin holochain \
    && install -d -o holochain -g holochain -m 0700 /var/lib/holochain /var/lib/holochain/keystore \
    && install -d -o holochain -g holochain -m 0755 /opt/happs

COPY --chown=holochain:holochain deploy/containers/holochain/conductor.yaml /etc/holochain/conductor.yaml
COPY --chown=holochain:holochain deploy/containers/holochain/entrypoint.sh /usr/local/bin/ainonymous-holochain-entrypoint

RUN chmod 0755 /usr/local/bin/ainonymous-holochain-entrypoint

USER 10000:10000
WORKDIR /var/lib/holochain

ENTRYPOINT ["/usr/local/bin/ainonymous-holochain-entrypoint"]
