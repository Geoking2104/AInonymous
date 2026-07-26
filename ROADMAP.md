# AInonymous — Roadmap & Paliers

## Statut actuel (juillet 2026)

### Palier E — Keyring OS natif + Rotation ed25519
**Statut** : ✅ Finalisé et testé

### Palier F — Intégration Holochain réelle + Membrane Proofs + Warrants
**Statut** : ✅ Largement terminé (voir `docs/PALIER_F.md` — corrigé le 26/07/2026, ce fichier était désynchronisé)

- `MembraneProofConfig` + injection automatique
- `call_zome_with_proof` et `install_app_with_membrane_proof`
- Types `Warrant` + `ModelClaim`
- `emit_warrant`, `verify_warrant`, `get_warrants_for_agent`
- `validate_node_warrants` (enforcement basique)
- Scoring intelligent des nœuds (VRAM 35% + charge 25% + slots 15% + Haversine 10-15%)
- Zome dédié `zomes/warrants/` avec validation on-chain

Voir `docs/PALIER_F.md` pour les détails.

---

## Paliers suivants

### Palier G — Moteur d'inférence réel (llama.cpp)
**Statut** : 🟡 En cours (corrigé le 26/07/2026)

- Détection GPU + auto-ajustement `n_gpu_layers` selon VRAM estimée : ✅ fait (`crates/ainonymous-daemon/src/llama.rs`)
- Testnet 2 nœuds (loopback, pipeline-split) : ✅ fonctionnel (`docs/TESTNET_2NODES.md`), 2 backends interchangeables via `BACKEND=python|native` (le client Rust `pipeline_client.rs` est agnostique du backend — simple client HTTP)
- Patch natif llama.cpp (pipeline-split par couche, sans dépendance Python), Gemma3 Dense : ✅ preuve de concept réelle et vérifiée (voir `patches/llama-cpp-pipeline-split/`) —
  - single forward pass : bit-exact (2 prompts)
  - multi-step (prefill+decode) sans transfert de KV-cache entre nœuds : bit-exact, confirme que le KV-cache reste local par nœud (corrige l'hypothèse `kv_snapshot` de l'ancien doc)
  - serveur HTTP réel (`pipeline_server.cpp`, 2 process séparés, protocole + CLI calqués sur `pipeline_client.rs`/`pipeline_server.py`) : testé end-to-end, résultat identique au baseline in-process
  - branché dans `scripts/testnet/run_testnet_2.sh` / `make testnet-2-native` (choix de backend au lancement, aucun changement Rust nécessaire) — logique validée, pas encore exécutée de bout en bout avec un vrai `ainonymous-daemon` compilé (pas de toolchain Rust dans le sandbox de dev)
  - **`pipeline_layer_end` (early-exit, nécessaire pour N>2 nœuds) : bug diagnostiqué et corrigé** — cause racine trouvée par instrumentation (`ggml_backend_buffer_get_type` appelé sur un tenseur `inp_out_ids` jamais alloué car construit avant que la portée early-exit ne soit connue dans `gemma3.cpp`). Fix d'une ligne (construction conditionnelle), pas de bug du scheduler ggml. Vérifié bit-exact (34658/34658) sans régression sur les tests existants. Reste à vérifier : un vrai nœud du milieu (`layer_start>0` + `layer_end<n_layer` simultanément) sur un modèle à 3+ couches (le modèle de test n'a que 2 couches) ; brancher ce fix dans `pipeline_server.cpp` (qui ne l'utilise pas encore)
  - restent : MoE (Gemma4/gemma3n), GPU, build automatisé du binaire natif (pas encore vendoré/scripté), run réel du testnet natif avec cargo, concurrence serveur
- Speculative decoding, KV-cache quantization, layer splitting production : pas commencé

### Palier H — mTLS QUIC strict + PeerKeyVerifier
- Vérification complète des certificats ed25519
- NodeAttestation avant connexion

### Palier I — Observabilité & Dashboard
- Prometheus metrics
- Dashboard simple

### Palier J — Testnet public & Go-to-market
- Testnet multi-régions
- Seed funding
- Premiers pilotes

## Objectif
Atteindre Palier G + H d'ici fin août 2026.
