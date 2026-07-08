# AInonymous — Roadmap & Paliers

## Statut actuel (juillet 2026)
- Core Rust crates compilables et testés (mtls_handshake OK)
- Testnet 2 nœuds fonctionnel en mock (pipeline-split)
- HybridNode scheduler + SD-WAN policy
- DNA Holochain (zomes integrity + coordinator)
- Site marketing multi-langue + Enterprise + Défense

## Paliers de développement

### Palier E — Keyring & Rotation d'identité (en cours / presque prêt)
- NodeIdentity::load_or_generate_keyring (macOS Keychain / Windows Credential Manager / libsecret)
- Rotation de clé ed25519 via POST /daemon/rotate-identity
- Re-annonce DHT automatique
- Feature `secure-keyring` dans ainonymous-quic

### Palier F — Intégration Holochain réelle
- Remplacer le bootstrap statique par AppWebsocket réel
- Appels zome signés depuis le daemon
- Membrane Proofs + PrivateNetworkProof pour consortiums
- Warrants enforcement dans le scheduler

### Palier G — Moteur d'inférence réel (llama.cpp)
- Intégration llama.cpp (GGUF loading + pipeline-splitting)
- Détection GPU réelle (NVIDIA via nvml, AMD via rocm, Apple Metal)
- KV-cache management + speculative decoding
- Support MoE (Qwen3.6) et dense (Gemma 4 31B)

### Palier H — mTLS QUIC strict + Sécurité renforcée
- PeerKeyVerifier ed25519 complet (plus de skip-verify)
- Vérification NodeAttestation avant connexion
- ModelClaim + SHA-256 GGUF validation par pairs
- Exclusion automatique via warrants DHT

### Palier I — Observabilité & Dashboard
- Prometheus metrics complets (ainonymous_* + hybridnode_*)
- Simple web dashboard (ou intégration Grafana)
- Logs structurés + OpenTelemetry traces optionnelles
- Health checks + SLA monitoring SD-WAN

### Palier J — Testnet public & Go-to-market
- Testnet public multi-régions
- Seed funding materials (pitch deck, traction metrics)
- Documentation utilisateur finale
- Premier utilisateurs pilotes (entreprises + défense)

## Objectif court terme
Atteindre Palier G + H d'ici fin août 2026 pour pouvoir démontrer une inférence distribuée réelle avec un vrai modèle GGUF sur 2-3 nœuds physiques.