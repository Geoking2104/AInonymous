# Palier F — Intégration Holochain réelle + Membrane Proofs + Warrants

**Statut** : Membrane Proofs + Warrants de base implémentés (juillet 2026)

## 1. Membrane Proofs (terminé)

- `MembraneProofConfig` (Base64 ou File)
- Injection automatique via `call_zome_with_proof`
- Support d’installation d’app avec preuve (`install_app_with_membrane_proof`)

## 2. Warrants (implémenté - version de base)

### Types

- `Warrant`
- `WarrantType` (ModelClaim, NodeCapabilities, ExecutionProof, Custom)
- `ModelClaim`

### API Holochain

```rust
// Émettre un warrant
holochain.emit_warrant(&warrant).await?;

// Vérifier
let valid = holochain.verify_warrant(&warrant).await?;

// Récupérer les warrants d’un agent
let warrants = holochain.get_warrants_for_agent(agent_id).await?;
```

### Enforcement basique

```rust
// Avant d’assigner des couches à un nœud
if validate_node_warrants(&holochain, &agent_id, Some("gemma4-e4b")).await? {
    // assigner le travail
}
```

## 3. Prochaines étapes

- Rendre l’enforcement plus strict dans le scheduler
- Ajouter la signature réelle des warrants (ed25519)
- Intégrer les warrants dans `negotiate_quic_session` et le pipeline
- Tests + documentation d’exemples de consortiums

## Fichiers clés

- `ainonymous-types/src/warrants.rs`
- `holochain.rs` → `emit_warrant`, `verify_warrant`, `validate_node_warrants`
- `conductor_client.rs` → Membrane Proofs

Voir aussi : `ROADMAP.md`