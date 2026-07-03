#!/usr/bin/env python3
"""
repack-happ.py — Repack ainonymous-core.happ depuis les WASM compilés.

Usage:
    python3 scripts/repack-happ.py [--dna-only]

Sans argument : rebuilde les 3 .dna ET le .happ final.
Avec --dna-only  : ne rebuilde que les .dna modifiés.

Dépendance : pip install msgpack
"""

import argparse
import gzip
import io
import os
import sys
from pathlib import Path

try:
    import msgpack
except ImportError:
    sys.exit("msgpack manquant — lance : pip install msgpack")

# ── Chemins ──────────────────────────────────────────────────────────────────

ROOT = Path(__file__).parent.parent
DNA_ROOT = ROOT / "dnas" / "ainonymous-core"
TARGET_WASM = (
    ROOT / "dnas" / "ainonymous-core" /
    "target" / "wasm32-unknown-unknown" / "release"
)

DNAS = {
    "inference-mesh": {
        "workdir":   DNA_ROOT / "dnas" / "inference-mesh" / "workdir",
        "zomes_dir": DNA_ROOT / "dnas" / "inference-mesh" / "zomes",
        "wasm_keys": [
            "inference-mesh-integrity.wasm",
            "inference-mesh-coordinator.wasm",
        ],
    },
    "agent-registry": {
        "workdir":   DNA_ROOT / "dnas" / "agent-registry" / "workdir",
        "zomes_dir": DNA_ROOT / "dnas" / "agent-registry" / "zomes",
        "wasm_keys": [
            "agent-registry-integrity.wasm",
            "agent-registry-coordinator.wasm",
        ],
    },
    "blackboard": {
        "workdir":   DNA_ROOT / "dnas" / "blackboard" / "workdir",
        "zomes_dir": DNA_ROOT / "dnas" / "blackboard" / "zomes",
        "wasm_keys": [
            "blackboard-integrity.wasm",
            "blackboard-coordinator.wasm",
        ],
    },
}

HAPP_YAML_PATH = DNA_ROOT / "happ.yaml"
HAPP_OUT = DNA_ROOT / "ainonymous-core.happ"

# ── Helpers msgpack+gzip ─────────────────────────────────────────────────────


def load_bundle(path: Path) -> dict:
    with gzip.open(path, "rb") as f:
        return msgpack.unpackb(f.read(), raw=False)


def save_bundle(path: Path, obj: dict) -> None:
    data = msgpack.packb(obj, use_bin_type=True)
    with gzip.open(path, "wb", compresslevel=6) as f:
        f.write(data)
    print(f"  ✓ {path.name} ({path.stat().st_size / 1024:.0f} KB)")


def load_wasm(zomes_dir: Path, key: str) -> bytes:
    """
    Charge le WASM depuis le dossier zomes/ (artefact déjà copié par
    build-happ.sh), sinon depuis target/wasm32-unknown-unknown/release/.
    """
    # 1) zomes/ en priorité (déjà copié / ancienne version)
    candidate = zomes_dir / key
    if candidate.exists():
        data = candidate.read_bytes()
        return data

    # 2) target release (nouvellement compilé, nom cargo = underscores)
    cargo_name = key.replace("-", "_")
    release_path = TARGET_WASM / cargo_name
    if release_path.exists():
        data = release_path.read_bytes()
        return data

    raise FileNotFoundError(
        f"WASM introuvable : {key}\n"
        f"  cherché dans : {candidate}\n"
        f"  cherché dans : {release_path}"
    )


# ── Repack .dna ──────────────────────────────────────────────────────────────


def repack_dna(name: str, cfg: dict, force: bool = False) -> bytes:
    """
    Repacke un .dna en chargeant les WASM depuis zomes/ ou target/.
    Retourne les bytes du nouveau .dna.
    """
    workdir: Path = cfg["workdir"]
    zomes_dir: Path = cfg["zomes_dir"]
    dna_path = workdir / f"{name}.dna"

    print(f"\n[DNA] {name}")

    # Charger le bundle existant (garde le manifest intact)
    bundle = load_bundle(dna_path)

    # Mettre à jour les resources avec les WASM frais
    resources: dict = bundle.get("resources", {})
    for wasm_key in cfg["wasm_keys"]:
        old_size = len(resources.get(wasm_key, b""))
        wasm_bytes = load_wasm(zomes_dir, wasm_key)
        resources[wasm_key] = wasm_bytes
        print(f"  {wasm_key}: {old_size / 1024:.0f} KB → {len(wasm_bytes) / 1024:.0f} KB")

        # Mettre à jour le zomes/ aussi (pour la cohérence)
        dest = zomes_dir / wasm_key
        dest.write_bytes(wasm_bytes)

    bundle["resources"] = resources

    # Sérialiser dans un buffer mémoire
    buf = io.BytesIO()
    data = msgpack.packb(bundle, use_bin_type=True)
    with gzip.GzipFile(fileobj=buf, mode="wb", compresslevel=6) as gz:
        gz.write(data)
    dna_bytes = buf.getvalue()

    # Écrire sur disque
    dna_path.write_bytes(dna_bytes)
    print(f"  ✓ {dna_path.name} ({len(dna_bytes) / 1024:.0f} KB)")

    return dna_bytes


# ── Repack .happ ─────────────────────────────────────────────────────────────


def repack_happ(dna_bytes_map: dict[str, bytes]) -> None:
    """
    Repacke ainonymous-core.happ avec les .dna mis à jour.
    `dna_bytes_map` : { "inference-mesh.dna": bytes, ... }
    """
    print(f"\n[HAPP] {HAPP_OUT.name}")

    bundle = load_bundle(HAPP_OUT)
    resources: dict = bundle.get("resources", {})

    for dna_key, dna_bytes in dna_bytes_map.items():
        old_size = len(resources.get(dna_key, b""))
        resources[dna_key] = dna_bytes
        print(f"  {dna_key}: {old_size / 1024:.0f} KB → {len(dna_bytes) / 1024:.0f} KB")

    bundle["resources"] = resources
    save_bundle(HAPP_OUT, bundle)


# ── Entrypoint ───────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="Repack ainonymous-core.happ")
    parser.add_argument("--dna-only", action="store_true",
                        help="Ne repacke que les .dna, pas le .happ final")
    args = parser.parse_args()

    dna_bytes_map: dict[str, bytes] = {}

    for name, cfg in DNAS.items():
        dna_bytes = repack_dna(name, cfg)
        dna_bytes_map[f"{name}.dna"] = dna_bytes

    if not args.dna_only:
        repack_happ(dna_bytes_map)

    print("\n✅ Repack terminé.")
    if not args.dna_only:
        print(f"   → {HAPP_OUT}")


if __name__ == "__main__":
    main()
