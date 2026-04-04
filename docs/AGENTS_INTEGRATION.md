# Intégration Agents — Goose + Gemma 4

> Comment les agents IA s'intègrent dans le mesh AInonymous via MCP et Holochain.

---

## 1. Goose dans AInonymous

### Rôle de Goose

[Goose](https://github.com/block/goose) (Block Open Source, Apache 2.0) est l'orchestrateur d'agents dans AInonymous. Il remplace le système d'orchestration centralisé en fonctionnant directement sur le mesh Holochain via MCP.

```
Goose ←→ MCP Server AInonymous ←→ Conducteur Holochain ←→ Mesh LLM
```

Goose apporte :
- Exécution autonome de tâches complexes (code, fichiers, API, shell)
- Support natif MCP → connexion aux zomes Holochain comme "outils"
- Multi-LLM configurable → Gemma 4 en local, fallback cloud si nécessaire
- Mode équipe (GooseTeam) : plusieurs instances Goose collaborant via Blackboard

### Configuration de Goose pour AInonymous

**Fichier : `~/.config/goose/config.yaml`**

```yaml
# Configuration AInonymous pour Goose
provider: openai-compatible
model: gemma4-31b
base_url: http://localhost:9337/v1
api_key: "ainonymous-local"   # clé factice, auth gérée par Holochain

# Modèles disponibles par tâche
profiles:
  fast:
    model: gemma4-e4b          # rapide, léger, 5GB VRAM
    base_url: http://localhost:9337/v1
  standard:
    model: gemma4-26b-moe      # qualité/vitesse équilibré
    base_url: http://localhost:9337/v1
  powerful:
    model: gemma4-31b          # meilleure qualité, multi-nœuds
    base_url: http://localhost:9337/v1

# Serveurs MCP connectés
extensions:
  - name: ainonymous-mesh
    type: stdio
    cmd: ainonymous
    args: ["mcp"]
    description: "Accès aux capacités du mesh AInonymous via Holochain"
  - name: ainonymous-blackboard
    type: stdio
    cmd: ainonymous
    args: ["mcp", "--dna", "blackboard"]
    description: "Blackboard partagé pour collaboration d'agents"
```

### Lancement

```bash
# Démarrer le mesh et Goose ensemble
ainonymous goose

# Avec profil spécifique
ainonymous goose --profile powerful

# Démarrer plusieurs agents en mode équipe
ainonymous goose --team --agents 3 --blackboard
```

---

## 2. Serveur MCP AInonymous

Le serveur MCP expose les capacités Holochain comme outils Goose.

### Outils MCP disponibles

```json
{
  "tools": [
    {
      "name": "mesh_query_nodes",
      "description": "Lister les nœuds disponibles dans le mesh pour un modèle donné",
      "inputSchema": {
        "type": "object",
        "properties": {
          "model_id": {"type": "string", "description": "ex: gemma4-31b"},
          "min_vram_gb": {"type": "number"},
          "region": {"type": "string"}
        },
        "required": ["model_id"]
      }
    },
    {
      "name": "mesh_run_inference",
      "description": "Exécuter une inférence LLM sur le mesh distribué",
      "inputSchema": {
        "type": "object",
        "properties": {
          "model_id": {"type": "string"},
          "prompt": {"type": "string"},
          "max_tokens": {"type": "integer", "default": 2048},
          "temperature": {"type": "number", "default": 0.7},
          "stream": {"type": "boolean", "default": false}
        },
        "required": ["model_id", "prompt"]
      }
    },
    {
      "name": "blackboard_post",
      "description": "Publier un message sur le blackboard partagé des agents",
      "inputSchema": {
        "type": "object",
        "properties": {
          "prefix": {"type": "string", "enum": ["STATUS", "FINDING", "QUESTION", "TIP", "DONE"]},
          "content": {"type": "string", "maxLength": 4096},
          "tags": {"type": "array", "items": {"type": "string"}},
          "ttl_hours": {"type": "integer", "default": 48}
        },
        "required": ["prefix", "content"]
      }
    },
    {
      "name": "blackboard_search",
      "description": "Rechercher dans le blackboard partagé des agents",
      "inputSchema": {
        "type": "object",
        "properties": {
          "terms": {"type": "array", "items": {"type": "string"}, "description": "Termes OR"},
          "prefix_filter": {"type": "string", "enum": ["STATUS", "FINDING", "QUESTION", "TIP", "DONE"]},
          "limit": {"type": "integer", "default": 20}
        },
        "required": ["terms"]
      }
    },
    {
      "name": "mesh_get_metrics",
      "description": "Obtenir les métriques de performance du mesh (latence, débit, nœuds actifs)",
      "inputSchema": {
        "type": "object",
        "properties": {
          "time_window_minutes": {"type": "integer", "default": 60}
        }
      }
    }
  ]
}
```

### Implémentation du serveur MCP (Rust)

```rust
// crates/ainonymous-mcp/src/main.rs

use mcp_sdk::{Server, Tool, ToolResult};
use holochain_client::AppWebsocket;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connexion au conducteur Holochain local
    let client = AppWebsocket::connect("ws://localhost:8888").await?;

    let server = Server::new("ainonymous-mesh", "1.0.0")
        .tool(mesh_query_nodes_tool(&client))
        .tool(mesh_run_inference_tool(&client))
        .tool(blackboard_post_tool(&client))
        .tool(blackboard_search_tool(&client))
        .tool(mesh_get_metrics_tool(&client));

    server.run_stdio().await
}

fn blackboard_post_tool(client: &AppWebsocket) -> Tool {
    Tool::new("blackboard_post", |params: BlackboardPostParams| async {
        let result = client
            .call_zome(ZomeCallInput {
                cell_id: get_blackboard_cell_id()?,
                zome_name: "coordinator".into(),
                fn_name: "post".into(),
                payload: encode(&params)?,
                ..Default::default()
            })
            .await?;
        Ok(ToolResult::text(format!("Publié: {:?}", result)))
    })
}
```

---

## 3. Gemma 4 — Intégration Technique

### Modèles et Cas d'Usage

| Modèle | VRAM (4-bit) | Contexte | Cas d'usage dans AInonymous |
|---|---|---|---|
| `gemma4-e2b` | ~3 GB | 128K tokens | Nœuds CPU uniquement, draft spéculatif léger |
| `gemma4-e4b` | ~5 GB | 128K tokens | Draft spéculatif, nœuds GPU entrée de gamme |
| `gemma4-26b-moe` | ~18 GB | 256K tokens | Sharding experts MoE, inférence qualitative rapide |
| `gemma4-31b` | ~20 GB | 256K tokens | Pipeline-split, haute qualité, 2-3 nœuds |

### Quantizations recommandées

```
Gemma4-31B pour usage réseau :
  Q8_0     → 34 GB  (haute qualité, nœuds 40GB+ VRAM)
  Q4_K_M   → 20 GB  (recommandé, balance qualité/mémoire)
  Q3_K_M   → 15 GB  (nœuds 16GB VRAM)

Gemma4-26B-MoE :
  Q8_0     → 28 GB
  Q4_K_M   → 18 GB  (recommandé)

Gemma4-E4B (draft spéculatif) :
  Q8_0     → 6 GB
  Q4_K_M   → 4 GB   (recommandé)

Gemma4-E2B (CPU) :
  Q8_0     → 4 GB
  Q4_0     → 2 GB   (CPUs anciens)
```

### Téléchargement automatique

```bash
# Via CLI ainonymous
ainonymous model pull gemma4-31b               # Q4_K_M par défaut
ainonymous model pull gemma4-31b --quant q8_0  # Forcer Q8
ainonymous model pull gemma4-26b-moe           # MoE automatiquement détecté

# Depuis HuggingFace directement
ainonymous model pull unsloth/gemma-4-31B-it-GGUF
ainonymous model pull unsloth/gemma-4-26B-A4B-it-GGUF

# Depuis chemin local
ainonymous model add /path/to/gemma4-31b-q4_k_m.gguf
```

### Configuration llama-server pour Gemma 4

```bash
# Démarrage automatique par le daemon ainonymous
llama-server \
  --model ~/.models/gemma4-31b-q4_k_m.gguf \
  --port 9337 \
  --host 127.0.0.1 \
  --ctx-size 65536 \
  --n-predict -1 \
  --rope-scaling yarn \       # Gemma 4 utilise YaRN pour long contexte
  --rope-freq-scale 0.25 \
  --n-gpu-layers 99 \         # Toutes les couches sur GPU si possible
  --threads 8 \
  --flash-attn \              # Flash Attention 2 activée
  --cache-type-k q8_0 \       # KV cache quantisé (économie mémoire)
  --cache-type-v q8_0 \
  --parallel 4                # 4 requêtes simultanées max
```

### Sharding MoE Gemma 4-26B

Gemma 4-26B est une architecture MoE avec ~4B paramètres actifs par token. Pour le distribuer sur plusieurs nœuds dans AInonymous :

```
Architecture Gemma4-26B-MoE :
  Couches totales   : 30 blocs Transformer
  Paramètres totaux : 26B
  Paramètres actifs : ~4B par token (routage sparse)
  Experts par bloc  : N experts sparse + 1 expert dense

Stratégie AInonymous :
  ┌─────────────────────────────────────────────────────┐
  │ Tous les nœuds portent : tronc dense (embedding,    │
  │ attention, norm layers)                             │
  │                                                     │
  │ Nœud A (18GB) : experts 0-N/2 des blocs 0-14       │
  │ Nœud B (18GB) : experts N/2-N des blocs 0-14       │
  │ Nœud C (18GB) : tous experts des blocs 15-29        │
  └─────────────────────────────────────────────────────┘

  Token routing : le routeur MoE détermine quels experts
  activer → le zome "router" dispatche vers le bon nœud
  → résultat agrégé par le nœud coordinateur
```

### Capacités Multimodales Gemma 4

Gemma 4 supporte nativement images + texte (et audio sur E2B/E4B). AInonymous expose cela via l'API :

```json
// Requête multimodale
{
  "model": "gemma4-31b",
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "type": "image_url",
          "image_url": {
            "url": "data:image/jpeg;base64,/9j/4AAQ..."
          }
        },
        {
          "type": "text",
          "text": "Décris cette image en détail."
        }
      ]
    }
  ],
  "max_tokens": 1024
}
```

Le proxy local encode l'image en base64 et la transmet à llama-server qui gère nativement la modalité via le format GGUF multimodal de Gemma 4.

---

## 4. Mode Équipe GooseTeam sur AInonymous

Plusieurs instances Goose peuvent collaborer via le Blackboard Holochain.

### Workflow multi-agents

```
Tâche : "Analyser ce codebase Python et générer des tests unitaires"

Agent A (Coordinator)
  └── blackboard_post("STATUS: analyse codebase en cours", tags=["python", "analyse"])
  └── découpe la tâche en sous-tâches
  └── assigne via blackboard :
      blackboard_post("QUESTION: qui peut analyser src/utils.py ?", tags=["python"])

Agent B (disponible, gemma4-31b)
  └── blackboard_search(["utils.py", "python"]) → trouve la question
  └── blackboard_post("STATUS: je prends utils.py", tags=["python", "utils"])
  └── analyse utils.py
  └── blackboard_post("FINDING: 3 fonctions sans tests, complexité cyclomatique élevée",
                       tags=["python", "utils", "findings"])
  └── génère tests → blackboard_post("DONE: tests utils.py générés", tags=["python", "done"])

Agent C (disponible, gemma4-26b-moe)
  └── prend src/models.py
  └── suit le même pattern
  └── blackboard_search(["FINDING"]) pour éviter la duplication

Agent A (Coordinator)
  └── blackboard_search(["DONE"]) toutes les 30s
  └── agrège les résultats quand tous les "DONE" reçus
  └── blackboard_post("DONE: tous les tests générés, PR créée", tags=["final"])
```

### Configuration GooseTeam

```yaml
# ~/.config/goose/team.yaml
team:
  name: "ainonymous-dev-team"
  coordinator:
    profile: powerful          # gemma4-31b pour la coordination
  workers:
    count: 3
    profile: standard          # gemma4-26b-moe pour les tâches
  blackboard:
    dna: blackboard            # DNA Holochain utilisée
    ttl_hours: 4               # Posts expirent après 4h
    sync_interval_seconds: 15  # Vérification des mises à jour

# Skill pour Goose : installer automatiquement le connecteur blackboard
# ainonymous blackboard install-skill
```

---

## 5. Compatibilité avec Autres Clients

L'API OpenAI-compatible sur `localhost:9337` fonctionne avec tous les clients mesh-llm existants :

```bash
# Tous ces clients fonctionnent sans modification
ainonymous goose          # Goose (recommandé)
ainonymous opencode       # opencode
ainonymous claude         # Claude Code

# Variables d'environnement pour clients custom
export OPENAI_API_KEY=ainonymous-local
export OPENAI_BASE_URL=http://localhost:9337/v1

# Test rapide
curl $OPENAI_BASE_URL/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -d '{"model":"gemma4-31b","messages":[{"role":"user","content":"ping"}]}'
```
