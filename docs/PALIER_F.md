# Palier F — Intégration Holochain réelle + Membrane Proofs + Warrants

**Statut** : Membrane Proofs + Warrants de base implémentés (juillet 2026)

## Objectif du Palier F

Passer d’un mode testnet statique (bootstrap via `peers`) à une intégration réelle avec un conducteur Holochain, incluant :

- Appels de zome signés
- Membrane Proofs pour les réseaux privés / consortiums
- Warrants (attestations signées) pour la sécurité du mesh

---

## 1. Membrane Proofs (implémenté)

### Configuration

```toml
[holochain]
backend = "conductor"
admin_port = 8888
app_port = 8890

[holochain.membrane_proof]
Base64 = "<base64-encoded-membrane-proof>"
# ou
# File = { path = "/chemin/vers/proof.bin" }
```

### Fonctionnalités

- `call_zome_with_proof()` : injection automatique de la preuve dans les payloads
- `install_app_with_membrane_proof()` : installation d’une `.happ` avec preuve (consortiums privés)

---

## 2. Warrants (implémenté - version de base)

### Types

```rust
pub struct Warrant {
    pub issuer: [u8; 32],
    pub warrant_type: WarrantType,
    pub payload: Value,
    pub signature: Vec<u8>,
    pub issued_at: u64,
    pub ttl_seconds: u64,
}

pub enum WarrantType {
    ModelClaim,
    NodeCapabilities,
    ExecutionProof,
    Custom(String),
}

pub struct ModelClaim { ... }
```

### API

```rust
// Émettre
holochain.emit_warrant(&warrant).await?;

// Vérifier
let valid = holochain.verify_warrant(&warrant).await?;

// Récupérer
let warrants = holochain.get_warrants_for_agent(agent_id).await?;

// Enforcement basique avant d’assigner du travail
if validate_node_warrants(&holochain, &agent_id, Some("gemma4")).await? {
    // assigner le pipeline
}
```

### État actuel

- Types et API de base présents
- Enforcement simple implémenté
- Signature cryptographique réelle et zome `warrants` → à compléter

---

## 3. Configuration recommandée (mode réel)

```toml
[holochain]
backend = "conductor"
admin_port = 8888
app_port = 8890
membrane_proof = { Base64 = "..." }   # optionnel pour réseaux privés
```

---

## Fichiers modifiés

- `crates/ainonymous-types/src/warrants.rs`
- `crates/ainonymous-daemon/src/config.rs`
- `crates/ainonymous-daemon/src/conductor_client.rs`
- `crates/ainonymous-daemon/src/holochain.rs`
- `docs/PALIER_F.md`

---

## Prochaines étapes

- Rendre l’enforcement des warrants plus strict dans le scheduler
- Implémenter la signature réelle des warrants
- Ajouter un zome dédié `warrants`
- Tests d’intégration

Voir aussi : `ROADMAP.md`