# Rejoindre le réseau — guide de mise en route

*Version texte du guide interactif du site
([geoking2104.github.io/AInonymous/#rejoindre](https://geoking2104.github.io/AInonymous/#rejoindre)).
Le guide du site et ce document décrivent les mêmes étapes ; toute commande ajoutée
ici doit exister dans le dépôt.*

> **À lire avant de commencer.** AInonymous est un logiciel **alpha**. Le mode
> public est expérimental (développement uniquement), le réseau **n'offre pas
> d'anonymat** ([SECURITY.md](SECURITY.md)) et le calcul réparti n'a pas encore été
> mesuré sur plusieurs machines réelles ([ROADMAP](../ROADMAP.md)). Il n'y a pas de
> compte en ligne : votre identité est une clé générée sur votre machine. Pour un
> **mesh privé**, l'administrateur du réseau vous délivre une preuve d'admission.

## Choisir son parcours

| Objectif | Parcours | Persistance |
| --- | --- | --- |
| Découvrir, faire tourner un nœud sur sa machine | [A. Nœud local d'essai](#a-nœud-local-dessai) | Jetable (keystore de test) |
| Faire tourner un nœud durable | [B. Pile Docker](#b-nœud-persistant-pile-docker) | Volumes Docker versionnés |
| Rejoindre un mesh privé existant | [C. Membre d'un mesh privé](#c-rejoindre-un-mesh-privé) | Selon votre conducteur |
| Opérer un mesh privé | [D. Administrateur](#d-créer-un-mesh-privé-administrateur) | — |
| Vérifier ou contribuer au code | [E. Tests](#e-tester--contribuer) | — |

## Prérequis communs

```bash
git clone https://github.com/Geoking2104/AInonymous.git
cd AInonymous
rustup target add wasm32-unknown-unknown    # Rust 1.91 ou plus récent
```

Holochain **0.7.0** (`hc`, `holochain`, Lair 0.7.1) — les scripts vérifient les
empreintes SHA-256 des binaires officiels :

```bash
# x86_64 Linux ou WSL2
bash scripts/setup_holochain_wsl.sh
```

```powershell
# x86_64 Windows
powershell -ExecutionPolicy Bypass -File scripts/setup_holochain_windows.ps1
```

```bash
hc --version          # doit afficher 0.7.0
holochain --version   # doit afficher 0.7.0
```

Le dépôt ne fournit pas de script pour macOS ni ARM : installez `hc` et `holochain`
0.7.0 depuis les releases officielles de Holochain. Sous Windows natif, compilez les
zomes avec un `CARGO_TARGET_DIR` court (par exemple `C:\wz-ain`) pour éviter
l'erreur `LNK1104` ; les scripts Bash se lancent depuis WSL ou Git Bash.

## A. Nœud local d'essai

S'appuie sur les configurations de test du dépôt
(`scripts/testnet/conductor_t51.yaml`, `scripts/testnet/daemon_t51.toml`).

1. **Compiler et packer**

   ```bash
   cargo build --workspace
   bash scripts/build-happ.sh release
   ```

   Produit `dnas/ainonymous-core/ainonymous-core.happ` et
   `dnas/hybridnode/hybridnode.happ`. Une recompilation locale change l'empreinte
   des DNA : suffisant pour un essai, pas pour rejoindre un réseau épinglé (voir
   [DNA_HASHES.md](DNA_HASHES.md)).

2. **Démarrer le conducteur de test** (laisser le terminal ouvert)

   ```bash
   holochain -c scripts/testnet/conductor_t51.yaml
   ```

   Admin sur le port 65000, données sous `/tmp/ainonymous-t51`, keystore
   `danger_test_keystore` (non adapté à une identité persistante), bootstrap public
   `bootstrap.holo.host`.

3. **Installer le hApp** (mêmes commandes que
   `deploy/containers/holochain/entrypoint.sh`, sur les ports de test)

   ```bash
   hc client call --port 65000 new-agent          # renvoie une clé uhCAk…
   hc client call --port 65000 install-app --app-id ainonymous \
     --agent-key <uhCAk...> dnas/ainonymous-core/ainonymous-core.happ
   hc client call --port 65000 enable-app ainonymous
   hc client call --port 65000 add-app-ws 65001 --allowed-origins "*" \
     --installed-app-id ainonymous
   ```

4. **Lancer le daemon**

   ```bash
   AINON_CONFIG=scripts/testnet/daemon_t51.toml ./target/debug/ainonymous-daemon
   curl http://127.0.0.1:18890/health
   ```

   Sans `AINON_CONFIG`, le daemon crée une configuration par défaut dans
   `~/.config/ainonymous/config.toml` (API sur 8890). Une panne du conducteur est
   fatale : pas de repli silencieux. L'inférence réelle demande `llama-server`
   (`llama_server_bin`) et un modèle dans `models_dir`.

5. **Lancer le proxy OpenAI-compatible**

   ```bash
   AINON_DAEMON_URL=http://127.0.0.1:18890 ./target/debug/ainonymous-proxy
   curl http://127.0.0.1:9337/health
   curl http://127.0.0.1:9337/v1/ainonymous/mesh/status
   ```

   `AINON_DAEMON_URL` est l'URL de l'API HTTP du daemon. Par défaut :
   `http://127.0.0.1:8890` (port `daemon_port` de la config par défaut) ; la
   config de test t51 utilise 18890. L'ancienne variable `AINON_HOLOCHAIN_APP_PORT`
   reste acceptée comme port du daemon, avec un avertissement de dépréciation.

   Variante : `./target/debug/ainonymous start` lance le daemon puis le proxy
   (binaires du même répertoire) et attend que chacun réponde sur `/health`.
   Il hérite de `AINON_CONFIG` et de `AINON_DAEMON_URL` ; le conducteur Holochain
   (étape précédente) doit déjà tourner.

## B. Nœud persistant (pile Docker)

Prérequis : Docker Engine + Compose v2, Rust 1.91+, cible WASM, `hc` 0.7.0 local,
OpenSSL pour le script Bash. Détails : [deploy/containers](../deploy/containers/README.md).

```bash
docker compose version
/absolute/path/to/hc --version
```

Provisionnement — **destructif** pour les volumes nommés du projet
`ainonymous-hc07-v3` :

```bash
HC_BIN=/absolute/path/to/hc ./scripts/reprovision-containers.sh --confirm-reset
```

```powershell
.\scripts\reprovision-containers.ps1 -ConfirmReset -HcBin C:\absolute\path\to\hc.exe
```

Vérification :

```bash
docker compose --project-name ainonymous-hc07-v3 ps
docker compose --project-name ainonymous-hc07-v3 logs --follow holochain
curl http://127.0.0.1:8890/health
```

Seule l'API du daemon est exposée (`127.0.0.1:8890`) ; QUIC sur 9000/UDP. La
configuration de référence utilise le bootstrap/relais publics de Holochain.

## C. Rejoindre un mesh privé

L'administrateur vous fournit : le bundle `hybridnode.happ` du réseau, l'identifiant
réseau, le seed réseau, la clé publique d'administration et, après réception de
votre clé d'agent, votre preuve `proof.b64`.

1. **Générer votre clé d'agent** et l'envoyer à l'administrateur
   (`<ADMIN_PORT>` = port admin de votre conducteur, 65000 avec la config de test) :

   ```bash
   hc client call --port <ADMIN_PORT> new-agent
   ```

2. **Vérifier la preuve hors ligne**

   ```bash
   cargo run -p private-network-proof -- verify \
     --network-id <network-id> --agent-key <uhCAk...> \
     --admin-pubkey-hex <raw 36-byte hex> --proof proof.b64
   ```

3. **Préparer les role settings** — la preuve est fournie à la genèse sous forme
   de suite d'entiers YAML, comme `roles_yaml()` dans `scripts/sg1/run_sg1.py`. Le
   nom du rôle est celui du bundle (`hybridnode-core` dans `dnas/hybridnode/happ.yaml`) :

   ```bash
   python3 - <<'PY'
   import base64, pathlib
   raw = base64.b64decode(pathlib.Path('proof.b64').read_bytes().strip())
   nums = ', '.join(str(b) for b in raw)
   pathlib.Path('roles.yaml').write_text(
       f'hybridnode-core:\n  type: provisioned\n  membrane_proof: [{nums}]\n')
   PY
   ```

4. **Installer avec la preuve**

   ```bash
   hc client call --port <ADMIN_PORT> install-app <hybridnode.happ> <network-seed> roles.yaml \
     --app-id hybridnode --agent-key <uhCAk...>
   hc client call --port <ADMIN_PORT> enable-app hybridnode
   ```

   Preuve valide → l'app s'installe ; sinon la genèse est refusée.

## D. Créer un mesh privé (administrateur)

1. **Clé d'administration** — le seed (32 octets hex) est **secret** : il signe
   toutes les preuves.

   ```bash
   cargo run -p private-network-proof -- admin-pubkey --seed-hex <64 hex chars>
   ```

2. **Propriétés DNA** — renseigner `network_id` et `network_admin_pubkey` (clé brute
   de 36 octets) dans `dnas/hybridnode/dnas/hybridnode-core/workdir/dna.yaml`, puis :

   ```bash
   bash scripts/build-happ.sh release
   ```

   Changer ces propriétés change le hash : c'est un nouvel epoch. Distribuez ce
   bundle à tous les membres.

3. **Une preuve par membre**

   ```bash
   cargo run -p private-network-proof -- proof \
     --network-id <network-id> --agent-key <uhCAk...> \
     --seed-hex <64 hex chars> --out proof.b64
   ```

4. **Valider la configuration et l'admission**

   ```bash
   bash scripts/hybridnode/init_project.sh my-project
   python scripts/hybridnode/validate_config.py --production my-project/hybridnode.yaml
   python3 scripts/sg1/run_sg1.py
   ```

Limites actuelles : pas encore de bootstrap/relais privés opérés par
l'administrateur ; l'expiration d'une preuve n'est qu'un contrôle à l'émission ; la
révocation en temps réel est en cours ([PR #57](https://github.com/Geoking2104/AInonymous/pull/57)).

## E. Tester / contribuer

```bash
cargo test --workspace
python3 scripts/sg1/run_sg1.py
cargo build --workspace
BIN="$PWD/target/debug" TOTAL_LAYERS=18 bash scripts/testnet/run_testnet_2_mock.sh
```

Le testnet mock valide le flux de contrôle et les trames, pas l'équivalence
numérique d'un vrai modèle ([TESTNET_2NODES.md](TESTNET_2NODES.md)).

## Ports

| Service | Port par défaut | Config de test (t51) |
| --- | --- | --- |
| Admin Holochain | 8888 | 65000 |
| WebSocket app `ainonymous` | 8889 | 65001 |
| WebSocket app `hybridnode` | 8891 | — |
| API HTTP du daemon (`AINON_DAEMON_URL` côté proxy) | 8890 | 18890 |
| QUIC (UDP) | 9000 | 9010 |
| Proxy OpenAI-compatible | 9337 | 9337 |

## Dépannage

| Symptôme | Piste |
| --- | --- |
| `Manifest rejected` | `hc --version` doit afficher 0.7.0 |
| `installed_hash` mismatch | Bundle recompilé localement : installer les bundles CI/release ou l'image `deploy/containers/repro/Dockerfile` |
| `No cell found` | App ID, app activée, rôle provisionné |
| `Unauthorized app WebSocket` | Émettre un nouveau jeton via l'interface admin locale |
| `Genesis rejected` | Clé d'agent de la preuve, identifiant réseau, signature, propriétés DNA |
| Les pairs ne se voient pas | DNA hashes, seeds réseau, bootstrap/relais identiques |
| Windows `LNK1104` | `CARGO_TARGET_DIR` court |
| `/v1/ainonymous/mesh/nodes` en 503 | Le proxy ne joint pas le daemon : vérifier `AINON_DAEMON_URL` et `curl $AINON_DAEMON_URL/health` ; le daemon renvoie lui-même 503 si le conducteur ne répond pas |

## Le guide interactif

Le guide du site (`site/setup-guide.js`, `site/setup-guide.css`) est un **assistant
scripté** : il pose trois questions (objectif, système, matériel), affiche ces
étapes une par une avec leurs commandes, garde la progression dans le
`localStorage` du navigateur et répond aux questions fréquentes par mots-clés.
Il fonctionne entièrement côté client, sans clé d'API ni appel réseau.

Un **mode LLM local** optionnel envoie les questions à un point d'accès
OpenAI-compatible sur `localhost`/`127.0.0.1` (par défaut le proxy du dépôt,
`http://127.0.0.1:9337/v1`). Le proxy n'envoie pas encore d'en-têtes CORS : depuis
le site en ligne, le navigateur bloquera probablement l'appel, et le guide revient
alors automatiquement au mode scripté.

Pour modifier le guide, mettez à jour ce document **et** `site/setup-guide.js`
dans la même pull request.
