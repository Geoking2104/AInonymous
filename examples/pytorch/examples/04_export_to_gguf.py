"""
04_export_to_gguf.py — Export PyTorch → GGUF + attestation Holochain.

Workflow complet:
  1. Convertir un modèle HuggingFace en GGUF (via llama.cpp)
  2. Quantifier (Q4_K_M recommandé — bon compromis taille/qualité)
  3. Calculer le SHA-256 → ModelManifest
  4. Vérifier l'intégrité avant service
  5. (Optionnel) Publier le manifest dans le DHT Holochain via AInonymous

Prérequis:
    pip install torch transformers ainonymous-torch
    git clone https://github.com/ggerganov/llama.cpp
    cd llama.cpp && cmake -B build && cmake --build build --config Release

Usage:
    python examples/04_export_to_gguf.py \\
        --model google/gemma-2-2b \\
        --output ~/.ainonymous/models \\
        --quant Q4_K_M \\
        --llama-cpp ~/llama.cpp
"""

import argparse
import json
import os
from pathlib import Path

from ainonymous_torch.model_export import export_to_gguf, verify_gguf, ModelManifest

parser = argparse.ArgumentParser(description="Export PyTorch → GGUF pour AInonymous")
parser.add_argument("--model",    default="google/gemma-2-2b", help="HF model ID ou chemin local")
parser.add_argument("--output",   default="~/.ainonymous/models", help="Dossier de sortie")
parser.add_argument("--quant",    default="Q4_K_M", help="Type de quantification (Q4_K_M, Q8_0, F16, none)")
parser.add_argument("--llama-cpp",default=os.environ.get("LLAMA_CPP_DIR", "~/llama.cpp"))
parser.add_argument("--layers",   type=int, default=0, help="Nombre de couches (0=auto)")
parser.add_argument("--arch",     default="gemma2", help="Architecture (pour le manifest)")
parser.add_argument("--publish",  action="store_true", help="Publier le manifest dans le DHT Holochain")
parser.add_argument("--ainonymous-url", default="http://localhost:9337")
args = parser.parse_args()

print("=" * 60)
print("AInonymous — Export PyTorch → GGUF + Attestation")
print("=" * 60)
print(f"Modèle   : {args.model}")
print(f"Sortie   : {args.output}")
print(f"Quant    : {args.quant}")
print(f"llama.cpp: {args.llama_cpp}")
print()

# ------------------------------------------------------------------
# 1. Export GGUF
# ------------------------------------------------------------------
try:
    manifest = export_to_gguf(
        model_id=args.model,
        output_dir=args.output,
        quantization=None if args.quant.lower() == "none" else args.quant,
        llama_cpp_dir=args.llama_cpp,
        num_layers=args.layers,
        architecture=args.arch,
    )

    print("\n" + "=" * 60)
    print("✓ Export réussi")
    print("=" * 60)
    print(f"Fichier  : {manifest.gguf_path}")
    print(f"Taille   : {manifest.size_bytes / 1024**3:.2f} GB")
    print(f"SHA-256  : {manifest.sha256}")
    print(f"Couches  : {manifest.num_layers}")

    # ------------------------------------------------------------------
    # 2. Vérification intégrité
    # ------------------------------------------------------------------
    print("\n[Vérification SHA-256...]")
    ok = verify_gguf(manifest.gguf_path, manifest.sha256)
    print(f"Intégrité: {'✓ OK' if ok else '✗ ÉCHEC — ne pas servir ce modèle!'}")

    # ------------------------------------------------------------------
    # 3. Afficher le manifest (format Holochain)
    # ------------------------------------------------------------------
    print("\n[ModelManifest pour Holochain DHT]")
    print(json.dumps(manifest.to_dict(), indent=2))

    # ------------------------------------------------------------------
    # 4. Publier dans le DHT Holochain (optionnel)
    # ------------------------------------------------------------------
    if args.publish:
        print("\n[Publication dans le DHT Holochain...]")
        import requests
        try:
            resp = requests.post(
                f"{args.ainonymous_url}/v1/holochain/publish_manifest",
                json=manifest.to_dict(),
                timeout=10,
            )
            resp.raise_for_status()
            action_hash = resp.json().get("action_hash")
            print(f"✓ ModelManifest publié — action_hash: {action_hash}")
            print("  Les pairs peuvent maintenant vérifier ce modèle via le DHT.")

            # Claim: ce nœud sert toutes les couches
            claim_resp = requests.post(
                f"{args.ainonymous_url}/v1/holochain/claim_model",
                json={
                    "manifest_hash": action_hash,
                    "layer_range": [0, manifest.num_layers],
                },
                timeout=10,
            )
            claim_resp.raise_for_status()
            print(f"✓ ModelClaim publié — couches [0, {manifest.num_layers}]")

        except requests.exceptions.ConnectionError:
            print("  ℹ Daemon non joignable — publication ignorée (mode hors-ligne)")
        except Exception as e:
            print(f"  ⚠ Erreur publication: {e}")

    print("\n[Prochaine étape]")
    print(f"  Démarrer llama.cpp avec ce modèle:")
    print(f"  llama-server -m {manifest.gguf_path} --port 9337 -ngl 99")

except FileNotFoundError as e:
    print(f"\n⚠ {e}")
    print("\nPour installer llama.cpp:")
    print("  git clone https://github.com/ggerganov/llama.cpp")
    print("  cd llama.cpp && cmake -B build -DLLAMA_CUDA=ON && cmake --build build -j")
    print("  export LLAMA_CPP_DIR=$(pwd)")

except Exception as e:
    print(f"\n✗ Erreur: {e}")
    raise
