# Reproducible bundle builder for the pinned DNA epoch.
#
# WHY THIS IMAGE EXISTS
# ---------------------
# The effective DNA hash is the hash of the packed DNA bundle, which embeds the
# zome WASM. Two independent builds are bit-identical on the same machine
# (measured 2026-10-29), so the compiler itself is deterministic; what leaks into
# the WASM is the *environment*: rustc embeds the absolute source paths of
# registry dependencies (for example
# /home/runner/.cargo/registry/src/index.crates.io-.../serde-1.0.x/src/de.rs) in
# panic locations. Change CARGO_HOME, the workspace path, the rustup home or the
# host, and the bytes change. Reproducing the epoch therefore means reproducing
# the reference environment exactly, not merely pinning the compiler.
#
# Measured on 2026-10-29:
#   * ubuntu-latest, /home/runner/{.cargo,.rustup}, workspace
#     /home/runner/work/AInonymous/AInonymous -> the pinned hashes;
#   * the same sources on Windows, with Rust 1.91.1 and path remapping -> other
#     hashes (drive letters cannot be remapped to POSIX paths);
#   * a Debian container with the default CARGO_HOME (/usr/local/cargo) and the
#     reference workspace path -> yet another hash.
#
# This image mirrors the runner layout (same HOME, CARGO_HOME, RUSTUP_HOME,
# toolchain name and workspace path), so any host with Docker rebuilds the
# bundles byte for byte. The CI job that runs it is the proof: it exits non-zero
# unless the rebuilt bundles match deploy/holochain/dna-hashes.json.
#
# USAGE
# -----
#   docker build -f deploy/containers/repro/Dockerfile -t ainonymous-repro .
#   docker run --rm ainonymous-repro
FROM debian:bookworm-slim

# Mirror the GitHub Actions runner layout, because these paths end up in the WASM.
ENV HOME=/home/runner \
    RUSTUP_HOME=/home/runner/.rustup \
    CARGO_HOME=/home/runner/.cargo \
    DEBIAN_FRONTEND=noninteractive \
    CARGO_TERM_COLOR=never

ENV PATH=/home/runner/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# Official holochain-0.7.0 Linux release asset, same value pinned in
# .github/workflows/ci.yml, scripts/setup_holochain_wsl.sh and
# scripts/setup_holochain_windows.ps1 for their own platforms.
ARG HC_SHA256=f1eca56b97bc2261324e00e0e86a274f7dc8363f73264e697fd0c0216b2aac23

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
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /home/runner

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain none \
 && rustup toolchain install 1.91 --profile minimal --no-self-update \
 && rustup default 1.91 \
 && rustup target add wasm32-unknown-unknown \
 && rustc --version \
 && cargo --version

RUN curl --fail --location --silent --show-error \
      "https://github.com/holochain/holochain/releases/download/holochain-0.7.0/hc-x86_64-unknown-linux-gnu" \
      --output /usr/local/bin/hc \
 && echo "${HC_SHA256}  /usr/local/bin/hc" | sha256sum --check --strict \
 && chmod 0755 /usr/local/bin/hc \
 && hc --version

# The workspace path is part of the input as well.
WORKDIR /home/runner/work/AInonymous/AInonymous

COPY . .

CMD ["bash", "-c", "bash scripts/build-happ.sh release && find dnas -path '*/zomes/*.wasm' -print0 | sort -z | xargs -0 sha256sum && cargo run --locked -p dna-hashes -- --check deploy/holochain/dna-hashes.json dnas/ainonymous-core/dnas/inference-mesh/workdir/inference-mesh.dna dnas/ainonymous-core/dnas/agent-registry/workdir/agent-registry.dna dnas/ainonymous-core/dnas/blackboard/workdir/blackboard.dna dnas/hybridnode/dnas/hybridnode-core/workdir/hybridnode-core.dna"]
