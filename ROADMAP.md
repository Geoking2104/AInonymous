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
- Testnet 2 nœuds (loopback, pipeline-split via `pipeline_server.py` Python/HuggingFace) : ✅ fonctionnel (`docs/TESTNET_2NODES.md`), mais decode séquentiel, KV-cache purgée par session
- Patch natif llama.cpp (pipeline-split par couche, sans dépendance Python) : 🟡 preuve de concept réelle et vérifiée sur Gemma3 Dense (voir `patches/llama-cpp-pipeline-split/`), architecture MoE + endpoint serveur + migration `conductor.rs` restent à faire
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
