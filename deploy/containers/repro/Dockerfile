# Reproducible bundle builder for the pinned DNA epoch.
#
# WHY THIS IMAGE EXISTS
# ---------------------
# The effective DNA hash is the hash of the packed DNA bundle, which embeds the
# zome WASM. Cargo mixes the *absolute* path of each package into the crate
# metadata hash, and that hash reaches symbol names inside the WASM, so the same
# sources built at a different path (or on another OS) may produce different
# bytes. Measured on 2026-10-29:
#
#   * ubuntu at /home/runner/work/AInonymous/AInonymous (with --remap-path-prefix)
#     -> the pinned hashes, unchanged;
#   * Windows, same Rust 1.91.1, with --remap-path-prefix -> different hashes;
#   * Windows, Rust 1.95 -> a third, different set.
#
# Path remapping alone is therefore not sufficient, and the working explanation
# is the path-derived metadata hash. This image pins the toolchain AND the build
# path: the CI job that runs it is the proof (it exits non-zero unless the
# rebuilt bundles match the pinned registry).
#
# See docs/DNA_HASHES.md for the measurements. The conclusion: pin the toolchain
# AND the build path. This image does both, so any host with Docker rebuilds the
# bundles byte for byte and can verify them against the pinned registry.
#
# USAGE
# -----
#   docker build -f deploy/containers/repro/Dockerfile -t ainonymous-repro .
#   docker run --rm ainonymous-repro
#
# The container exits non-zero unless the produced bundles match
# deploy/holochain/dna-hashes.json.
FROM debian:bookworm-slim

ARG RUST_VERSION=1.91.1

# Official holochain-0.7.0 Linux release asset, same value as
# .github/workflows/ci.yml, scripts/setup_holochain_wsl.sh and
# scripts/setup_holochain_windows.ps1 pin for their own platforms.
ARG HC_SHA256=f1eca56b97bc2261324e00e0e86a274f7dc8363f73264e697fd0c0216b2aac23

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_TERM_COLOR=never \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/local/sbin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      build-essential \
      ca-certificates \
      clang \
      cmake \
      curl \
      libdbus-1-dev \
      libclang-dev \
      libssl-dev \
      pkg-config \
      python3 \
 && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal \
          --default-toolchain "${RUST_VERSION}" \
 && rustup target add wasm32-unknown-unknown \
 && rustc --version \
 && cargo --version

RUN curl --fail --location --silent --show-error \
      "https://github.com/holochain/holochain/releases/download/holochain-0.7.0/hc-x86_64-unknown-linux-gnu" \
      --output /usr/local/bin/hc \
 && echo "${HC_SHA256}  /usr/local/bin/hc" | sha256sum --check --strict \
 && chmod 0755 /usr/local/bin/hc \
 && hc --version

# The build path is part of the input: keep the reference path used by CI.
WORKDIR /home/runner/work/AInonymous/AInonymous

COPY . .

CMD ["bash", "-c", "bash scripts/build-happ.sh release && cargo run --locked -p dna-hashes -- --check deploy/holochain/dna-hashes.json dnas/ainonymous-core/dnas/inference-mesh/workdir/inference-mesh.dna dnas/ainonymous-core/dnas/agent-registry/workdir/agent-registry.dna dnas/ainonymous-core/dnas/blackboard/workdir/blackboard.dna dnas/hybridnode/dnas/hybridnode-core/workdir/hybridnode-core.dna"]
