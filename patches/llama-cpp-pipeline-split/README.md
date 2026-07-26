# Pipeline-split patch for llama.cpp — Gemma3 Dense (Preuve de concept vérifiée)

Statut : **mécanisme prouvé, réel et compilé, y compris en HTTP réel entre 2 process** — pas une simulation, pas un plan. Portée volontairement réduite (voir "Ce qui n'est PAS fait" plus bas).

## Contexte

`docs/LLAMA_CPP_PATCH.md` (existant, juillet 2026) décrivait un plan à 3 semaines pour patcher llama.cpp,
basé sur l'ancienne architecture monolithique (`src/llama.cpp` de 50k+ lignes, fonctions `llm_build_gemma3()`).
Ce plan est **obsolète** : llama.cpp a été refactorisé depuis — chaque architecture a son propre fichier sous
`src/models/*.cpp` (ex. `src/models/gemma3.cpp`, ~225 lignes), et le projet a déjà commencé à construire une
infrastructure générique pour l'extraction d'états cachés intermédiaires (`cparams.embeddings_layer_inp`,
`llama_get_embeddings_layer_inp`, mécanisme "nextn" pour le MTP/EAGLE3 speculative decoding).

Conséquence : le patch réellement nécessaire est **beaucoup plus petit** que prévu — 37 lignes nettes sur 5
fichiers (voir `0001-pipeline-split-poc.patch`), pas des semaines de travail C++.

## Ce qui a été fait et vérifié

### 1. Patch de base (single forward pass)

1. Fork réel de `ggml-org/llama.cpp` (commit `42fc243060709331ff9b158a9ed2cbe37219ae83`, 26/07/2026).
2. Compilation CPU-only réussie (`cmake` + `make`, backend CPU uniquement, pas de CUDA/Metal disponible dans
   l'environnement de build).
3. Patch appliqué à **Gemma3 Dense uniquement** (`src/models/gemma3.cpp`) :
   - Nouveau champ `cparams.pipeline_layer_start` (0 = comportement normal, inchangé).
   - La boucle de couches démarre à `pipeline_layer_start` au lieu de 0 — les couches précédentes ne sont
     **pas calculées** (vérifié : le graphe ggml généré passe de 87 nœuds à 47 nœuds sur un modèle à 2 couches,
     soit la moitié du calcul économisée).
   - Réutilisation de l'infrastructure **déjà existante** dans llama.cpp pour l'injection (`llama_batch.embd`,
     mécanisme utilisé nativement pour les entrées multimodales) et l'extraction (`t_layer_inp[il]`, déjà câblé
     génériquement pour le mécanisme "nextn" mais pas encore branché pour Gemma3 — une ligne ajoutée :
     `res->t_layer_inp[il] = inpL;`).
4. Modèle de test réel : `zelk12/gemma-3-tiny-random-Q6_K-GGUF` (8.4M paramètres aléatoires, architecture
   Gemma3 authentique, 2 couches — HuggingFace, 15 Mo).
5. Test de parité (`pipeline_test.cpp`) : un contexte A calcule le modèle complet (couches 0 et 1) et capture
   l'état caché en sortie de la couche 0 ; un contexte B **frais et indépendant**, configuré avec
   `pipeline_layer_start=1`, reçoit cet état caché via `llama_batch.embd` et ne calcule que la couche 1.

Résultat (2 prompts différents testés) : argmax identique, diff absolue max = **0** (bit-exact), rel_l2 = **0**.

### 2. Multi-step (prefill + decode), sans transfert de KV-cache — `pipeline_test2.cpp`

Question ouverte après la passe 1 : le mécanisme ne prouvait qu'un seul forward pass. Une génération réelle
enchaîne plusieurs `llama_decode()` (prefill puis N decodes autorégressifs), et l'ancien `LLAMA_CPP_PATCH.md`
supposait qu'il fallait un mécanisme de `kv_snapshot` pour transférer le KV-cache entre nœuds à chaque étape.

Test : node0 (contexte complet, tape la couche 1 via `embeddings_layer_inp`) + node1
(`pipeline_layer_start=1`), sur **2 étapes autorégressives réelles** (prefill du prompt, puis 1 decode du
token généré), comparé à un contexte baseline unique faisant tout le calcul normalement.

| Étape | Token match | max_abs_diff | rel_l2 |
|---|---|---|---|
| step1 (prefill) | **YES** | 0 | 0 |
| step2 (decode) | **YES** | 0 | 0 |

**Finding clé (corrige l'ancien doc) : aucun transfert de KV-cache entre nœuds n'est nécessaire.** Chaque
contexte llama.cpp maintient son propre KV-cache local, qui persiste automatiquement entre appels successifs
à `llama_decode()` sur ce même contexte. Seul l'état caché du flux résiduel (`hidden_states`, un vecteur par
token) doit transiter d'un nœud à l'autre à chaque étape — pas le KV-cache. Le design `kv_snapshot` de
l'ancien doc n'est donc pas nécessaire pour un pipeline-split séquentiel classique.

Note : une tentative d'ajouter un `pipeline_layer_end` (arrêt anticipé sur node0 pour éviter de calculer la
couche 1 inutilement) a fait planter le scheduler ggml (`GGML_ASSERT(buffer)` dans `ggml-backend.cpp`) quand
l'ensemble des tensors de sortie du graphe change entre deux `reserve()` sur le même contexte. **Reverté**,
pas livré dans le patch — node0 calcule donc le modèle complet à chaque étape (correct mais pas optimal en
calcul ; voir limitations).

### 3. Serveur HTTP réel, 2 process séparés — `pipeline_server.cpp`

Objectif : sortir du test in-process C++ et vérifier que le mécanisme fonctionne à travers un vrai réseau
(TCP + JSON + base64), avec une interface calquée exactement sur `pipeline_client.rs` (le client Rust déjà
existant dans `ainonymous-daemon`), pour que ce binaire puisse servir de backend alternatif à
`pipeline_server.py`.

- Serveur autonome (~340 lignes), linké directement à `libllama.a` (pas `tools/server/`, trop lourd et hors
  scope pour ce PoC).
- Dépendances vendorées : `httplib.h` + `httplib.cpp` (cpp-httplib, fork qui sépare déclaration/implémentation
  — inhabituel pour un header-only classique) + `json.hpp` (nlohmann).
- Endpoints : `GET /status`, `POST /prefill`, `POST /decode`, `POST /clear`, `POST /tokenize`,
  `POST /detokenize` — champs JSON alignés avec `PipelineStatus` côté Rust (`layer_end` inclus).
- Base64 encode/decode implémenté à la main (pas de lib externe).

**Test end-to-end réel** : 2 process serveur lancés (node0 : `--layer-start 0 --split 1 --port 9340` ;
node1 : `--layer-start 1 --last --port 9341`), requêtes envoyées via un script Python (`urllib`) simulant
exactement ce que ferait `pipeline_client.rs` : `/status` sur les deux (vérifié `layer_end=1` et `layer_end=2`
respectivement), puis même prompt fixe `[2, 55123, 9821, 174, 61000, 3]` envoyé en `/prefill` puis `/decode`
à travers les deux nœuds.

Résultat : **`step1: next_token_id=34658 " Não"`, `step2: next_token_id=34658 " Não"`, match exact avec le
baseline in-process C++ (`pipeline_test2` → 34658/34658).** Le round-trip HTTP réel (sérialisation base64 des
états cachés, requêtes réseau réelles sur 127.0.0.1) est donc bit-exact-équivalent au mécanisme in-process.

## Comment reproduire

```bash
# 1. Cloner llama.cpp au commit testé
git clone https://github.com/ggml-org/llama.cpp.git
cd llama.cpp && git checkout 42fc243060709331ff9b158a9ed2cbe37219ae83

# 2. Appliquer le patch
git apply /chemin/vers/0001-pipeline-split-poc.patch

# 3. Configurer et compiler (CPU-only ; adapter les flags GGML_CUDA/GGML_METAL si GPU dispo)
cmake -B build -DLLAMA_BUILD_COMMON=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_TOOLS=OFF \
  -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=OFF -DLLAMA_BUILD_APP=OFF -DLLAMA_BUILD_MTMD=OFF
cmake --build build -j$(nproc)

# 4. Télécharger le modèle de test
mkdir -p /tmp/models
curl -L -o /tmp/models/gemma3-tiny.gguf \
  "https://huggingface.co/zelk12/gemma-3-tiny-random-Q6_K-GGUF/resolve/main/gemma-3-tiny-random-q6_k.gguf"

# 5. Test de parité single-step
g++ -std=c++17 -O2 -I include -I ggml/include -I src pipeline_test.cpp \
  build/src/libllama.a build/ggml/src/libggml.a build/ggml/src/libggml-cpu.a build/ggml/src/libggml-base.a \
  -fopenmp -lpthread -ldl -lm -o pipeline_test
./pipeline_test

# 6. Test multi-step (prefill + decode, sans transfert KV)
g++ -std=c++17 -O2 -I include -I ggml/include -I src pipeline_test2.cpp \
  build/src/libllama.a build/ggml/src/libggml.a build/ggml/src/libggml-cpu.a build/ggml/src/libggml-base.a \
  -fopenmp -lpthread -ldl -lm -o pipeline_test2
./pipeline_test2

# 7. Serveur HTTP réel (2 nœuds) -- nécessite httplib.h/.cpp + json.hpp vendorés
# (voir ggml-org/llama.cpp vendor/cpp-httplib/ et vendor/nlohmann/ pour les récupérer)
g++ -std=c++17 -O2 -pthread \
  -I include -I ggml/include -I src -I vendor/cpp-httplib -I vendor/nlohmann \
  pipeline_server.cpp vendor/cpp-httplib/httplib.cpp \
  build/src/libllama.a build/ggml/src/libggml.a build/ggml/src/libggml-cpu.a build/ggml/src/libggml-base.a \
  -fopenmp -lpthread -ldl -lm -o pipeline_server

# terminal 1 (node0, first node, tape la couche 1)
./pipeline_server --model /tmp/models/gemma3-tiny.gguf --layer-start 0 --split 1 --port 9340
# terminal 2 (node1, last node, reprend à la couche 1)
./pipeline_server --model /tmp/models/gemma3-tiny.gguf --layer-start 1 --last --port 9341
# puis POST /prefill sur node0 -> hidden_states_b64 -> POST /prefill sur node1 -> next_token_id
```

## Ce qui n'est PAS fait (à ne pas confondre avec "terminé")

- **MoE (Gemma4/gemma3n)** : hors scope de cette passe. L'architecture "Gemma4" réelle dans llama.cpp actuel
  (`src/models/gemma4.cpp`) implémente en fait le style Gemma 3n (embeddings par couche + KV partagée +
  sliding window par couche) — bien plus complexe qu'un décodeur dense classique. À traiter séparément.
- **N > 2 nœuds** : bloqué sur le `pipeline_layer_end` reverté (voir section 2). Sans lui, un nœud
  intermédiaire ne peut pas s'arrêter proprement avant la fin du modèle — seul un split en exactement 2
  segments (premier + dernier) fonctionne aujourd'hui.
- **GPU (CUDA/Metal)** : patch et tests faits en CPU-only (pas de matériel GPU dans l'environnement de build).
  Le mécanisme (champs cparams + `llama_batch.embd`) est indépendant du backend, mais n'a pas été testé sur GPU.
- **Concurrence en production** : le serveur HTTP ne gère qu'une seule séquence à la fois (pas de slots de
  requêtes concurrentes, pas de file d'attente). Suffisant pour un testnet, pas pour de la charge réelle.
- **Efficacité calcul sur node0** : node0 recalcule le modèle complet à chaque étape au lieu de s'arrêter à
  la couche de coupure (conséquence directe du revert de `pipeline_layer_end`) — correct numériquement,
  gaspille du CPU/temps par rapport à un vrai pipeline-split optimisé.
- **Migration `conductor.rs`** : `pipeline_client.rs` continue de parler à `pipeline_server.py` (pont Python)
  en production. Le nouveau `pipeline_server.cpp` n'a pas encore été branché comme backend réel côté
  `ainonymous-daemon` — c'est un binaire testé et fonctionnel, mais pas encore intégré au conducteur.

## Prochaine étape logique

Brancher `pipeline_server.cpp` comme backend réel derrière `LlamaPipelineClient` côté `ainonymous-daemon`
(remplacement progressif de `pipeline_server.py`), et/ou déboguer la vraie cause du crash `pipeline_layer_end`
dans le scheduler ggml pour débloquer les chaînes à N > 2 nœuds.
