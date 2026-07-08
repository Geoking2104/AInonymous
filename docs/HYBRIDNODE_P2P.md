# HybridNode P2P Architecture

AInonymous utilise une architecture **HybridNode** qui combine :

- **Holochain** (plan de contrôle P2P) : DHT, warrants, membrane proofs, découverte de nœuds, négociation de sessions.
- **QUIC + mTLS ed25519** (plan de données) : transfert haute performance des activations tensorielles.

## Flux P2P typique

1. Le coordinateur appelle `discover_nodes_p2p(model_id)` → récupère les nœuds via le DHT Holochain.
2. Pour chaque nœud du plan, il appelle `negotiate_quic_session_p2p(...)` → le zome `inference-mesh` fait un `call_remote` P2P.
3. Une session QUIC mTLS est établie directement entre les nœuds (data plane).
4. Les warrants sont vérifiés avant d'assigner du travail.

## Avantages

- Découverte et négociation 100% P2P via Holochain (pas de serveur central).
- Sécurité via warrants + membrane proofs.
- Performance via QUIC direct pour les tenseurs.

## Méthodes recommandées

- `holochain.discover_nodes_p2p(model_id)`
- `holochain.negotiate_quic_session_p2p(...)`
- `holochain.emit_warrant_with_cleanup(...)`

## Mode Static vs Conductor

| Mode       | Découverte     | Négociation          | Recommandé pour          |
|------------|----------------|----------------------|--------------------------|
| Static     | Fichier config | REST direct          | Testnet rapide           |
| Conductor  | DHT Holochain  | call_remote P2P      | Réseaux privés / prod    |
