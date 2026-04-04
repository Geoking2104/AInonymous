# Spécification API — AInonymous

> API OpenAI-compatible exposée localement, routée vers le mesh Holochain.

---

## Endpoint Principal

```
http://localhost:9337/v1
```

Authentification : `Bearer ainonymous-local` (token local, validé par le daemon uniquement).
Pour l'accès réseau distant : membrane proof Holochain (clé cryptographique ed25519).

---

## Endpoints Disponibles

### `GET /v1/models`

Liste les modèles disponibles sur le mesh local et dans le réseau.

**Réponse**
```json
{
  "object": "list",
  "data": [
    {
      "id": "gemma4-31b",
      "object": "model",
      "created": 1735000000,
      "owned_by": "ainonymous-local",
      "meta": {
        "vram_required_gb": 20,
        "context_length": 131072,
        "multimodal": true,
        "architecture": "dense",
        "nodes_available": 2,
        "avg_latency_ms": 340
      }
    },
    {
      "id": "gemma4-26b-moe",
      "object": "model",
      "created": 1735000000,
      "owned_by": "ainonymous-mesh",
      "meta": {
        "vram_required_gb": 18,
        "context_length": 262144,
        "multimodal": true,
        "architecture": "moe",
        "active_params_b": 4,
        "nodes_available": 3,
        "avg_latency_ms": 210
      }
    },
    {
      "id": "gemma4-e4b",
      "object": "model",
      "created": 1735000000,
      "owned_by": "ainonymous-local",
      "meta": {
        "vram_required_gb": 5,
        "context_length": 131072,
        "multimodal": true,
        "architecture": "dense-edge",
        "nodes_available": 1,
        "avg_latency_ms": 45,
        "speculative_draft": true
      }
    }
  ]
}
```

---

### `POST /v1/chat/completions`

Inférence principale — compatible OpenAI Chat Completions.

**Requête (texte)**
```json
{
  "model": "gemma4-31b",
  "messages": [
    {"role": "system", "content": "Tu es un assistant utile."},
    {"role": "user", "content": "Explique le sharding MoE en 3 points."}
  ],
  "max_tokens": 1024,
  "temperature": 0.7,
  "top_p": 0.9,
  "stream": true,

  // Extensions AInonymous (optionnel)
  "ainonymous": {
    "execution_mode": "auto",         // "auto" | "solo" | "pipeline" | "expert_shard" | "speculative"
    "min_nodes": 1,
    "prefer_region": "eu-west",
    "speculative_draft_model": "gemma4-e4b",  // activer décodage spéculatif
    "blackboard_context": true         // injecter contexte Blackboard récent
  }
}
```

**Requête (multimodale — image)**
```json
{
  "model": "gemma4-31b",
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "Qu'est-ce que cette image représente ?"},
        {
          "type": "image_url",
          "image_url": {
            "url": "data:image/jpeg;base64,/9j/4AAQ...",
            "detail": "high"
          }
        }
      ]
    }
  ],
  "max_tokens": 512,
  "stream": false
}
```

**Réponse (non-stream)**
```json
{
  "id": "chatcmpl-ainon-abc123",
  "object": "chat.completion",
  "created": 1735000000,
  "model": "gemma4-31b",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Le sharding MoE consiste à distribuer..."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 45,
    "completion_tokens": 128,
    "total_tokens": 173
  },

  // Extensions AInonymous dans la réponse
  "ainonymous": {
    "execution_mode": "pipeline_split",
    "nodes_used": 2,
    "node_ids": ["hCAk...ABC", "hCAk...DEF"],
    "total_latency_ms": 340,
    "tokens_per_second": 37.6,
    "speculative_acceptance_rate": null
  }
}
```

**Réponse (stream — Server-Sent Events)**
```
data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","choices":[{"delta":{"role":"assistant"}}]}

data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","choices":[{"delta":{"content":"Le"}}]}

data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","choices":[{"delta":{"content":" sharding"}}]}

data: [DONE]
```

---

### `POST /v1/completions`

Completion texte brut (legacy OpenAI).

```json
{
  "model": "gemma4-31b",
  "prompt": "def fibonacci(n):",
  "max_tokens": 256,
  "temperature": 0.2,
  "stop": ["\n\n"]
}
```

---

### `POST /v1/embeddings`

Génération d'embeddings (si modèle embedding disponible dans le mesh).

```json
{
  "model": "nomic-embed-text",
  "input": ["Texte à encoder", "Autre texte"]
}
```

---

## Endpoints AInonymous Natifs

### `GET /v1/ainonymous/mesh/status`

État complet du mesh local.

```json
{
  "local_node": {
    "agent_id": "hCAk...XYZ",
    "status": "active",
    "vram_available_gb": 18.2,
    "loaded_models": ["gemma4-31b", "gemma4-e4b"],
    "current_load": 0.35,
    "requests_handled_24h": 142
  },
  "mesh": {
    "peers_connected": 7,
    "peers_active": 5,
    "total_vram_gb": 112.4,
    "requests_in_flight": 3,
    "avg_latency_ms": 280,
    "uptime_seconds": 86400
  },
  "blackboard": {
    "posts_last_24h": 89,
    "agents_active": 4
  }
}
```

### `GET /v1/ainonymous/mesh/nodes`

Liste détaillée des nœuds du mesh.

```json
{
  "nodes": [
    {
      "agent_id": "hCAk...ABC",
      "region": "eu-west",
      "vram_gb": 24.0,
      "gpu": "NVIDIA RTX 4090",
      "backend": "cuda",
      "models": ["gemma4-31b"],
      "load": 0.2,
      "latency_ms": 12,
      "uptime_hours": 48.5,
      "requests_24h": 423
    }
  ]
}
```

### `POST /v1/ainonymous/blackboard/post`

Poster sur le Blackboard directement via API REST.

```json
{
  "prefix": "STATUS",
  "content": "Analyse de codebase en cours, 3 fichiers restants",
  "tags": ["python", "analyse", "projet-x"],
  "ttl_hours": 48
}
```

### `GET /v1/ainonymous/blackboard/search`

```
GET /v1/ainonymous/blackboard/search?q=CUDA+OOM&prefix=FINDING&limit=10
```

**Réponse**
```json
{
  "posts": [
    {
      "id": "uhCEk...123",
      "prefix": "FINDING",
      "content": "FINDING: CUDA OOM sur gemma4-31b avec batch_size > 4",
      "tags": ["cuda", "oom", "gemma4"],
      "author_id": "hCAk...DEF",  // anonymisé si privacy_mode
      "created_at": 1735000000,
      "expires_at": 1735172800
    }
  ],
  "total": 1
}
```

### `POST /v1/ainonymous/models/pull`

Télécharger un modèle dans le mesh local.

```json
{
  "model_id": "gemma4-31b",
  "quantization": "q4_k_m",
  "source": "huggingface"  // "huggingface" | "local" | "mesh"
}
```

**Réponse (streaming du téléchargement)**
```
data: {"status": "downloading", "progress": 0.12, "speed_mbps": 45.2}
data: {"status": "downloading", "progress": 0.65, "speed_mbps": 52.1}
data: {"status": "verifying", "hash": "sha256:abc..."}
data: {"status": "ready", "model_id": "gemma4-31b", "size_gb": 20.1}
```

---

## Codes d'Erreur

| Code | Signification | Action |
|---|---|---|
| `400` | Requête malformée (model_id invalide, etc.) | Corriger les paramètres |
| `404` | Modèle non disponible dans le mesh | `pull` le modèle ou attendre un nœud |
| `429` | Mesh saturé, file d'attente pleine | Réessayer dans quelques secondes |
| `503` | Aucun nœud disponible pour ce modèle | Vérifier `GET /v1/ainonymous/mesh/nodes` |
| `507` | VRAM insuffisante pour ce modèle | Utiliser un modèle plus léger |

**Format d'erreur**
```json
{
  "error": {
    "message": "Aucun nœud disponible pour gemma4-31b (VRAM requise: 20GB, disponible: 16GB max)",
    "type": "mesh_unavailable",
    "code": "NO_CAPABLE_NODE",
    "ainonymous": {
      "available_nodes": 3,
      "max_available_vram_gb": 16.0,
      "suggested_model": "gemma4-26b-moe"
    }
  }
}
```

---

## Flux Interne : Requête → Holochain → Réponse

```
POST /v1/chat/completions
        │
        ▼
┌──────────────────┐
│  Proxy Local     │  Parse model_id, paramètres
│  (Rust HTTP)     │  Construit InferenceRequest
└────────┬─────────┘
         │ WebSocket
         ▼
┌──────────────────┐
│  Conducteur      │  call_zome("inference-mesh",
│  Holochain       │            "coordinator",
│  (local)         │            "compute_execution_plan")
└────────┬─────────┘
         │ DHT query
         ▼
┌──────────────────┐
│  DNA             │  query_available_nodes()
│  inference-mesh  │  → get_links(anchor("models","gemma4-31b"))
│  zome: router    │  → retourne plan d'exécution
└────────┬─────────┘
         │ call_remote() vers nœuds sélectionnés
         ▼
┌───────────────────────────────────────┐
│  Nœuds mesh                           │
│  Nœud A: couches 0-23                 │
│    └── llama-server :9337 local       │
│  Nœud B: couches 24-47               │
│    └── llama-server :9337 local       │
└───────────────────┬───────────────────┘
                    │ tokens + métriques
                    ▼
┌──────────────────┐
│  Proxy Local     │  Agrège tokens
│                  │  Stream SSE → client
│                  │  Publie InferenceMetrics
└──────────────────┘
```

---

## Rate Limiting

Le mesh AInonymous gère le rate limiting de manière décentralisée :

- Chaque nœud expose son `max_concurrent_requests` dans ses `NodeCapabilities`
- Le routeur répartit les requêtes en fonction de la charge (`current_load`)
- Si tous les nœuds sont saturés → HTTP 429 avec `Retry-After` en secondes
- Les requêtes en attente peuvent être mises en file via `"queue": true` dans les extensions AInonymous

```json
// Requête avec mise en file acceptée
{
  "model": "gemma4-31b",
  "messages": [...],
  "ainonymous": {
    "queue": true,
    "queue_timeout_seconds": 300
  }
}
```
