# Zome Warrants - Gestion des conflits de liens

## Problème résolu

Lorsqu'un nœud émet plusieurs warrants du même type (ex: rotation de clé ou mise à jour de ModelClaim), on peut avoir des liens dupliqués ou des conflits.

## Solutions implémentées

### 1. `emit_warrant` (classique)
Crée un nouveau lien à chaque appel. Simple mais peut créer des doublons.

### 2. `emit_warrant_with_cleanup` (recommandé)
- Supprime d'abord tous les anciens liens du **même type** de warrant
- Puis crée le nouveau
- Évite les conflits et les doublons

### 3. Requêtes ciblées
- `get_warrants(agent_id)` → tous les warrants
- `get_warrants_by_type(agent_id, WarrantType)` → seulement un type précis (ex: seulement les ModelClaim)

## Recommandation

Utiliser `emit_warrant_with_cleanup` quand on veut remplacer un warrant existant (rotation de clé, mise à jour de capacités, etc.).

## Exemple d'utilisation depuis le daemon

```rust
// Rotation de clé → on veut remplacer l'ancien warrant
let new_warrant = Warrant::new_signed(...);
holochain.emit_warrant_with_cleanup(new_warrant).await?;
```