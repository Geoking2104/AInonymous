# Phase 0 — Environnement des spikes (#26)

> Ce document décrit l'environnement **effectivement installé** le 27/10/2026 sur
> la box de développement Linux (VM partagée, sans GPU ni téléphone), qui a servi
> aux spikes S1, S4 et S5. Le poste Windows de Geoffroy est décrit au §3 (besoins)
> et au §4 (état effectif au 29/10/2026).

## 1. Machine

| Élément | Valeur |
|---|---|
| OS | Debian GNU/Linux 13 (trixie), noyau 6.12 |
| CPU / RAM | 8 vCPU Intel Xeon, 15 Gio |
| GPU | **aucun** (S2/S3 et les critères GPU de S4 ne sont pas mesurables ici) |
| Disque | 128 Go |

## 2. Outils installés

| Outil | Version | Installation | Écart au plan |
|---|---|---|---|
| Rust (rustup, stable) | rustc/cargo 1.99.1 | `rustup` (le rustc 1.85 du système est < `rust-version` 1.91 du workspace) | — |
| Cibles Rust | `aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`, `x86_64-linux-android`, `wasm32-unknown-unknown` | `rustup target add` (les 3 dernières cibles Android ont été ajoutées par `tauri android init`) | — |
| Holochain / hc | 0.7.0 / 0.7.0 | binaires officiels GitHub (release `holochain-0.7.0`), sha256 vérifiés, dans `~/.local/bin` | — |
| JDK | OpenJDK **21.0.12** (headless) | `apt` | Plan : JDK 17+. Debian trixie ne fournit plus OpenJDK 17 ; la 21 est compatible avec Gradle 9 et AGP. |
| Android cmdline-tools | 19.0 (`commandlinetools-linux-13114758`, sha256 `7ec96528…17ee`) | `~/android/sdk/cmdline-tools/latest` | — |
| Android SDK | platforms `android-35` (+ `android-36` et `android-37.0`, téléchargées par Gradle), build-tools 35.0.0 (+ 36.0.0), platform-tools (adb 37.0.1) | `sdkmanager` | — |
| Android NDK | 27.2.12479018 | `sdkmanager "ndk;27.2.12479018"` | — |
| Gradle | 9.6.1 (wrapper du projet généré par Tauri) | automatique | — |
| Node.js / npm | 20.19.2 / 11.21.1 | préinstallé | — |
| Tauri CLI | **2.12.4** (`@tauri-apps/cli`, dépendance npm locale de `spike-android/tauri-min`) | `npm install` | Le plan signalait une CLI 1.6.6 sur le poste Windows : il faudra la remplacer par la 2.x. |
| CMake / clang / g++ | 3.31.6 / 19.1.7 / 14.2.0 | `apt` | — |
| protoc | 3.21.12 | `apt` | — |
| Python (venv) | 3.13.5 ; torch 2.14.1+cpu, transformers 5.21.0, gguf, sentencepiece, safetensors | `/workspace/venv` | Nécessaire uniquement pour convertir un modèle HF en GGUF (S4). |
| llama.cpp patché | commit `42fc2430…` + `patches/llama-cpp-pipeline-split` | `build_native.sh` | — |
| **Nix** | **non installé** | — | Le chemin Android officiel de Holochain (`holochain/android-service-runtime`, p2p Shipyard) repose sur Nix. Voir S1 dans `spike-android/README.md`. |

Variables d'environnement utilisées :

```bash
source ~/.cargo/env
export ANDROID_HOME=$HOME/android/sdk
export NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
```

## 3. Reste à faire sur le matériel de Geoffroy

| Besoin | Pour | Statut |
|---|---|---|
| Téléphone Android réel (USB debug ou adb Wi-Fi) | S1 (cold start, RAM, service 30 min), S2 (Vulkan, tok/s, thermique) | à fournir |
| PC Windows + GPU NVIDIA : driver, CUDA 12.x, Vulkan SDK, VS 2022 Build Tools | S3 (G2/G3) | à installer |
| Second nœud sur le LAN (et Tailscale) | S4 (RTT, débit, TTFT, tok/s sur 2 nœuds) | à fournir |
| ~~Remplacer Holochain 0.4.1 par 0.7.0 dans le PATH Windows~~ — **fait le 29/10/2026, voir §4** ; reste la CLI Tauri 1.6.6 → 2.x | S1/S3 | Holochain : fait ; Tauri : à faire |
| Certificat de signature de test Windows et keystore Android de release | S3/S5 | keystore **de test** généré à la volée par `spike-android/verify_apk_chain.sh` ; aucun secret n'est versionné |

## 4. Poste Windows (état au 29/10/2026)

Le remplacement prévu au §3 a été effectué. Il a révélé un point que le plan ne
décrivait pas comme tel : **`hc` 0.4.1 n'a aucune sous-commande `client`**
(`hc client call --help` se termine par `panicked ... Failed to run external
subcommand ... "program not found"`). Le flux conducteur du dépôt (`hc client
call`, assertions de la CI) était donc impossible sur cette version — ce n'était
pas seulement un écart de numéro de version.

| Élément | État |
|---|---|
| Holochain / `hc` **0.7.0** | assets officiels `x86_64-pc-windows-msvc`, SHA-256 vérifiés, installés dans `C:\Users\geoff\holochain-0.7.0\bin` ; `.cargo\bin` pointe désormais sur 0.7.0 (`hc --version` → `holochain_cli 0.7.0`) |
| Holochain 0.4.1 (isolé) | conservé intact dans `C:\Users\geoff\holochain-0.4.1\` (`activate.ps1` permet de le remettre dans le `PATH` d'une session) |
| `lair-keystore` | 0.7.1 (déjà à jour) |
| Script reproductible | [`scripts/setup_holochain_windows.ps1`](../scripts/setup_holochain_windows.ps1) : hash des assets Windows épinglés, mêmes contrôles de version et de surface CLI que la CI |
| Surface CLI exigée par la CI | `hc client call --help` expose `new-agent`, `list-app-ws`, `install-app`, `add-app-ws` ✔ |
| Cible Rust `wasm32-unknown-unknown` | déjà installée |
| `hc app pack` + `dna-hashes --check` | exécutés le 29/10/2026 : les deux hApps se packent, mais les hashes DNA **ne correspondent pas** aux valeurs épinglées (voir §4.1) |
| `bash` | résout vers le lanceur WSL (`WindowsApps\bash.exe`) : les scripts `.sh` du dépôt s'exécutent dans WSL, pas en natif |
| Docker | **absent** — la pile Compose et les tests à deux conducteurs restent hors de portée sur ce poste |
| GPU | Intel Iris Xe (intégré, ~2 Go partagés), **aucun CUDA** — S3 (porte G2) reste bloqué par le matériel |

Non vérifié sur ce poste : démarrage réel d'un conducteur, `hc app pack` sur les
manifestes du dépôt (exigent les WASM compilés), et toute mesure `tok/s`.

### 4.1 Reproductibilité des hashes DNA : mesurée, non acquise

Premier test de bout en bout de la chaîne 0.7.0 sur ce poste — zomes wasm →
`hc dna pack` → `hc app pack` → `dna-hashes --check` — le tout avec `hc` 0.7.0.
Tout passe **sauf** la dernière étape :

| Environnement | Hash `inference-mesh` |
|---|---|
| Référence (CI ubuntu x86_64, Rust 1.91) — épinglé | `uhC0kYFyQtLQaaKs--WrpAQdNh44E0SXDX8Kk409Tan7Us4QLU5Q_` |
| Windows, Rust 1.91, `hc` 0.7.0 | `uhC0kzs5P3jR0D2MKCjueITX4GDLgUd7v6yCmqUb_PVnEc09nrEKF` |
| Windows, Rust 1.95, `hc` 0.7.0 | `uhC0kPZgq4_efY2J6_ZwSZU3AOuuvso7jhowLelod0ClwX7OcKpPK` |

Le hash DNA dépend des octets WASM, donc de la chaîne de build (hôte et version de
`rustc`) : les valeurs épinglées ne sont reproductibles que dans l'environnement de
référence. Conséquences pratiques : un opérateur rejoint le réseau avec les bundles
produits par la CI/les releases, jamais avec un rebuild local ; `hc app pack` ne
vérifie pas `installed_hash`, seul `dna-hashes --check` fait foi. Détail et tableau
complet : [DNA hashes](DNA_HASHES.md#reproducibility-boundary-measured-2026-10-29).

> Note build : sous Windows, compiler les workspaces zomes échoue en `LNK1104`
> (chemin d'objet trop long) si l'on garde le `target/` du dépôt ; utiliser un
> `CARGO_TARGET_DIR` court (ex. `C:\Users\geoff\wz-ain`).

> Note : un binaire `hc07.exe` non conforme aux assets officiels préexistait dans
> `C:\Users\geoff\.cargo\bin`. Il n'a pas été utilisé ni supprimé ; à archiver
> ou supprimer sur décision du propriétaire.

### 4.2 Reproductibilité : résolue par l'image

Le build est déterministe sur une machine donnée (deux compilations successives
donnent des octets identiques), mais le WASM embarque les chemins absolus du
workspace **et du registre cargo** (ex. `/home/runner/.cargo/registry/src/...`
apparaît dans les messages de panique). Reproduire l'epoch revient donc à
reproduire l'environnement du runner, pas à normaliser les chemins. Mesuré le
29/10/2026 sur `inference-mesh` :

| Environnement | Hash |
|---|---|
| Runner de référence (`HOME=/home/runner`, `CARGO_HOME=/home/runner/.cargo`, workspace `/home/runner/work/AInonymous/AInonymous`, Rust 1.91.1) | `uhC0kYFy…` (épinglé) |
| Conteneur Debian, chemin du workspace de référence, mais `CARGO_HOME=/usr/local/cargo` | `uhC0kidn…` |
| Conteneur Debian calqué sur le runner | `uhC0kYFy…` ✔ |
| Windows, Rust 1.91.1, remap du workspace | `uhC0kzs5…` |

`deploy/containers/repro/Dockerfile` matérialise cet environnement (toolchain
1.91, `hc` 0.7.0 vérifié par SHA-256, arborescence du runner) et échoue si les
bundles ne correspondent pas au registre. Le workflow `Reproducible bundles`
l'exécute à chaque PR touchant `dnas/**` et une fois par semaine : la
reproductibilité est donc une propriété vérifiée, plus une intention.
