# Testnet 2 nœuds (loopback) — pipeline-split

Valide de bout en bout le chemin d'inférence distribuée en **topologie chaîne**
(cf. [`ADR_001_coordinator_decode_loop.md`](ADR_001_coordinator_decode_loop.md))
sur **une seule machine**, sans Holochain (plan d'exécution **statique** via la
config).

```
daemon A  = coordinateur + étage 0   couches [0, SPLIT[     pipeline_server :9340
daemon B  = étage 1 (dernier)        couches [SPLIT, N[     pipeline_server :9341

requête → A(coord) → A(stage0) → B(stage1) → token → relayé A → A(coord) → réponse
```

Le coordinateur ouvre **une** session QUIC vers l'étage 0 et la réutilise pour
toutes les passes (prefill + chaque decode) ; à la fin, la fermeture de session
**purge le KV-cache** de toute la chaîne.

`PipelineClient` (côté Rust, `crates/ainonymous-daemon/src/pipeline_client.rs`)
est un simple client HTTP générique — il ignore complètement quel process tourne
en face, il ne connaît que le protocole JSON. Deux backends sont donc
interchangeables sur ce testnet (voir `BACKEND` ci-dessous) sans aucun changement
Rust.

## Prérequis

1. **Binaires** : `cargo build` (cible `target/debug/`).
2. Un backend pipeline_server (voir "Backend" ci-dessous).
3. **Modèle** : dépend du backend choisi.

## Backend : `python` (défaut) ou `native`

| | `BACKEND=python` (défaut) | `BACKEND=native` |
|---|---|---|
| Process lancé | `scripts/pipeline_server.py` (HuggingFace transformers) | `patches/llama-cpp-pipeline-split/pipeline_server.cpp` (llama.cpp patché) |
| Modèle | ID HuggingFace (`MODEL`), téléchargé à la volée | fichier `.gguf` local (`NATIVE_MODEL_GGUF`) |
| Device | CPU ou CUDA (`DEVICE`) | **CPU uniquement** |
| Précision hidden states | float16 | float32 |
| Architectures supportées | Gemma 4 dense + MoE (via transformers) | **Gemma3 Dense uniquement** (voir patch) |
| Nœuds | N quelconque | **2 max** (bloqué par un crash ggml non résolu, voir README du patch) |
| Statut | prévu pour un vrai modèle HF (`pip install fastapi uvicorn transformers accelerate torch numpy`) | preuve de concept vérifiée bit-exact (single-step, multi-step, HTTP), voir `patches/llama-cpp-pipeline-split/README.md` |

`BACKEND=native` nécessite un binaire `pipeline_server` déjà compilé (voir
`patches/llama-cpp-pipeline-split/README.md` § "Comment reproduire" — le fork
llama.cpp patché n'est pas vendoré dans ce repo, il faut le construire à part).

## Lancement

```bash
# Backend Python (par défaut)
make testnet-2 TOTAL_LAYERS=18 MODEL=google/gemma-3-1b-it
# ou directement :
TOTAL_LAYERS=18 MODEL=google/gemma-3-1b-it DEVICE=cpu \
  bash scripts/testnet/run_testnet_2.sh

# Backend natif llama.cpp (Gemma3 Dense, 2 nœuds, CPU)
make testnet-2-native TOTAL_LAYERS=2 \
  NATIVE_BIN=/chemin/vers/pipeline_server \
  NATIVE_MODEL_GGUF=/chemin/vers/model.gguf
```

Variables d'environnement (backend `python`) :

| Var | Défaut | Rôle |
|---|---|---|
| `TOTAL_LAYERS` | — (obligatoire) | `num_hidden_layers` du modèle |
| `MODEL` | `google/gemma-3-1b-it` | ID HuggingFace |
| `SPLIT` | `TOTAL_LAYERS/2` | couche de coupure étage0/étage1 |
| `DEVICE` | `cpu` | `cpu` ou `cuda` |
| `DTYPE` | `bf16` | `fp16` ou `bf16` |
| `PROMPT` | "Bonjour…" | prompt de test |
| `MAX_TOKENS` | `32` | tokens à générer |
| `BIN` | `target/debug` | dossier des binaires |

Variables supplémentaires (backend `native`) :

| Var | Rôle |
|---|---|
| `NATIVE_BIN` | chemin du binaire `pipeline_server` compilé (obligatoire) |
| `NATIVE_MODEL_GGUF` | chemin d'un modèle `.gguf` local (obligatoire, pas un ID HF) |

Le script génère les configs dans `.testnet-run/`, lance les 2 pipeline_servers
(python ou natif selon `BACKEND`) puis les 2 daemons, attend qu'ils répondent, puis
envoie une requête à `POST http://127.0.0.1:8889/mesh/infer`. Réponse affichée +
sauvée dans `.testnet-run/last_response.json`. Logs dans `.testnet-run/logs/`.
`Ctrl-C` arrête tout.

## Vérifier le succès (Definition of Done)

- La réponse JSON contient `"execution_mode":"pipeline_split"` et
  `"node_ids":["node-a","node-b"]` (`token_count` > 0).
- `logs/daemon-a.log` montre la négociation de session puis « Activations
  transmises » ; `logs/daemon-b.log` montre une session entrante et la
  production de tokens.
- Couper B (Ctrl-C sur son process) puis relancer une requête : A doit logguer
  l'échec de l'étage suivant (robustesse).

## Dépannage

| Symptôme | Cause probable |
|---|---|
| `plan indisponible` (503) | `pipeline_stages`/`peers` absents de `node-a.toml` |
| `aucun token reçu` | pipeline_server B planté → voir `logs/pipe-b.log` |
| `connexion QUIC` échoue | `quic_advertise` ≠ adresse joignable (ici `127.0.0.1`) |
| sortie incohérente | `TOTAL_LAYERS`/`SPLIT` ne correspondent pas au modèle |
| OOM / lenteur | modèle trop gros pour `DEVICE=cpu` → prendre un modèle plus petit |
| `BACKEND=native` : `NATIVE_BIN` non exécutable | vérifier le chemin + `chmod +x`, voir instructions de build dans `patches/llama-cpp-pipeline-split/README.md` |

## Limites connues (testnet)

- Une session QUIC est **renégociée par lien**, pas par token (optimisé), mais
  le **decode reste séquentiel** (un aller-retour de chaîne par token).
- Le **KV-cache** est purgé à la fermeture de session (fin de requête / coupure).
- `BACKEND=native` : Gemma3 Dense uniquement, CPU uniquement, 2 nœuds max (voir
  tableau ci-dessus et `patches/llama-cpp-pipeline-split/README.md` pour le détail).
- Ne pas mélanger un nœud `python` et un nœud `native` dans la même chaîne : la
  sérialisation des hidden states diffère (float16 vs float32).
- Passage **2 machines réelles** : mêmes configs mais remplacer `127.0.0.1` par
  les IP réelles dans `daemon_url` / `quic_endpoint` / `quic_advertise`, et
  ouvrir les ports QUIC (UDP) + REST (TCP).
