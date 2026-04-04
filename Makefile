.PHONY: all build build-rust build-happ install clean dev test

# ─── Variables ────────────────────────────────────────────────────────────────
CARGO        := cargo
CARGO_FLAGS  := --release
TARGET_WASM  := wasm32-unknown-unknown

# ─── Cibles principales ───────────────────────────────────────────────────────

all: build

## build: compile crates Rust natifs + zomes WASM + package le hApp
build: build-rust build-happ

## build-rust: compile uniquement les crates Rust natifs (daemon, proxy, cli, mcp)
build-rust:
	$(CARGO) build $(CARGO_FLAGS)

## build-happ: compile les zomes WASM et package le hApp Holochain
build-happ:
	@bash scripts/build-happ.sh release

## build-dev: compilation rapide en mode debug
build-dev:
	$(CARGO) build
	@bash scripts/build-happ.sh dev

## install: installe les binaires dans ~/.local/bin
install: build-rust
	$(CARGO) install --path crates/ainonymous-cli
	$(CARGO) install --path crates/ainonymous-daemon
	$(CARGO) install --path crates/ainonymous-proxy
	$(CARGO) install --path crates/ainonymous-mcp
	@echo "✓ ainonymous-cli, ainonymous-daemon, ainonymous-proxy, ainonymous-mcp installés"

## dev: lance le daemon + proxy en mode développement
dev: build-dev
	RUST_LOG=debug ainonymous start --verbose

## test: lance les tests unitaires
test:
	$(CARGO) test

## clippy: linter Rust
clippy:
	$(CARGO) clippy -- -D warnings
	cd dnas/ainonymous-core && $(CARGO) clippy --target $(TARGET_WASM) -- -D warnings

## clean: supprime les artefacts de build
clean:
	$(CARGO) clean
	cd dnas/ainonymous-core && $(CARGO) clean
	find dnas/ainonymous-core/dnas -name "*.wasm" -delete
	find dnas/ainonymous-core/dnas -name "*.dna" -delete
	rm -f dnas/ainonymous-core/*.happ

## wasm-target: installe la cible WASM (une seule fois)
wasm-target:
	rustup target add $(TARGET_WASM)

## setup: installe les prérequis (une seule fois)
setup: wasm-target
	@echo "Vérification des outils Holochain..."
	@command -v hc >/dev/null 2>&1 || { echo "⚠ hc (Holochain CLI) non trouvé — voir https://developer.holochain.org/get-started/"; }
	@command -v holochain >/dev/null 2>&1 || { echo "⚠ holochain non trouvé — voir https://developer.holochain.org/get-started/"; }
	@echo "✓ Setup terminé"

help:
	@grep -E '^## ' Makefile | sed 's/## /  /'
