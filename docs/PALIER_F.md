# Palier F — Intégration Holochain réelle + Membrane Proofs

**Statut** : En cours (juillet 2026)

## Objectif

Passer du mode "bootstrap statique" (testnet) à une intégration réelle avec un conducteur Holochain :

- Appels de zome signés via `AppWebsocket`
- Membrane Proofs / PrivateNetworkProof pour les réseaux privés et consortiums
- Support d’installation d’apps avec preuve
- Préparation aux Warrants (Palier F avancé)

## 1. Membrane Proofs (implémenté)

### Configuration

```toml
[holochain]
backend = "conductor"
admin_port = 8888
app_port = 8890

[holochain.membrane_proof]
Base64 = "<base64-encoded-proof>"
# ou
# File = { path = "/chemin/vers/private-network-proof.bin" }
```

### Utilisation automatique

Le daemon injecte automatiquement la `membrane_proof` dans les payloads des zome calls stratégiques via :

```rust
client.call_zome_with_proof("inference-mesh", "coordinator", "some_function", payload).await?
```

### Installation d’une happ avec Membrane Proof

```rust
let mut admin = AdminWebsocket::connect(...).await?;

conductor_client
    .install_app_with_membrane_proof(
        &mut admin,
        "ainonymous-private",
        Path::new("./ainonymous.happ"),
        Some(membrane_proof_bytes),
    )
    .await?;
```

## 2. Backend Holochain réel

Le daemon supporte deux modes :

| Mode       | Description                              | Quand l’utiliser                  |
|------------|------------------------------------------|-----------------------------------|
| `Static`   | Bootstrap via `peers` + REST             | Testnet rapide, développement     |
| `Conductor`| Vrai conducteur Holochain + DHT          | Réseaux privés, production        |

Activation :

```toml
[holochain]
backend = "conductor"
```

## 3. Prochaines étapes

- [ ] Utilisation systématique de `call_zome_with_proof` sur les fonctions critiques
- [ ] Warrants enforcement (Palier F avancé)
- [ ] Tests d’intégration avec un vrai conducteur
- [ ] Documentation d’exemples de consortiums privés

## Fichiers modifiés

- `config.rs` → `MembraneProofConfig`
- `conductor_client.rs` → `call_zome_with_proof` + `install_app_with_membrane_proof`
- `holochain.rs` → propagation de la preuve

Voir aussi : `ROADMAP.md`