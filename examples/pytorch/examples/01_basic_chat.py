"""
01_basic_chat.py — Chat avec AInonymous via l'API OpenAI-compatible.

Ce premier exemple montre l'intégration la plus simple : utiliser le client
HTTP d'ainonymous_torch pour envoyer des prompts au réseau HybridNode.
Le HybridNode sélectionne automatiquement le meilleur pair selon la topologie
SD-WAN et les capacités attestées dans le DHT Holochain.

Prérequis:
    pip install ainonymous-torch requests
    # HybridNode daemon démarré:
    hybridnode --config ainonymous.hybridnode.yaml

Usage:
    python examples/01_basic_chat.py
"""

from ainonymous_torch import AInonymousClient, InferenceOptions

# ------------------------------------------------------------------
# 1. Client de base — connexion au daemon local
# ------------------------------------------------------------------
client = AInonymousClient(
    base_url="http://localhost:9337",
    model="gemma4-31b",
)

print("=== Statut du réseau ===")
try:
    health = client.health()
    print(f"Daemon: {health.get('status', 'unknown')}")
    print(f"Holochain: {health.get('holochain_status', 'unknown')}")
    print(f"SD-WAN site: {health.get('site_id', 'unknown')}")

    nodes = client.list_nodes()
    print(f"\nPairs connus dans le DHT: {len(nodes)}")
    for node in nodes[:3]:
        warrant = " ⚠️ WARRANT" if node.has_warrant else ""
        print(f"  [{node.site_id}] rep={node.reputation:.2f} vram={node.vram_mb}MB "
              f"models={node.held_models}{warrant}")
except Exception as e:
    print(f"Daemon non joignable ({e}) — assurez-vous que HybridNode tourne.")
    print("Lancement: hybridnode --config ainonymous.hybridnode.yaml\n")

# ------------------------------------------------------------------
# 2. Chat simple
# ------------------------------------------------------------------
print("\n=== Chat simple ===")
reply = client.chat(
    "Explique le concept de pipeline-splitting pour les LLMs en 3 phrases.",
    system="Tu es un expert en inférence distribuée. Sois concis.",
)
print(f"Réponse: {reply}")

# ------------------------------------------------------------------
# 3. Chat avec options de routage HybridNode
# ------------------------------------------------------------------
print("\n=== Chat avec contraintes de routage ===")
options = InferenceOptions(
    max_latency_ms=20,           # Seulement des pairs à moins de 20ms (intra-site)
    redundancy_mode="primary_shadow",  # Redondance PrimaryShadow
    require_attestation=True,    # Pairs attestés uniquement (NodeAttestation valide)
)
reply = client.chat(
    "Quels sont les avantages du mTLS ed25519 pour l'inférence P2P ?",
    options=options,
)
print(f"Réponse (routage strict): {reply}")

# ------------------------------------------------------------------
# 4. Streaming
# ------------------------------------------------------------------
print("\n=== Streaming (tokens en temps réel) ===")
print("Réponse: ", end="", flush=True)
for token in client.stream_chat(
    "Décris l'architecture Holochain DHT en moins de 50 mots.",
    max_tokens=100,
):
    print(token, end="", flush=True)
print()

# ------------------------------------------------------------------
# 5. Embeddings
# ------------------------------------------------------------------
print("\n=== Embeddings ===")
texts = [
    "AInonymous réseau décentralisé Holochain",
    "PyTorch pipeline splitting distributed inference",
    "SD-WAN QoS DSCP topology aware scheduling",
]
embeddings = client.embed(texts)
print(f"Embeddings: {len(embeddings)} vecteurs × {len(embeddings[0])} dimensions")

# Similarité cosinus entre les deux premiers
import math
def cosine_sim(a, b):
    dot = sum(x*y for x,y in zip(a,b))
    na = math.sqrt(sum(x**2 for x in a))
    nb = math.sqrt(sum(x**2 for x in b))
    return dot / (na * nb) if na and nb else 0.0

sim = cosine_sim(embeddings[0], embeddings[1])
print(f"Similarité (Holochain ↔ PyTorch): {sim:.3f}")
