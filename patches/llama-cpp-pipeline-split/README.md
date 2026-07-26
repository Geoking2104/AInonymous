# Pipeline-split patch for llama.cpp — Gemma3 Dense (Preuve de concept vérifiée)

Statut : **mécanisme prouvé, réel et compilé** — pas une simulation, pas un plan. Portée volontairement réduite (voir "Ce qui n'est PAS fait" plus bas).

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

### Résultat mesuré (2 prompts différents testés)

| Métrique | Valeur |
|---|---|
| Argmax(logits) contexte complet vs contexte splitté | **identique** dans les 2 cas |
| Différence absolue max sur les logits | **0** (bit-exact) |
| Différence relative L2 | **0** |
| Stats état caché transféré (n=192, prompt 2) | mean=0.587, std=3.20, min=-7.53, max=10.21 — non dégénéré |
| Stats logits (prompt 2) | mean=0.0008, std=2.48, min=-10.9, max=11.5 — non dégénéré |

Le deuxième prompt donne un argmax différent du premier (34658 vs 4000), ce qui exclut une sortie constante
accidentelle qui aurait faussement validé le test.

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

# 5. Compiler et lancer le test de parité
g++ -std=c++17 -O2 -I include -I ggml/include -I src pipeline_test.cpp \
  build/src/libllama.a build/ggml/src/libggml.a build/ggml/src/libggml-cpu.a build/ggml/src/libggml-base.a \
  -fopenmp -lpthread -ldl -lm -o pipeline_test
./pipeline_test
```

## Ce qui n'est PAS fait (à ne pas confondre avec "terminé")

- **MoE (Gemma4/gemma3n)** : hors scope de cette passe. L'architecture "Gemma4" réelle dans llama.cpp actuel
  (`src/models/gemma4.cpp`) implémente en fait le style Gemma 3n (embeddings par couche + KV partagée +
  sliding window par couche) — bien plus complexe qu'un décodeur dense classique. À traiter séparément.
- **Endpoint serveur** (`POST /v1/pipeline/forward` dans `tools/server/`) : non fait. Le code serveur actuel
  fait 217 Ko rien que pour `server-context.cpp` — une tâche à part entière, pas ajoutée dans cette passe pour
  éviter un patch bâclé et non testé.
- **Génération multi-tokens / KV-cache inter-process** : le test ne couvre qu'un seul forward pass (prefill).
  Le maintien du KV-cache entre nœuds sur plusieurs étapes de decode (mentionné comme `kv_snapshot` dans
  l'ancien `LLAMA_CPP_PATCH.md`) reste à concevoir.
- **GPU (CUDA/Metal)** : patch et test faits en CPU-only (pas de matériel GPU dans l'environnement de build).
  Le mécanisme (champs cparams + `llama_batch.embd`) est indépendant du backend, mais n'a pas été testé sur GPU.
- **Migration `conductor.rs`** : `pipeline_client.rs` continue de parler à `pipeline_server.py` (pont Python)
  pour l'instant. Pas de `LlamaPipelineClient` Rust écrit dans cette passe — prématuré tant qu'il n'y a pas
  d'endpoint serveur réel à cibler.

## Prochaine étape logique

Écrire l'endpoint serveur minimal (pas besoin de toute la complexité de `tools/server/` — un petit
exécutable HTTP autonome linké à `libllama.a`, exposant `pipeline_layer_start` + injection/extraction via
JSON+base64, suffit pour un testnet 2 nœuds réel). Ensuite seulement, écrire le client Rust côté
`ainonymous-daemon`.
