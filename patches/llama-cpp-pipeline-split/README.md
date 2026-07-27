# Pipeline-split patch for llama.cpp — Gemma3 Dense (Preuve de concept vérifiée)

Statut : **mécanisme prouvé, réel et compilé** — single forward pass, multi-step, serveur HTTP réel entre 2
process, branché dans le testnet du daemon, l'early-exit `pipeline_layer_end` (nécessaire pour des chaînes à
N > 2 nœuds) débogué et branché dans `pipeline_server.cpp` (économie de calcul confirmée), **et un vrai nœud
du milieu (`pipeline_layer_start>0` ET `pipeline_layer_end<n_layer` simultanément) désormais testé et
vérifié bit-exact sur une vraie chaîne à 3 nœuds**. Pas une simulation, pas un plan. Portée volontairement
réduite (voir "Ce qui n'est PAS fait" plus bas).

## Contexte

`docs/LLAMA_CPP_PATCH.md` (existant, juillet 2026) décrivait un plan à 3 semaines pour patcher llama.cpp,
basé sur l'ancienne architecture monolithique (`src/llama.cpp` de 50k+ lignes, fonctions `llm_build_gemma3()`).
Ce plan est **obsolète** : llama.cpp a été refactorisé depuis — chaque architecture a son propre fichier sous
`src/models/*.cpp` (ex. `src/models/gemma3.cpp`, ~225 lignes), et le projet a déjà commencé à construire une
infrastructure générique pour l'extraction d'états cachés intermédiaires (`cparams.embeddings_layer_inp`,
`llama_get_embeddings_layer_inp`, mécanisme "nextn" pour le MTP/EAGLE3 speculative decoding).

Conséquence : le patch réellement nécessaire est **beaucoup plus petit** que prévu — 110 lignes nettes sur 5
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

Dans cette passe, node0 calculait le modèle complet à chaque étape au lieu de s'arrêter à la couche de
coupure (une tentative d'early-exit avait planté et avait été revertée — voir section 5, le bug est
maintenant corrigé et branché dans le serveur HTTP, section 6).

### 3. Serveur HTTP réel, 2 process séparés — `pipeline_server.cpp`

Objectif : sortir du test in-process C++ et vérifier que le mécanisme fonctionne à travers un vrai réseau
(TCP + JSON + base64), avec une interface calquée exactement sur `pipeline_client.rs` (le client Rust déjà
existant dans `ainonymous-daemon`), pour que ce binaire puisse servir de backend alternatif à
`pipeline_server.py`.

- Serveur autonome (~360 lignes), linké directement à `libllama.a` (pas `tools/server/`, trop lourd et hors
  scope pour ce PoC).
- Dépendances vendorées : `httplib.h` + `httplib.cpp` (cpp-httplib, fork qui sépare déclaration/implémentation
  — inhabituel pour un header-only classique) + `json.hpp` (nlohmann).
- Endpoints : `GET /status`, `POST /prefill`, `POST /decode`, `POST /clear`, `POST /tokenize`,
  `POST /detokenize` — champs JSON alignés avec `PipelineStatus` côté Rust (`layer_end` inclus).
- Base64 encode/decode implémenté à la main (pas de lib externe).
- **CLI aligné sur `scripts/pipeline_server.py`** : accepte `--layer-end`, `--is-first-node`, `--is-last-node`
  en plus des flags internes (`--split`, `--last`), et `--device`/`--dtype` (acceptés puis ignorés — ce
  binaire est CPU/F32 uniquement). But : pouvoir le lancer avec exactement les mêmes arguments que le
  serveur Python dans un script de testnet.

**Test end-to-end réel** : 2 process serveur lancés (node0 : `--layer-start 0 --layer-end 1 --is-first-node
--port 9350` ; node1 : `--layer-start 1 --is-last-node --port 9351`), requêtes envoyées via un script Python
(`urllib`) simulant exactement ce que ferait `pipeline_client.rs` : `/status` sur les deux (vérifié
`layer_end=1` et `layer_end=2` respectivement, `is_first_node`/`is_last_node` corrects), puis même prompt
fixe `[2, 55123, 9821, 174, 61000, 3]` envoyé en `/prefill` puis `/decode` à travers les deux nœuds.

Résultat : **`step1: next_token_id=34658 " Não"`, `step2: next_token_id=34658 " Não"`, match exact avec le
baseline in-process C++ (`pipeline_test2` → 34658/34658).** Le round-trip HTTP réel (sérialisation base64 des
états cachés, requêtes réseau réelles sur 127.0.0.1) est donc bit-exact-équivalent au mécanisme in-process, et
ce avec les flags CLI calqués sur `pipeline_server.py`.

### 4. Intégration dans le testnet du daemon — `scripts/testnet/run_testnet_2.sh`

Lecture du code réel (`crates/ainonymous-daemon/src/pipeline_client.rs`, `conductor.rs`, `config.rs`,
`scripts/testnet/run_testnet_2.sh`) : **`PipelineClient` est un simple client HTTP générique** — il ne sait
pas si le process en face est `pipeline_server.py` ou autre chose, il parle juste le protocole JSON défini
par les structs `PrefillRequest`/`PrefillResponse`/`DecodeRequest`/`DecodeResponse`/`PipelineStatus`. Les
configs `node-a.toml`/`node-b.toml` générées par le script ne référencent même pas le backend — elles ne
contiennent que le port (`pipeline_server_port`). Autrement dit : **aucun changement Rust n'était nécessaire**
pour brancher `pipeline_server.cpp`, seul le lanceur de process (le script bash) doit choisir quel binaire
démarrer sur ce port.

Changements livrés :
- `pipeline_server.cpp` : CLI étendu (section 3 ci-dessus) pour matcher exactement les flags de
  `pipeline_server.py`.
- `scripts/testnet/run_testnet_2.sh` : nouvelle variable `BACKEND` (`python` par défaut, ou `native`). En mode
  `native`, lance `$NATIVE_BIN` (le binaire `pipeline_server` compilé) avec `$NATIVE_MODEL_GGUF` (un `.gguf`
  local, pas un ID HuggingFace) au lieu de `python3 scripts/pipeline_server.py` — le reste du script (configs
  daemon, attente `/status`, requête `/mesh/infer`) est identique.
- `Makefile` : cible `testnet-2-native` (variables : `TOTAL_LAYERS`, `NATIVE_BIN`, `NATIVE_MODEL_GGUF`, `SPLIT`).

Exemple :
```bash
make testnet-2-native TOTAL_LAYERS=2 \
  NATIVE_BIN=/tmp/pipeline_server \
  NATIVE_MODEL_GGUF=/tmp/models/gemma3-tiny.gguf
```

**Vérifié dans cette session** : le binaire natif avec les nouveaux flags CLI (`--layer-end`,
`--is-first-node`, `--is-last-node`) reproduit exactement le comportement testé en section 3 (mêmes tokens,
mêmes champs `/status`). **Non vérifié** : le run complet du script bash avec de vrais binaires
`ainonymous-daemon` (pas de toolchain Rust/cargo dans le sandbox de cette session) — seule la syntaxe bash du
script modifié a été validée (`bash -n` sur le patron identique). À faire avant de considérer l'intégration
"testée en conditions réelles" : lancer `make testnet-2-native` sur une machine avec `cargo build` disponible.

### 5. `pipeline_layer_end` : cause racine trouvée et corrigée — `pipeline_test3_earlyexit.cpp`

Contexte : la section 2 utilisait node0 en calculant le **modèle complet** à chaque étape, même si seule la
couche 0 nous intéressait — gaspillage de calcul. Une tentative précédente d'ajouter un early-exit
(`pipeline_layer_end`, pour que node0 s'arrête après sa dernière couche assignée) avait fait planter le
programme avec `GGML_ASSERT(buffer) failed` dans `ggml-backend.cpp`, et avait été revertée sans être
diagnostiquée à fond.

**Démarche de debug** :
1. Réimplémentation de `pipeline_layer_end` (nouveau champ `cparams`, setter `set_pipeline_layer_end()` avec
   `sched_need_reserve = true` — même patron que `pipeline_layer_start`), et modification de
   `src/models/gemma3.cpp` pour que la boucle de couches s'arrête à `pipeline_layer_end` au lieu de `n_layer`,
   en sautant `norm`+`lm_head` dans ce cas.
2. Reproduction immédiate et fiable du crash original (`pipeline_test3_earlyexit.cpp`, node0 =
   `pipeline_layer_start=0` + `pipeline_layer_end=1`).
3. Pas de `gdb` dans l'environnement de build → diagnostic par instrumentation : `fprintf` +
   `__builtin_return_address(0)` injectés temporairement dans les fonctions `ggml_backend_buffer_get_usage`
   et `ggml_backend_buffer_get_type` (`ggml/src/ggml-backend.cpp`), binaire recompilé en `-no-pie` pour avoir
   des adresses fixes, puis `addr2line -f -C -e <binaire> <adresse>` pour résoudre le symbole appelant.
4. **Résultat** : la fonction fautive est `llm_graph_input_out_ids::set_input()`. Cause racine exacte :
   `build_inp_out_ids()` est appelé **inconditionnellement** en haut de la fonction de construction du graphe
   Gemma3, **avant** le point où l'early-exit était décidé. Ce tenseur `inp_out_ids` n'est utilisé que dans la
   branche `if (il == n_layer - 1 && inp_out_ids)`, jamais atteinte en mode early-exit. Résultat :
   `ggml_build_forward_expand()` ne lui donne jamais d'arête consommatrice, le scheduler ne lui alloue donc
   jamais de buffer — mais `set_input()` (appelé pour **tous** les inputs enregistrés, qu'ils soient
   réellement utilisés dans le graphe ou non) tente quand même d'écrire dedans, et déréférence un
   `tensor->buffer` nul. **Ce n'est pas un bug du scheduler ggml** : c'est un bug d'ordre de construction dans
   le patch — `inp_out_ids` était construit avant que la portée réelle du graphe (early-exit ou non) ne soit
   connue.
5. **Fix** : ne construire `inp_out_ids` (`build_inp_out_ids()`) que si le graphe va réellement atteindre la
   couche finale (`pipeline_end >= n_layer`). Une ligne de condition, pas de changement dans `ggml/`.
6. Effet de bord découvert et corrigé au passage : quand `pipeline_layer_end` est actif,
   `embeddings_layer_inp(pipeline_end)` (utilisé pour extraire l'état cascade vers le nœud suivant) attend que
   `res->t_layer_inp[pipeline_end]` soit rempli — mais la boucle s'arrête avant d'atteindre cette itération.
   Fix : peupler explicitement `res->t_layer_inp[pipeline_end] = cur;` dans la branche early-exit.

**Vérification** (`pipeline_test3_earlyexit.cpp`, node0 = `layer_start=0` + `layer_end=1` + capture
`embeddings_layer_inp(1)`, node1 = `layer_start=1` normal, 2 étapes prefill+decode) :

| Étape | Token (avant fix) | Token (après fix) |
|---|---|---|
| step1 | **crash** | 34658 |
| step2 | — | 34658 |

Résultat identique au baseline `pipeline_test2.cpp` (34658/34658) — **aucune régression** : `pipeline_test.cpp`
et `pipeline_test2.cpp` (non modifiés) ont été réexécutés après le fix et restent bit-exact (diff = 0, rel_l2
= 0) — le changement structurel dans `gemma3.cpp` (réordonnancement de `build_inp_out_ids()`) n'affecte pas le
chemin "normal" (`pipeline_layer_end = 0`, comportement par défaut).

**Ce qui restait non vérifié à l'époque (résolu section 7 ci-dessous)** : le fix avait été testé avec un node
early-exit qui est aussi le **premier** nœud (`pipeline_layer_start=0` + `pipeline_layer_end<n_layer`). Un vrai
**nœud intermédiaire** (`pipeline_layer_start>0` **et** `pipeline_layer_end<n_layer` simultanément, cas
nécessaire pour des chaînes à N>2 nœuds) n'avait pas été testé avec un modèle réel à 3+ couches.

### 6. `pipeline_layer_end` branché dans `pipeline_server.cpp` — économie de calcul confirmée

Le serveur HTTP (section 3/4) avait été testé et livré *avant* le fix `pipeline_layer_end` (section 5) : il
acceptait le flag `--layer-end` mais ne l'utilisait que pour l'extraction (`embeddings_layer_inp`), sans
jamais appeler `llama_set_pipeline_layer_end()` — node0 recalculait donc le modèle complet (couches + norm +
lm_head) à chaque étape, pour ne garder que l'état intermédiaire de la couche 0.

Changement : dans `main()`, la branche `if (!S.cfg.is_last_node)` appelle maintenant aussi
`llama_set_pipeline_layer_end(S.ctx, S.cfg.split)`, en plus de `llama_set_embeddings_layer_inp(...)` déjà
présent.

**Vérifié** : recompilation + relance des 2 process HTTP (node0 : `--layer-start 0 --layer-end 1
--is-first-node` ; node1 : `--layer-start 1 --is-last-node`), même prompt, même résultat bit-exact
(`step1`/`step2` → token `34658` sur les deux, identique à avant le changement). **Et** les logs de reserve
confirment l'économie de calcul réelle : le graphe ggml de node0 passe de **87 nœuds** (reserve initiale,
modèle complet) à **42 nœuds** après que `set_pipeline_layer_end` force un nouveau reserve — soit à peu près
la moitié du calcul, cohérent avec le fait que node0 ne possède qu'1 couche sur 2 dans ce modèle de test.
node1 (dernier nœud, non affecté par ce changement) reste à 47 nœuds comme avant.

### 7. Vrai nœud du milieu (N=3, `layer_start>0` ET `layer_end<n_layer` simultanément) — vérifié

Dernière limite explicitement non testée (sections 5/6) : tout ce qui précède n'avait été vérifié qu'avec un
nœud early-exit qui est aussi le **premier** nœud (`pipeline_layer_start=0`). Un vrai nœud du **milieu** — ni
premier ni dernier, qui reçoit un état caché injecté (comme un nœud non-first) **et** s'arrête avant la fin
(comme un nœud non-last) — n'avait jamais été exercé faute d'un modèle de test à 3+ couches (le seul modèle
disponible, `gemma3-tiny.gguf`, n'a que 2 couches).

**Modèle de test à 3 couches** (`build_3layer_test_gguf.py`) : plutôt que d'installer `torch`/`transformers`
(lourd, inutile pour un test de plomberie sur poids aléatoires), le script construit le GGUF directement avec
le package `gguf` (pur Python + numpy, aucune dépendance ML) : il copie tous les tenseurs et métadonnées de
`gemma3-tiny.gguf` **tels quels** (y compris les tenseurs déjà quantifiés Q8_0, copiés octet pour octet sans
déquantification — piège rencontré : `raw_shape` doit être la "byte shape" du tenseur brut, pas sa forme
logique ne, sinon `gguf-py` échoue en tentant de reconvertir une forme déjà convertie), passe `block_count` de
2 à 3, et duplique les tenseurs de la couche 1 pour créer une couche 2. Les poids sont aléatoires de toute
façon — seule la forme/le dtype comptent pour vérifier que le mécanisme fonctionne.

**Test** : 4 process serveur HTTP lancés simultanément sur le modèle à 3 couches —
- `baseline` (port 9500) : `--layer-start 0 --is-last-node` (calcule les 3 couches normalement, sert de référence)
- `node0` (port 9501) : `--layer-start 0 --layer-end 1 --is-first-node` (possède la couche 0)
- `node1` (port 9502) : `--layer-start 1 --layer-end 2` (**ni `--is-first-node` ni `--is-last-node` — le vrai
  nœud du milieu recherché**, possède la couche 1 seule)
- `node2` (port 9503) : `--layer-start 2 --is-last-node` (possède la couche 2, calcule les logits)

Même prompt fixe envoyé au `baseline` (prefill + decode) et à la chaîne `node0 → node1 → node2` (prefill puis
decode, relayant les états cachés en base64 à travers les 3 process réels) :

| | baseline (1 contexte, 3 couches) | chaîne (node0→node1→node2) | match |
|---|---|---|---|
| step1 (prefill) | token 34658 | token 34658 | **YES** |
| step2 (decode) | token 34658 | token 34658 | **YES** |

**Bit-exact.** Et les logs de `sched_reserve` confirment que le nœud du milieu bénéficie bien du même
mécanisme d'économie de calcul que les autres : `node1` (le vrai milieu) passe de 127 nœuds (réserve initiale
plein modèle à 3 couches) à **42 nœuds** après `set_pipeline_layer_start` + `set_pipeline_layer_end` —
identique à `node0` (42 nœuds). `node2` (dernier nœud, calcule jusqu'à la fin) reserve à 47 nœuds.

**Conclusion** : le fix `pipeline_layer_end` (section 5) et son branchement dans `pipeline_server.cpp`
(section 6) sont bien **indépendants de la valeur de `pipeline_layer_start`**, comme argumenté
structurellement à l'époque — c'est maintenant vérifié empiriquement, pas juste déduit. Les chaînes N>2
nœuds avec un vrai nœud intermédiaire sont donc débloquées de bout en bout au niveau du mécanisme llama.cpp +
serveur HTTP (reste hors scope : le run réel avec `ainonymous-daemon`/cargo, voir "Ce qui n'est PAS fait").

**Piège opérationnel rencontré (sandbox uniquement, sans rapport avec le patch)** : lancer 4 process serveur
en arrière-plan avec `pkill -f pipeline_server` en tête de script tue le script lui-même, car sous `bwrap
--unshare-pid`, `pkill -f` matche aussi le propre `argv` du process `bash -c "<script>"` qui contient
littéralement la chaîne "pipeline_server" dans son propre texte. Fix : capturer les PID via `$!` et ne jamais
utiliser `pkill -f` avec un motif qui apparaît dans le script appelant.

## Comment reproduire

```bash
# 1. Cloner llama.cpp au commit testé
git clone https://github.com/ggml-org/llama.cpp.git
cd llama.cpp && git checkout 42fc243060709331ff9b158a9ed2cbe37219ae83

# 2. Appliquer le patch
git apply /chemin/vers/0001-pipeline-split-poc.patch

# 3. Configurer et compiler (CPU-only ; adapter les flags GGML_CUDA/GGML_METAL si GPU dispo)
cmake -B build -DBUILD_SHARED_LIBS=OFF -DLLAMA_BUILD_COMMON=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_TOOLS=OFF \
  -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=OFF -DLLAMA_BUILD_APP=OFF -DLLAMA_BUILD_MTMD=OFF
cmake --build build -j$(nproc)

# 4. Télécharger le modèle de test (2 couches)
mkdir -p /tmp/models
curl -L -o /tmp/models/gemma3-tiny.gguf \
  "https://huggingface.co/zelk12/gemma-3-tiny-random-Q6_K-GGUF/resolve/main/gemma-3-tiny-random-q6_k.gguf"

# 4b. (Optionnel, pour tester un vrai nœud du milieu / N=3) générer une variante à 3 couches
pip install gguf --break-system-packages
python3 build_3layer_test_gguf.py   # lit gemma3-tiny.gguf, écrit gemma3-tiny-3l.gguf

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

# 6b. Test early-exit (pipeline_layer_end) -- le crash historique, maintenant corrigé
g++ -std=c++17 -O2 -I include -I ggml/include -I src pipeline_test3_earlyexit.cpp \
  build/src/libllama.a build/ggml/src/libggml.a build/ggml/src/libggml-cpu.a build/ggml/src/libggml-base.a \
  -fopenmp -lpthread -ldl -lm -o pipeline_test3
./pipeline_test3

# 7. Serveur HTTP réel (2 nœuds) -- nécessite httplib.h/.cpp + json.hpp vendorés
# (déjà présents dans vendor/cpp-httplib et vendor/nlohmann de ce fork llama.cpp)
g++ -std=c++17 -O2 -pthread \
  -I include -I ggml/include -I src -I vendor/cpp-httplib -I vendor/nlohmann \
  pipeline_server.cpp vendor/cpp-httplib/httplib.cpp \
  build/src/libllama.a build/ggml/src/libggml.a build/ggml/src/libggml-cpu.a build/ggml/src/libggml-base.a \
  -fopenmp -lpthread -ldl -lm -o pipeline_server

# terminal 1 (node0, first node, tape la couche 1, s'arrête après -- fix pipeline_layer_end branché)
./pipeline_server --model /tmp/models/gemma3-tiny.gguf --layer-start 0 --layer-end 1 --is-first-node --port 9340
# terminal 2 (node1, last node, reprend à la couche 1)
./pipeline_server --model /tmp/models/gemma3-tiny.gguf --layer-start 1 --is-last-node --port 9341
# puis POST /prefill sur node0 -> hidden_states_b64 -> POST /prefill sur node1 -> next_token_id

# 7b. (Optionnel) chaîne à 3 nœuds avec un vrai nœud du milieu, sur le modèle à 3 couches (voir 4b)
./pipeline_server --model /tmp/models/gemma3-tiny-3l.gguf --layer-start 0 --layer-end 1 --is-first-node --port 9501
./pipeline_server --model /tmp/models/gemma3-tiny-3l.gguf --layer-start 1 --layer-end 2 --port 9502              # <- vrai milieu
./pipeline_server --model /tmp/models/gemma3-tiny-3l.gguf --layer-start 2 --is-last-node --port 9503
# relayer hidden_states_b64 : node0 -> node1 -> node2, comparer au résultat d'un node --is-last-node seul
# calculant les 3 couches (voir section 7 pour le script Python de test complet)

# 8. Testnet complet avec le daemon Rust (depuis la racine du repo AInonymous)
make testnet-2-native TOTAL_LAYERS=2 \
  NATIVE_BIN=/chemin/vers/pipeline_server \
  NATIVE_MODEL_GGUF=/tmp/models/gemma3-tiny.gguf
```

## Ce qui n'est PAS fait (à ne pas confondre avec "terminé")

- **MoE (Gemma4/gemma3n)** : hors scope de cette passe. L'architecture "Gemma4" réelle dans llama.cpp actuel
  (`src/models/gemma4.cpp`) implémente en fait le style Gemma 3n (embeddings par couche + KV partagée +
  sliding window par couche) — bien plus complexe qu'un décodeur dense classique. À traiter séparément.
- **GPU (CUDA/Metal)** : patch et tests faits en CPU-only (pas de matériel GPU dans l'environnement de build).
  Le mécanisme (champs cparams + `llama_batch.embd`) est indépendant du backend, mais n'a pas été testé sur GPU.
- **Concurrence en production** : le serveur HTTP ne gère qu'une seule séquence à la fois (pas de slots de
  requêtes concurrentes, pas de file d'attente). Suffisant pour un testnet, pas pour de la charge réelle.
- **Build automatisé du binaire natif** : le fork llama.cpp patché n'est pas vendoré dans ce repo ni construit
  par `cargo build`/le Makefile — `NATIVE_BIN` doit être compilé et fourni à la main (voir "Comment reproduire").
  Automatiser ça (script de build, ou vendoring du fork) reste à faire.
- **Run réel du testnet natif avec `cargo build`** : le branchement `BACKEND=native` dans
  `run_testnet_2.sh`/`Makefile` a été écrit et sa logique (choix du process à lancer) validée manuellement,
  mais pas exécutée de bout en bout avec un vrai `ainonymous-daemon` compilé (pas de toolchain Rust dans le
  sandbox de cette session).
- **Format de sérialisation incompatible avec le backend Python** : `pipeline_server.py` sérialise les hidden
  states en **float16**, `pipeline_server.cpp` en **float32**. Ne pas mélanger un nœud natif et un nœud Python
  dans la même chaîne — les deux process d'un même testnet doivent être du même backend.
- **N > 3 nœuds jamais essayé** : le mécanisme pour un vrai nœud du milieu est maintenant vérifié (section 7),
  mais seulement avec exactement 3 nœuds (1 first + 1 middle + 1 last). Une chaîne à 4+ nœuds (2+ middles
  successifs) n'a pas été testée — devrait fonctionner par construction (chaque nœud est indépendant), mais
  reste à vérifier empiriquement avant de l'affirmer.

## Prochaine étape logique

1. Lancer `make testnet-2-native` (ou une variante `testnet-3-native`) sur une machine avec `cargo build` +
   le binaire natif compilé, pour vérifier l'intégration testnet de bout en bout (pas seulement le mécanisme
   HTTP isolé) — y compris avec un vrai nœud du milieu maintenant que le mécanisme est vérifié.
2. Automatiser le build du fork llama.cpp patché (script ou vendoring) pour que `NATIVE_BIN` n'ait plus besoin
   d'être compilé à la main.
3. Tester une chaîne à 4+ nœuds (2+ nœuds du milieu successifs) pour généraliser au-delà de N=3.
