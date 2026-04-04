# Patch llama.cpp — Pipeline Split natif

## Contexte

llama.cpp exécute toujours le modèle complet, de l'embedding (`llm_build_inp_embd`)
jusqu'aux logits (`lm_head`). Il n'existe pas d'API publique pour :
- Injecter des hidden states à une couche arbitraire
- Interrompre l'exécution après une couche donnée et exporter les activations

Le `pipeline_server.py` (HuggingFace transformers) est la solution opérationnelle
**aujourd'hui**. Ce document décrit le patch llama.cpp natif pour la **production**,
qui apportera :
- Inférence en GGUF quantisé (Q4_K_M, Q8_0) → ~2-4× moins de VRAM
- Pas de dépendance PyTorch / transformers sur les workers
- Latence réduite (pas de Python, pas de conversion de format)
- Support Metal (macOS), CUDA, Vulkan natif

---

## Architecture du patch

### 1. Nouvelles structures C

```c
// include/llama.h  (nouvelles fonctions publiques)

// Résultat d'un forward pass partiel
typedef struct llama_pipeline_result {
    // hidden states sérialisés [seq_len × hidden_size × sizeof(float16_t)]
    float16_t * hidden_states;
    int64_t     hidden_states_size;   // bytes
    int32_t     seq_len;
    int32_t     hidden_size;
    // KV-cache snapshot (opaque, utilisé dans les passes decode suivantes)
    struct llama_kv_cache_view * kv_snapshot;
} llama_pipeline_result;

// Paramètres pour un forward pass partiel
typedef struct llama_pipeline_params {
    int32_t layer_start;    // première couche à exécuter (inclusive)
    int32_t layer_end;      // dernière couche (exclusive)
    bool    is_first_node;  // si true : exécuter embed_tokens avant layer_start
    bool    is_last_node;   // si true : exécuter norm + lm_head après layer_end
    // hidden states entrants (ignoré si is_first_node)
    const float16_t * input_hidden_states;
    int64_t           input_hidden_states_size;
} llama_pipeline_params;
```

### 2. Nouvelles fonctions publiques

```c
// Exécuter un forward pass partiel (prefill)
LLAMA_API llama_pipeline_result * llama_pipeline_forward(
    struct llama_context * ctx,
    const llama_pipeline_params * params,
    const llama_token * tokens,   // utilisé si is_first_node
    int32_t n_tokens
);

// Exécuter un decode step (1 token, utilise le KV-cache existant)
LLAMA_API llama_pipeline_result * llama_pipeline_decode_step(
    struct llama_context * ctx,
    const llama_pipeline_params * params,
    llama_token last_token,       // utilisé si is_first_node
    const llama_pipeline_result * prev_result  // pour le KV-cache
);

// Libérer le résultat
LLAMA_API void llama_pipeline_result_free(llama_pipeline_result * result);
```

### 3. Modifications de `llama.cpp` (fichier principal)

**Modification de `llm_build_context` / `llm_build_llama` :**

```cpp
// Dans llama.cpp, fonction llm_build_llama() (ou équivalent Gemma4)
// Ajouter les paramètres de tranche :

struct llm_build_context {
    // ... champs existants ...

    // NOUVEAU : tranche de couches
    int32_t pipeline_layer_start = 0;
    int32_t pipeline_layer_end   = -1;  // -1 = toutes
    bool    pipeline_inject_hidden = false;
    ggml_tensor * pipeline_input_hidden = nullptr;  // hidden states entrants
};
```

**Modification de la boucle des couches :**

```cpp
// Dans llm_build_llama (simplifié)
for (int il = 0; il < n_layer; ++il) {

    // NOUVEAU : skip des couches hors tranche
    if (il < hparams.pipeline_layer_start) continue;
    if (hparams.pipeline_layer_end >= 0 && il >= hparams.pipeline_layer_end) break;

    // ... code existant des couches ...
}
```

**Injection des hidden states entrants :**

```cpp
// Avant la boucle des couches, si pipeline_inject_hidden :
ggml_tensor * inpL;
if (ctx->pipeline_inject_hidden && ctx->pipeline_input_hidden) {
    // Utiliser les hidden states entrants au lieu de l'embedding
    inpL = ggml_dup_tensor(ctx0, ctx->pipeline_input_hidden);
} else {
    // Chemin normal : embedding lookup
    inpL = llm_build_inp_embd(ctx0, lctx, hparams, ubatch, model.tok_embd, cb);
}
```

**Export des hidden states sortants :**

```cpp
// Après la boucle des couches, si pas is_last_node :
if (!is_last_node) {
    // Ne pas exécuter norm + lm_head
    // Exporter cur (les hidden states après la dernière couche traitée)
    cb(cur, "pipeline_output_hidden", -1);
    ggml_build_forward_expand(gf, cur);
    // ctx->pipeline_output_hidden = cur;  (stocké pour récupération)
    return gf;  // arrêt avant norm/lm_head
}
```

### 4. Endpoint llama-server additionnel

```cpp
// Dans llama-server (examples/server/server.cpp)
// Ajouter route : POST /v1/pipeline/forward

svr.Post("/v1/pipeline/forward", [&](const httplib::Request & req,
                                     httplib::Response & res) {
    json body = json::parse(req.body);

    llama_pipeline_params pparams = {
        .layer_start = body["layer_start"],
        .layer_end   = body["layer_end"],
        .is_first_node = body.value("is_first_node", false),
        .is_last_node  = body.value("is_last_node", false),
    };

    // Décoder les hidden states entrants depuis base64
    if (!pparams.is_first_node) {
        std::string b64 = body["hidden_states_b64"];
        pparams.input_hidden_states = base64_decode_fp16(b64);
        pparams.input_hidden_states_size = /* ... */;
    }

    // Tokens d'entrée (si premier nœud)
    std::vector<llama_token> tokens;
    if (pparams.is_first_node) {
        for (auto id : body["input_ids"]) tokens.push_back(id);
    }

    auto * result = llama_pipeline_forward(ctx, &pparams,
                                           tokens.data(), tokens.size());
    if (!result) {
        res.status = 500;
        return;
    }

    json resp;
    resp["request_id"] = body["request_id"];
    resp["seq_len"]    = result->seq_len;
    resp["hidden_size"] = result->hidden_size;

    if (pparams.is_last_node) {
        // Retourner le token généré
        resp["next_token_id"] = /* sample from logits */;
    } else {
        // Encoder les hidden states en base64
        resp["hidden_states_b64"] = base64_encode(
            result->hidden_states,
            result->hidden_states_size
        );
    }

    llama_pipeline_result_free(result);
    res.set_content(resp.dump(), "application/json");
});
```

---

## Modèles impactés

Le patch doit couvrir toutes les architectures utilisées par Gemma 4 :

| Architecture  | Fichier llama.cpp        | Couches                   |
|---------------|--------------------------|---------------------------|
| Gemma4 Dense  | `llm_build_gemma3()`     | `GemmaDecoderLayer × N`   |
| Gemma4 MoE    | `llm_build_gemma3_moe()` | Mix MLP dense + experts   |

Les fonctions `llm_build_*` sont dans `src/llama.cpp` (>50k lignes).
Le patch cible les boucles `for (int il = 0; il < n_layer; ++il)`.

---

## Compatibilité GGUF

Les quantifications concernées (utilisées en prod) :

| Format  | Précision  | Notes                          |
|---------|------------|--------------------------------|
| Q4_K_M  | ~4.5 bits  | Recommandé pour les workers    |
| Q8_0    | 8 bits     | Draft nodes, premier/dernier   |
| F16     | 16 bits    | Full precision (validation)    |

Les hidden states exportés sont **toujours en F16** indépendamment de la
quantification interne, pour garder la précision entre nœuds.

---

## Plan d'implémentation

```
Semaine 1 :
  - Fork llama.cpp (ggml-org/llama.cpp)
  - Ajouter llama_pipeline_params + llama_pipeline_result dans include/llama.h
  - Implémenter llama_pipeline_forward() pour Gemma4 Dense

Semaine 2 :
  - Étendre à Gemma4 MoE (llm_build_gemma3_moe)
  - Ajouter llama_pipeline_decode_step()
  - Tests unitaires : vérifier que full_model == pipeline(node0) + pipeline(node1)

Semaine 3 :
  - Endpoint llama-server POST /v1/pipeline/forward
  - Benchmark latence vs pipeline_server.py Python
  - Migration conductor.rs : swapper PipelineClient HTTP vers nouveau endpoint
```

---

## Transition depuis pipeline_server.py

Une fois le patch disponible, la migration dans `conductor.rs` se fait en
changeant uniquement `PipelineClient` :

```rust
// Avant (Python server)
let pipeline = PipelineClient::new(config.pipeline_server_port);

// Après (llama-server natif)
let pipeline = LlamaPipelineClient::new(config.llama_server_port);
```

L'interface HTTP est identique (`/prefill`, `/decode`, `/clear`) — seul le
backend change. Les hidden states format F16 + base64 restent la convention
de sérialisation dans les deux cas.

---

## Référence

- Boucle couches Gemma : `src/llama.cpp` → `llm_build_gemma3()`
- Injection embeddings : `llm_build_inp_embd()` dans `src/llama.cpp`
- KV-cache API : `llama_kv_cache_view`, `llama_kv_cache_seq_rm()`
- Précédent : `--embedding` mode (extraction de la couche finale seulement,
  insuffisant pour le pipeline split mais bon point de départ)
