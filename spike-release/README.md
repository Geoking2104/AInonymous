# Spike S5 — Release signée, refus d'installeur altéré, rollback (porte G5, #31)

Crate `relay-release-spike` (bibliothèque `relay_release` + CLI `relay-release`).
Ce code sert à valider le **mécanisme** de mise à jour. Ce n'est pas encore
l'updater de production.

## Ce qui est validé ici

| Critère G5 / #32 | Mécanisme | Test |
|---|---|---|
| Manifeste signé vérifié | Ed25519 (`verify_strict`) sur JSON canonique (clés triées) préfixé d'un contexte de domaine `ainonymous-relay/release-manifest/v1` | `manifeste_signe_valide_est_accepte` |
| Clé non autorisée | trust store local (clés épinglées), `key_id` doit y figurer | `cle_non_autorisee_refusee`, `usurpation_de_key_id_refusee` |
| Signature invalide / manifeste altéré | toute modification après signature invalide la signature | `manifeste_altere_apres_signature_refuse`, `signature_tronquee_refusee`, `champ_inconnu_refuse_au_parsing` |
| Mauvais hash d'installeur / APK | taille puis SHA-256 en flux, avant toute installation, et vérifié à nouveau dans le répertoire de staging | `artefact_avec_mauvais_hash_refuse`, `artefact_avec_mauvaise_taille_refuse`, `install_refuse_mauvais_hash_sans_modifier_l_etat` |
| Rejeu | `sequence` strictement croissante par cible `produit/canal/plateforme`, persistée | `rejeu_du_meme_manifeste_refuse`, `ancien_manifeste_valide_rejoue_refuse` |
| Rétrogradation (downgrade) | version semver refusée si elle est inférieure à la version installée, même avec une nouvelle séquence | `retrogradation_avec_nouvelle_sequence_refusee` |
| Mauvaise cible | produit, canal et plateforme signés dans le manifeste | `mauvaise_plateforme_refusee` |
| Fenêtre de validité | `issued_at`/`expires_at` avec une tolérance de 300 s | `manifeste_expire_ou_futur_refuse` |
| Path traversal | noms d'artefacts sans séparateur ni `..` | `nom_artefact_avec_chemin_refuse` |
| Rollback | retour local à la version précédente (artefact vérifié de nouveau sur disque). La séquence n'est **pas** abaissée, donc un ancien manifeste reste refusé | `rollback_explicite_restaure_la_version_precedente`, `rollback_refuse_si_version_precedente_alteree_sur_disque`, `rollback_sans_version_precedente_refuse` |
| Rollback automatique | échec du contrôle de santé après l'installation : retour arrière et mise en quarantaine de la version | `echec_du_controle_de_sante_declenche_le_rollback` |
| Persistance | état écrit de façon atomique (tmp + rename) | `etat_persiste_entre_redemarrages` |
| CLI de bout en bout | keygen → manifest → sign → verify → install → rejeu/hash refusés | `tests/cli.rs` |

```bash
cargo test -p relay-release-spike     # 21 tests
```

## Exemple CLI

```bash
relay-release keygen --out release.key                  # affiche la clé publique
relay-release manifest --platform android-arm64 --version 0.1.0 --sequence 1 \
    --artifact relay.apk --kind apk --url https://example.invalid/relay.apk --out manifest.json
relay-release sign --key release.key --manifest manifest.json --out manifest.signed.json
relay-release verify --signed manifest.signed.json --trust <PUBKEY_HEX> --platform android-arm64
relay-release install --signed manifest.signed.json --trust <PUBKEY_HEX> --platform android-arm64 \
    --artifact relay.apk --root ./install-root
relay-release rollback --platform android-arm64 --root ./install-root
```

## Hors périmètre du spike (à couvrir en phase 1)

- La signature au niveau de l'OS reste nécessaire **en plus** du manifeste :
  Authenticode/`signtool` pour l'installeur Windows et APK Signature Scheme v2/v3
  (`apksigner verify`) pour Android. Sur Android, l'OS refuse déjà une mise à jour
  signée par une autre clé. Le manifeste ajoute l'anti-rejeu, l'anti-downgrade
  et le choix du canal.
- La garde de la clé de release (HSM ou clé hors ligne) et la rotation (plusieurs
  clés dans le trust store, révocation) ne sont pas traitées ici.
- Le téléchargement HTTP(S) n'est pas implémenté : le spike lit les artefacts
  depuis un dossier local.
