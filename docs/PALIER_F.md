# Palier F — Intégration Holochain + Membrane Proofs + Warrants

**Statut** : Largement implémenté (juillet 2026)

## Résumé

Palier F a pour objectif de passer d’un mode testnet statique à une intégration réelle avec Holochain, incluant :

- Membrane Proofs pour les réseaux privés
- Warrants (attestations signées) pour la sécurité du mesh
- Découverte et négociation P2P via le DHT
- Scoring intelligent des nœuds (avec composante géographique)

## 1. Membrane Proofs

- `MembraneProofConfig` (Base64 ou File)
- `call_zome_with_proof` (injection automatique)
- `install_app_with_membrane_proof`
- Support des consortiums privés

## 2. Warrants

### Types
- `Warrant`
- `WarrantType` (ModelClaim, NodeCapabilities, ExecutionProof, Custom)
- `ModelClaim`

### Fonctionnalités implémentées
- `emit_warrant` / `emit_warrant_with_cleanup`
- `verify_warrant` (vérification Ed25519 + Domain Separation)
- `get_warrants` / `get_warrants_by_type`
- `validate_node_warrants` (validation stricte dans le scheduler)
- Signature sécurisée avec `zeroize` + Domain Separation (Ed25519ctx)

### Zome dédié
- `zomes/warrants/` (integrity + coordinator)
- Validation on-chain renforcée
- Liens `AgentToWarrants`

## 3. Découverte et Scoring P2P

- `discover_nodes_p2p` + cache
- `discover_nodes_p2p_optimized` (filtrage + scoring)
- Scoring intelligent :
  - VRAM (35%)
  - Charge (25%)
  - Slots disponibles (15%)
  - Proximité géographique via Haversine (10-15%)
- `build_dynamic_pipeline_plan`

## 4. Sécurité

- Gestion sécurisée des clés privées (`zeroize`)
- Domain Separation sur les signatures Ed25519
- Validation stricte des warrants (signature + expiration + issuer)
- Émission non-fatale (`try_emit_*`)

## Fichiers principaux modifiés

- `crates/ainonymous-daemon/src/holochain.rs`
- `crates/ainonymous-daemon/src/conductor.rs`
- `crates/ainonymous-daemon/src/router.rs`
- `crates/ainonymous-types/src/warrants.rs`
- `zomes/warrants/`
- `docs/PALIER_F.md` + `docs/NODE_SCORING.md`

## Statut global

Palier F est considéré comme **largement terminé**. Les fondations (Warrants, Membrane Proofs, scoring, zome) sont en place et fonctionnelles.

Prochain palier : **Palier G** (llama.cpp + inférence réelle).