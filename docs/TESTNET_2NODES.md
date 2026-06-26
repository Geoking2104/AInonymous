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

## Prérequis

1. **Binaires** : `cargo build` (cible `target/debug/`).
2. **pipeline_server.py** :
   `pip install fastapi uvicorn transformers accelerate torch numpy`
3. **Modèle** HF accessible et son **nombre de couches** (`num_hidden_layers`
   dans le `config.json` du modèle). C'est `TOTAL_LAYERS`.

## Lancement

```bash
# TOTAL_LAYERS est obligatoire (doit correspondre au modèle).
make testnet-2 TOTAL_LAYERS=18 MODEL=google/gemma-3-1b-it
# ou directement :
TOTAL_LAYERS=18 MODEL=google/gemma-3-1b-it DEVICE=cpu \
  bash scripts/testnet/run_testnet_2.sh
```

Variables d'environnement :

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

Le script génère les configs dans `.testnet-run/`, lance les 2 pipeline_servers
puis les 2 daemons, attend qu'ils répondent, puis envoie une requête à
`POST http://127.0.0.1:8889/mesh/infer`. Réponse affichée + sauvée dans
`.testnet-run/last_response.json`. Logs dans `.testnet-run/logs/`. `Ctrl-C` arrête tout.

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

## Limites connues (testnet)

- Une session QUIC est **renégociée par lien**, pas par token (optimisé), mais
  le **decode reste séquentiel** (un aller-retour de chaîne par token).
- Le **KV-cache** est purgé à la fermeture de session (fin de requête / coupure).
- Passage **2 machines réelles** : mêmes configs mais remplacer `127.0.0.1` par
  les IP réelles dans `daemon_url` / `quic_endpoint` / `quic_advertise`, et
  ouvrir les ports QUIC (UDP) + REST (TCP).
