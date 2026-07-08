# Zome Warrants

Zome dédié à l'émission, la vérification et le stockage des Warrants (attestations signées).

## Structure

- `integrity/` : Définition des EntryTypes + validation
- `coordinator/` : Fonctions publiques (`emit_warrant`, `verify_warrant`, `get_warrants`)

## Fonctions exposées

- `emit_warrant(warrant: Warrant) -> ActionHash`
- `verify_warrant(warrant: Warrant) -> bool`
- `get_warrants(agent_id: String) -> Vec<Warrant>`

## TODO

- Implémenter la vraie vérification cryptographique dans `verify_warrant`
- Filtrer correctement par issuer dans `get_warrants`
- Ajouter des liens entre warrants et agents
- Intégrer avec le zome `agent-registry`

## Utilisation depuis le daemon

```rust
holochain.emit_warrant(&warrant).await?;
let valid = holochain.verify_warrant(&warrant).await?;
```