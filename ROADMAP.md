# AInonymous — Roadmap & Paliers

## Statut actuel (juillet 2026)

### Palier E — Keyring OS natif + Rotation ed25519
**Statut** : ✅ Finalisé et testé

### Palier F — Intégration Holochain réelle + Membrane Proofs + Warrants
**Statut** : Membrane Proofs + Warrants de base implémentés

- `MembraneProofConfig` + injection automatique
- `call_zome_with_proof` et `install_app_with_membrane_proof`
- Types `Warrant` + `ModelClaim`
- `emit_warrant`, `verify_warrant`, `get_warrants_for_agent`
- `validate_node_warrants` (enforcement basique)

Voir `docs/PALIER_F.md` pour les détails.

---

## Paliers suivants

### Palier G — Moteur d'inférence réel (llama.cpp)
- Intégration llama.cpp + GGUF
- Détection GPU réelle (NVIDIA/AMD/Apple Metal)
- Pipeline splitting + speculative decoding

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