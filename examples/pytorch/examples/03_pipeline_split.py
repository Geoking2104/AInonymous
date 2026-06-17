"""
03_pipeline_split.py — Pipeline-splitting d'un modèle Gemma entre deux nœuds.

Démontre comment couper un modèle Hugging Face en deux et router les couches
supérieures vers un pair HybridNode via QUIC/mTLS.

Architecture:
    Local (Node A)           Remote (Node B)
    ─────────────────        ─────────────────
    Embedding                layers[18..36]
    layers[0..18]    ──▶     lm_head
                    QUIC
                   mTLS ed25519

Prérequis:
    pip install torch transformers ainonymous-torch
    # Node B doit tourner: hybridnode --config peer-b.yaml
    # et avoir chargé les couches 18-36 du même modèle

Usage:
    python examples/03_pipeline_split.py --node-b http://192.168.1.42:9337
"""

import argparse
import time
import torch
from typing import Optional

from ainonymous_torch import PipelineSplit, LayerRange

# ------------------------------------------------------------------
# Arguments
# ------------------------------------------------------------------
parser = argparse.ArgumentParser()
parser.add_argument("--node-b", default="http://localhost:9337",
                    help="URL du second nœud HybridNode")
parser.add_argument("--model", default="google/gemma-2-2b",
                    help="Modèle HuggingFace à splitter")
parser.add_argument("--split-at", type=int, default=9,
                    help="Index de couche où couper le modèle")
parser.add_argument("--device", default="cpu")
args = parser.parse_args()

print(f"Pipeline split: {args.model} — couche {args.split_at} — Node B: {args.node_b}")

# ------------------------------------------------------------------
# Chargement du modèle (côté Node A uniquement)
# ------------------------------------------------------------------
try:
    from transformers import AutoModelForCausalLM, AutoTokenizer
    print(f"Chargement {args.model} ...")
    tokenizer = AutoTokenizer.from_pretrained(args.model)
    model = AutoModelForCausalLM.from_pretrained(
        args.model,
        torch_dtype=torch.bfloat16,
        device_map=args.device,
    )
    model.eval()
    n_layers = len(model.model.layers)
    print(f"Modèle chargé: {n_layers} couches totales")

    # ------------------------------------------------------------------
    # Construction du PipelineSplit
    # ------------------------------------------------------------------
    local_range  = LayerRange(model.model.layers[:args.split_at], 0, args.split_at)
    remote_range = LayerRange(start=args.split_at, end=n_layers)

    pipe = PipelineSplit(
        local_layers=local_range,
        remote_url=args.node_b,
        remote_layers=remote_range,
        device=args.device,
        transfer_dtype=torch.float16,  # Économie bande passante
    )

    # ------------------------------------------------------------------
    # Inférence
    # ------------------------------------------------------------------
    prompt = "L'inférence distribuée P2P permet de"
    inputs = tokenizer(prompt, return_tensors="pt")
    input_ids = inputs["input_ids"].to(args.device)

    print(f"\nPrompt: '{prompt}'")
    print(f"Tokens: {input_ids.shape}")

    with torch.no_grad():
        # Embedding local
        hidden = model.model.embed_tokens(input_ids)
        # Couches 0..split_at (local) + split_at..N (remote via QUIC)
        t0 = time.perf_counter()
        hidden_out = pipe(hidden)
        t1 = time.perf_counter()
        # lm_head local
        logits = model.lm_head(model.model.norm(hidden_out))

    next_token_id = logits[0, -1].argmax()
    next_token = tokenizer.decode(next_token_id)

    print(f"\nToken prédit: '{next_token}'")
    print(f"Temps pipeline total: {(t1-t0)*1000:.0f}ms")
    print(f"Stats transfert: {pipe.transfer_stats}")

    # Estimation du gain
    local_params = sum(p.numel() for p in model.model.layers[:args.split_at].parameters())
    remote_params = sum(p.numel() for p in model.model.layers[args.split_at:].parameters())
    print(f"\nDistribution paramètres:")
    print(f"  Node A (couches 0-{args.split_at}): {local_params/1e9:.2f}B params")
    print(f"  Node B (couches {args.split_at}-{n_layers}): {remote_params/1e9:.2f}B params")

except ImportError:
    print("transformers non installé — démo avec modèle factice")

    # ------------------------------------------------------------------
    # Démo sans transformers — utilise un modèle PyTorch simple
    # ------------------------------------------------------------------
    import torch.nn as nn

    class SimpleTransformerLayer(nn.Module):
        def __init__(self, d_model=512, nhead=8):
            super().__init__()
            self.attn = nn.MultiheadAttention(d_model, nhead, batch_first=True)
            self.ff = nn.Sequential(nn.Linear(d_model, d_model*4), nn.GELU(), nn.Linear(d_model*4, d_model))
            self.norm1 = nn.LayerNorm(d_model)
            self.norm2 = nn.LayerNorm(d_model)
        def forward(self, x, **kwargs):
            attn_out, _ = self.attn(x, x, x)
            x = self.norm1(x + attn_out)
            x = self.norm2(x + self.ff(x))
            return (x,)

    n_total = 12
    split_at = 6
    layers = nn.ModuleList([SimpleTransformerLayer() for _ in range(n_total)])

    local_range  = LayerRange(layers[:split_at], 0, split_at)
    remote_range = LayerRange(start=split_at, end=n_total)

    pipe = PipelineSplit(
        local_layers=local_range,
        remote_url=args.node_b,
        remote_layers=remote_range,
        transfer_dtype=torch.float16,
    )

    x = torch.randn(1, 32, 512)
    print(f"Input: {x.shape}")
    print(f"Split: couches 0-{split_at} local, {split_at}-{n_total} remote ({args.node_b})")

    try:
        t0 = time.perf_counter()
        out = pipe(x)
        t1 = time.perf_counter()
        print(f"Output: {out.shape} en {(t1-t0)*1000:.0f}ms")
        print(f"Stats: {pipe.transfer_stats}")
    except Exception as e:
        print(f"Node B non joignable: {e}")
        print("→ En production, le HybridNode Scheduler sélectionne automatiquement")
        print("  un pair disponible via la topologie SD-WAN + DHT Holochain")
