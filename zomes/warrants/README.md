# Zome Warrants (version améliorée)

Zome complet pour l'émission, la vérification et le stockage des Warrants avec liens.

## Fonctions

- `emit_warrant(warrant)` → crée l'entrée + lien Agent → Warrant
- `verify_warrant(warrant)` → vérification ed25519 réelle
- `get_warrants(agent_id)` → récupère via les liens

## Améliorations apportées

- LinkTypes (`AgentToWarrants`)
- Vérification cryptographique réelle
- Récupération optimisée via liens

## TODO suivant

- Connecter `emit_warrant` automatiquement depuis le daemon (après rotation de clé)
- Ajouter plus de validation dans l'integrity zome
