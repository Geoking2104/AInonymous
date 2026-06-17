"""
02_activation_transfer.py — Sérialisation de tenseurs PyTorch pour QUIC.

Montre comment sérialiser/désérialiser un tenseur d'activation (hidden state)
dans le format binaire d'AInonymous, compatible avec le transfer QUIC/mTLS.

Ce format est utilisé lorsqu'un modèle est split entre deux nœuds HybridNode:
  Node A → [couches 0-16] → hidden_state → QUIC → Node B → [couches 16-32]

Prérequis:
    pip install torch ainonymous-torch
"""

import time
import torch

from ainonymous_torch import pack_activation, unpack_activation, ActivationTransfer

# ------------------------------------------------------------------
# 1. Exemple de base — float32
# ------------------------------------------------------------------
print("=== 1. Pack/Unpack float32 ===")

# Simulation d'un hidden state: batch=2, seq_len=512, hidden_dim=4096 (Gemma 9B)
hidden = torch.randn(2, 512, 4096, dtype=torch.float32)
print(f"Tensor original: shape={hidden.shape} dtype={hidden.dtype} "
      f"size={hidden.numel() * 4 / 1024**2:.1f} MB")

t0 = time.perf_counter()
payload = pack_activation(hidden)
t1 = time.perf_counter()
print(f"Sérialisé: {len(payload) / 1024**2:.1f} MB en {(t1-t0)*1000:.1f}ms")

restored = unpack_activation(payload)
t2 = time.perf_counter()
print(f"Désérialisé en {(t2-t1)*1000:.1f}ms")
print(f"Identique: {torch.allclose(hidden, restored)}")

# ------------------------------------------------------------------
# 2. bfloat16 — économise 50% de bande passante
# ------------------------------------------------------------------
print("\n=== 2. Pack/Unpack bfloat16 (×2 économie réseau) ===")
hidden_bf16 = hidden.to(torch.bfloat16)
payload_bf16 = pack_activation(hidden_bf16)
restored_bf16 = unpack_activation(payload_bf16)

print(f"bfloat16 payload: {len(payload_bf16) / 1024**2:.1f} MB "
      f"(vs {len(payload) / 1024**2:.1f} MB float32)")
print(f"Dtype préservé: {restored_bf16.dtype}")
# Note: bfloat16 n'est pas exactement égal à float32 — c'est attendu
print(f"Max diff (bf16 vs f32): {(hidden - restored_bf16.float()).abs().max().item():.4f}")

# ------------------------------------------------------------------
# 3. Chunking pour grandes activations
# ------------------------------------------------------------------
print("\n=== 3. Chunking automatique (large batch) ===")
large = torch.randn(16, 2048, 4096, dtype=torch.float16)  # ~256 MB
print(f"Tensor large: {large.numel() * 2 / 1024**2:.0f} MB")

transfer = ActivationTransfer(max_chunk_mb=64)  # Chunks de 64 MB max
chunks = transfer.split(large)
print(f"Découpé en {len(chunks)} chunks de ≤ 64 MB")

reconstructed = transfer.merge(chunks)
print(f"Reconstruit: shape={reconstructed.shape} identique={torch.allclose(large, reconstructed)}")
print(f"Stats: {transfer.stats}")

# ------------------------------------------------------------------
# 4. Simulation d'un transfert inter-nœuds
# ------------------------------------------------------------------
print("\n=== 4. Simulation pipeline Node A → Node B ===")

# Node A: couches 0-16 d'un modèle transformeur
class FakeLayerBlock(torch.nn.Module):
    def __init__(self, hidden_dim, n_layers):
        super().__init__()
        self.layers = torch.nn.ModuleList([
            torch.nn.Linear(hidden_dim, hidden_dim) for _ in range(n_layers)
        ])
    def forward(self, x):
        for layer in self.layers:
            x = torch.relu(layer(x))
        return x

hidden_dim = 256  # Petit pour la démo
node_a = FakeLayerBlock(hidden_dim, n_layers=4)
node_b = FakeLayerBlock(hidden_dim, n_layers=4)

input_tensor = torch.randn(1, 32, hidden_dim)
print(f"Input: {input_tensor.shape}")

# Node A: forward + sérialisation
t0 = time.perf_counter()
with torch.no_grad():
    activation_a = node_a(input_tensor)
wire_bytes = pack_activation(activation_a.to(torch.bfloat16))
t_send = time.perf_counter()
print(f"Node A → {len(wire_bytes)} bytes — sérialisé en {(t_send-t0)*1000:.1f}ms")

# Simulation latence réseau (intra-site ≈ 1ms)
import time; time.sleep(0.001)

# Node B: désérialisation + forward
activation_b_in = unpack_activation(wire_bytes).float()  # bfloat16 → float32
with torch.no_grad():
    output = node_b(activation_b_in)
t_end = time.perf_counter()
print(f"Node B → output={output.shape} en {(t_end-t_send)*1000:.1f}ms")
print(f"Latence totale pipeline (sans réseau): {(t_end-t0)*1000:.1f}ms")
