# HybridNode

Architecture commune pour les projets Holochain + SD-WAN.

## Qu'est-ce que HybridNode ?

HybridNode est une couche de scheduling et de configuration réutilisable qui combine :
- **SD-WAN** comme underlay de transport (connectivité, QoS, failover)
- **Holochain** comme overlay applicatif (identité, coordination, validation, audit)
- **QUIC/mTLS** comme plan de données (flux lourds directs entre nœuds)

## Structure

```
hybridnode/
├── specs/hybridnode.yaml              # Spécification complète du format de config
├── configs/
│   ├── ainonymous.hybridnode.yaml     # Config de référence pour AInonymous
│   └── generic-project.hybridnode.yaml # Template vierge pour nouveaux projets
├── policies/
│   ├── sdwan-policy.yaml             # Politiques de trafic SD-WAN
│   ├── security-baseline.yaml        # Baseline de sécurité
│   ├── observability.yaml            # Config observabilité
│   └── model-validation.yaml         # Règles de validation des modèles
└── schemas/
    └── hybridnode.schema.json         # JSON Schema pour validation des configs
```

## Démarrage rapide

```bash
# Valider une config
python3 scripts/hybridnode/validate_config.py hybridnode/configs/ainonymous.hybridnode.yaml

# Initialiser HybridNode pour un nouveau projet
bash scripts/hybridnode/init_project.sh mon-projet
```

## Documentation

- `docs/HYBRIDNODE.md` — Spécification technique complète
- `docs/HYBRIDNODE_ARCHITECTURE.md` — Architecture détaillée + diagrammes
- `docs/HYBRIDNODE_CARGO_PATCH.md` — Intégration Rust/Cargo
- `HYBRIDNODE_APPLY.md` — Guide d'intégration dans un projet existant
