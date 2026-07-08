# Zome Warrants — Documentation

Zome dédié à la gestion des **Warrants** (attestations signées par les nœuds).

## Fonctionnalités

| Fonction                    | Description                                      | Sécurité                          |
|-----------------------------|--------------------------------------------------|-----------------------------------|
| `emit_warrant`              | Émet un warrant                                  | Basique                           |
| `emit_warrant_with_cleanup` | Émet un warrant en supprimant les anciens du même type | Recommandé (rotation)        |
| `verify_warrant`            | Vérifie un warrant (Ed25519 + Domain Separation) | Fort (Ed25519ctx)                 |
| `get_warrants`              | Récupère tous les warrants d'un agent            | -                                 |
| `get_warrants_by_type`      | Récupère les warrants d'un type précis           | Efficace grâce aux liens          |

## Sécurité

- **Domain Separation** : `AInonymous-Warrant-v1`
- **Validation on-chain** : L'issuer doit correspondre à l'agent créateur
- **Signature Ed25519** avec contexte
- **Vérification d'expiration**

## Utilisation depuis le daemon

```rust
// Émission sûre (non-fatale)
holochain.try_emit_model_claim("gemma4-e4b", hash, &identity).await?;

// Vérification
let valid = holochain.verify_warrant(&warrant).await?;

// Récupération ciblée
let model_claims = holochain
    .call_zome_with_proof("warrants", "coordinator", "get_warrants_by_type", json!({ 
        "agent_id": agent, 
        "warrant_type": "model_claim" 
    }))
    .await?;
```

## Structure

- `integrity/` : Définition des EntryTypes + règles de validation
- `coordinator/` : Fonctions publiques + logique métier

## Bonnes pratiques

- Utiliser `emit_warrant_with_cleanup` lors d'une rotation de clé
- Toujours vérifier les warrants avant d'assigner du travail (`validate_node_warrants`)
- Préférer `get_warrants_by_type` pour les requêtes ciblées

## Statut

Zome fonctionnel et sécurisé. Intégré au daemon via `HolochainClient`.