# AInonymous — Roadmap & Paliers

## Statut actuel (juillet 2026)

**Palier E — Keyring OS natif + Rotation ed25519** : ✅ **Finalisé et testé**

- `NodeIdentity::load_or_generate_keyring` (keyring OS + fallback fichier)
- `NodeIdentity::rotate` et `rotate_file`
- Endpoint REST `POST /daemon/rotate-identity`
- Ré-annonce automatique dans le DHT
- Tests unitaires ajoutés (`tempfile`)

Voir : `docs/PALIER_E.md`

---

## Paliers futurs

### Palier F — Intégration Holochain réelle
- Remplacer bootstrap statique par `AppWebsocket` réel
- Appels zome signés
- Membrane Proofs + PrivateNetworkProof

### Palier G — Moteur d'inférence réel (llama.cpp)
- Intégration llama.cpp GGUF
- Détection GPU réelle (NVIDIA/AMD/Apple)
- Pipeline-splitting + speculative decoding

### Palier H — mTLS QUIC strict
- `PeerKeyVerifier` ed25519 complet
- Vérification `NodeAttestation` avant connexion

### Palier I — Observabilité & Dashboard
- Prometheus + métriques complètes
- Dashboard simple ou Grafana

### Palier J — Testnet public & Go-to-market
- Testnet multi-régions
- Seed funding prep
- Premiers pilotes entreprises/défense

## Objectif
Atteindre Palier G+H d'ici fin août 2026 pour démontrer une inférence distribuée réelle.