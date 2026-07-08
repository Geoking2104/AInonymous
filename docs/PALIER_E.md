# Palier E — Keyring OS natif + Rotation de clé ed25519

**Statut** : ✅ Implémenté et testé (juillet 2026)

## Objectif

Remplacer le stockage de la seed ed25519 par un mécanisme sécurisé natif au système d'exploitation :

- **macOS** : Keychain
- **Windows** : Credential Manager
- **Linux** : libsecret / Secret Service

Avec fallback propre sur fichier si le keyring n'est pas disponible.

## Fonctionnalités

### 1. Chargement / Génération

```rust
let identity = NodeIdentity::load_or_generate_keyring(
    "ainonymous-daemon",
    "quic-node-identity",
    &identity_path,
)?;
```

- Tente d'abord le keyring OS
- Si indisponible ou vide → fallback fichier
- Si fichier absent → génère une nouvelle clé ed25519 et la persiste

### 2. Rotation de clé

```rust
// Via l'API REST (recommandé)
curl -X POST http://127.0.0.1:9338/daemon/rotate-identity

// Ou directement en Rust
let (old_pubkey, new_pubkey) = NodeIdentity::rotate(
    "ainonymous-daemon",
    "quic-node-identity",
    &identity_path,
)?;
```

Après rotation :
- La nouvelle clé est stockée de manière sécurisée
- L'ancienne clé reste valide jusqu'au redémarrage du daemon (mTLS existant non impacté)
- La nouvelle pubkey est automatiquement ré-annoncée dans le DHT Holochain

### 3. Endpoint REST

`POST /daemon/rotate-identity`

Réponse :
```json
{
  "old_pubkey": "<hex>",
  "new_pubkey": "<hex>",
  "restart_required": true,
  "dht_updated": true
}
```

## Tests

Tests unitaires ajoutés dans `mtls.rs` :

- `test_load_or_generate_creates_file`
- `test_rotate_file_changes_key`
- `test_rotate_returns_different_keys`

Les tests utilisent `tempfile` pour isolation.

## Intégration

- `main.rs` charge l'identité au démarrage avec fallback fichier
- `router.rs` expose l'endpoint de rotation
- `HolochainClient::reannounce_pubkey` est appelé après rotation

## Prochaines étapes (Palier F+)

- Ajouter des tests d'intégration avec un vrai keyring
- Gérer les erreurs de permission keyring de façon plus robuste
- Exposer la rotation via CLI (`ainonymous rotate-identity`)

## Fichiers modifiés

- `crates/ainonymous-quic/src/mtls.rs`
- `crates/ainonymous-daemon/src/router.rs` (endpoint déjà présent)
- `crates/ainonymous-daemon/src/main.rs` (chargement identité)
- `docs/PALIER_E.md` (ce document)