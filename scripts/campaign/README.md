# Kit de campagne CH3/S4 — mesure sur deux machines réelles

Outils pour exécuter la campagne de mesure réseau (portes G4/G7) entre un PC
(nœud A, coordinateur) et un téléphone Android (nœud B, worker) : compilation
croisée du binaire de mesure QUIC/mTLS pour Android, préparation de l'appareil,
orchestration des exécutions, agrégation des résultats.

**Protocole complet : [docs/campaign/PROTOCOL.md](../../docs/campaign/PROTOCOL.md)** —
ce qui est mesuré, les seuils §6.3, les règles de publication.

## Démarrage rapide (chemin Android)

```bash
# 0. PC — compiler le binaire de mesure du PC
cargo build --release -p relay-quic-spike

# 1. PC — compiler le binaire Android (NDK r23+ requis)
bash scripts/campaign/cross-compile-android.sh

# 2. PC — préparer le téléphone : Termux + binaire + redirection adb 7700
bash scripts/campaign/android/prepare-device.sh

# 3. Téléphone — dans Termux (~/campaign), démarrer le worker
bash worker-start.sh                          # Wi-Fi (annonce l'IP détectée)
ADVERTISE=127.0.0.1:7700 bash worker-start.sh # ou liaison USB (adb forward)

# 4. PC — lancer la campagne (≥ 3 exécutions par lien)
python3 scripts/campaign/run-campaign.py --links usb,wifi --runs 3 \
  --hardware "PC <modèle> + téléphone <modèle, Android <ver>>"

# 5. PC — consolider les résultats vers results/ch3/ + tableau Markdown
python3 scripts/campaign/aggregate-results.py
```

## Contenu

| Fichier | Rôle |
| --- | --- |
| `cross-compile-android.sh` | compilation croisée `relay-quic-spike` → `dist/android/quic-spike` (aarch64, API 24+) + `SHA256SUMS` |
| `android/prepare-device.sh` | appareil, Termux, poussée du binaire, `adb forward tcp:7700` |
| `android/worker-start.sh` | worker QUIC côté téléphone (Termux) + miroir de l'offre vers `/sdcard` |
| `run-campaign.py` | orchestrateur côté PC : clé A, offre fraîche (TTL 30 s), exécutions, agrégation `results/ch3/<lien>/<date>/runs.json` |
| `aggregate-results.py` | consolidation au format `results/` du dépôt + tableau Markdown |
| `tests/test_campaign_scripts.py` | tests unitaires (unittest, stdlib seule) exécutés en CI |

## Tests

```bash
python -m unittest discover -s scripts/campaign/tests -v
```

## Ce que ce kit n'est pas

- Ce n'est pas un canal de production : le binaire est le spike S4 (mesure),
  pas le data plane du produit.
- Ce n'est pas un APK complet : le binaire natif s'exécute dans Termux —
  l'application Android de référence est le chantier [CH2 #49](https://github.com/Geoking2104/AInonymous/issues/49).
- Ce n'est pas la mesure G8/CH3 complète (inférence pipeline sur machines
  réelles) : celle-ci vient après la couche transport ; voir [#50](https://github.com/Geoking2104/AInonymous/issues/50).
