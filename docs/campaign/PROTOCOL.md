# Campagne CH3/S4 — protocole de mesure sur deux machines réelles

Ce document définit le protocole de la campagne de mesure « nœud A (PC) ↔ nœud B
(téléphone Android ou seconde machine) » des chantiers [CH3 #50](https://github.com/Geoking2104/AInonymous/issues/50)
et [S4 #30](https://github.com/Geoking2104/AInonymous/issues/30) : couches réseau
QUIC/mTLS (portes G4/G7) entre deux machines physiques distinctes, dont au moins
un téléphone (G7 s'appuie sur la même pile côté Android).

Il complète [`spike-quic/README.md`](../../spike-quic/README.md) (mesures en
boucle locale) : **aucun chiffre LAN/Wi-Fi/USB n'existe tant que ce protocole
n'a pas été exécuté sur le matériel réel.** Les seuils §6.3 du
[plan Phase 0](plan-phase-0-relay-gpu.md) restent la référence d'acceptation.

## Ce que la campagne mesure — et ne mesure pas

Mesuré :

- établissement de session QUIC/mTLS complet (handshake, épinglage des deux
  clés Ed25519, jeton à usage unique) ;
- RTT p50/p95 (écho 64 o) ;
- débit et intégrité d'un transfert de 100 Mio d'activations (SHA-256 vérifié
  des deux côtés, accusé de réception).

Non mesuré (hors périmètre de cette campagne) :

- l'inférence elle-même (découpage pipeline sur llama.cpp/PyTorch) — la porte
  G8 et CH3 complet (TTFT, tok/s, perte de nœud) viendront après la couche
  transport ;
- le mesh Holochain complet (la campagne remplace le plan de contrôle par
  l'échange d'offre hors bande, comme le spike S4) ;
- toute forme d'anonymat réseau (hors sujet du projet à ce stade).

## Outillage

Le kit `scripts/campaign/` automatise la campagne quand le nœud B est un
téléphone Android (Termux) relié par USB ou au même Wi-Fi que le PC :

| Élément | Rôle |
| --- | --- |
| [`scripts/campaign/cross-compile-android.sh`](../scripts/campaign/cross-compile-android.sh) | compilation croisée `quic-spike` → `dist/android/` (NDK r23+, cible aarch64-linux-android, API 24+) + SHA256SUMS |
| [`scripts/campaign/android/prepare-device.sh`](../scripts/campaign/android/prepare-device.sh) | vérification de l'appareil, installation Termux si besoin, poussée du binaire, redirection adb `tcp:7700` |
| [`scripts/campaign/android/worker-start.sh`](../scripts/campaign/android/worker-start.sh) | script à lancer **dans Termux** : worker QUIC + miroir de l'offre vers `/sdcard` |
| [`scripts/campaign/run-campaign.py`](../scripts/campaign/run-campaign.py) | orchestrateur côté PC : clé, offre fraîche, exécutions, agrégation, verdict §6.3 |
| [`scripts/campaign/aggregate-results.py`](../scripts/campaign/aggregate-results.py) | consolidation vers `results/ch3/` au format établi + tableau Markdown |

Pour une campagne entre deux PC (Linux/WSL/macOS), les commandes manuelles du
README de spike-quic restent la référence ; l'orchestrateur ne pilote pas
encore ce cas (suivi possible dans #50).

## Prérequis

Côté PC (nœud A, coordinateur) :

- Rust 1.91+, `cargo build --release -p relay-quic-spike` ;
- `adb` (platform-tools) pour le chemin Android ;
- Python ≥ 3.9 (stdlib uniquement) pour l'orchestrateur.

Côté téléphone (nœud B, worker) :

- Android ≥ 7.0 (API 24), ABI `arm64-v8a` ;
- [Termux](https://termux.dev) (APK F-Droid — `prepare-device.sh` le télécharge
  si besoin) ;
- ~30 Mio libres ;
- pour le lien Wi-Fi : téléphone et PC sur le même réseau ; pour le lien USB :
  débogage USB activé.

## Déroulé (chemin Android)

### 1. Compiler et préparer

```bash
# PC — compiler le binaire du PC et celui du téléphone
cargo build --release -p relay-quic-spike
bash scripts/campaign/cross-compile-android.sh

# PC — préparer le téléphone (Termux, binaire, redirection)
bash scripts/campaign/android/prepare-device.sh
```

### 2. Démarrer le worker (téléphone, une fois)

Dans Termux (`~/campaign`) :

```bash
bash worker-start.sh            # Wi-Fi : annonce l'IP détectée
ADVERTISE=127.0.0.1:7700 bash worker-start.sh   # USB (redirection adb)
```

Le script affiche la clé publique du téléphone et publie l'offre de session
(jeton TTL 30 s, régénéré toutes les 20 s) vers `/sdcard/Download/campaign/`.

### 3. Lancer la campagne (PC)

```bash
python3 scripts/campaign/run-campaign.py \
  --links usb,wifi --runs 3 \
  --hardware "PC <modèle, CPU, OS> + téléphone <modèle, Android <ver>>"
```

L'orchestrateur pousse la clé publique du PC, attend une offre fraîche à
chaque exécution (le jeton est à usage unique), lance le client de mesure,
puis agrège : `results/ch3/<lien>/<horodatage>/runs.json`.

### 4. Consolider et archiver

```bash
python3 scripts/campaign/aggregate-results.py
```

Écrit `results/ch3/campaign-<date>.json` (format de
`spike-quic/results/loopback.json` : date, machine, commande, runs) et imprime
le tableau Markdown à reporter dans le suivi de #50.

## Seuils d'acceptation (§6.3)

| Lien | Setup session | RTT p50 | Débit 100 Mio |
| --- | --- | --- | --- |
| LAN (Ethernet) | < 1 s | < 2 ms | ≥ 500 Mb/s |
| Wi-Fi | < 1 s | indicatif (pas de seuil plan) | indicatif |
| USB (adb forward) | < 1 s | indicatif (tunnel USB, pas du QUIC pur) | indicatif |
| Tailscale/VPN | < 1 s | < 50 ms | indicatif |

Le lien USB traverse le tunnel adb (TCP côté PC) : ses chiffres bornent le
chemin de développement, pas la performance QUIC du téléphone. Les seuils
Wi-Fi/USB marqués « indicatif » seront fixés après la première campagne
réelle ; le tableau du README du spike reste la référence jusqu'ici.

## Règles de publication

- ≥ 3 exécutions par lien et par condition ; rapporter min/médiane/max
  (la variance du p95 observée en boucle locale le justifie) ;
- toujours noter le matériel (modèle, OS, liaison, charge) — sans cela la
  mesure n'est pas reproductible ;
- archiver les `runs.json` bruts dans `results/ch3/` et référencer le commit
  exact du binaire (`SHA256SUMS`) ;
- ne jamais extrapoler un lien à un autre (loopback ≠ LAN ≠ Wi-Fi ≠ USB).

## Limites connues

- L'offre circule par `/sdcard` (canal hors bande assumé, comme le spike) :
  nécessite `termux-setup-storage` exécuté une fois dans Termux.
- `run-campaign.py` ne pilote pas deux PC ; pour ce cas, suivre le README du
  spike (échange manuel de l'offre).
- La redirection adb forward expose le worker sur `127.0.0.1:7700` du PC
  seulement ; le pare-feu du téléphone n'a pas à être ouvert en USB.
