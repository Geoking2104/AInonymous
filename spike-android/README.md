# Spike S1 — Tauri 2 Android et conducteur Holochain 0.7 embarqué (porte G1, #27)

Date : 27/10/2026. Toutes les étapes ont été faites sur la box Linux (sans
téléphone et sans KVM, donc sans émulateur). Environnement : voir
[`docs/phase-0-env.md`](../docs/phase-0-env.md).

## Résultat en bref

| Étape | Statut | Mesure réelle |
|---|---|---|
| APK Tauri 2 minimal (`tauri-min`, arm64-v8a, release, non signé) | ✅ construit | **5 662 875 o (5,4 Mio)**, build à froid 2 min 37 s |
| Conducteur Holochain 0.7.0 compilé pour `aarch64-linux-android`, **NDK seul, sans Nix** | ✅ après 2 contournements (ci-dessous) | `.so` de la sonde : 101 Mo (non strippé) |
| Conducteur démarré **sur l'hôte Linux x86_64** (même code, lair in-process, base persistante) | ✅ | 1 968 ms au premier démarrage ; 1 567 et 1 639 ms au redémarrage |
| APK Tauri 2 **avec conducteur embarqué** (`--features holochain`) | ✅ construit | **39 049 571 o (37,2 Mio)** non signé ; 39 110 919 o signé (test). `.so` de 37,4 Mo (LTO, `opt-level=s`, strip). G1 exige < 200 Mo |
| Signature Android v2/v3 et refus d'un APK altéré (apksigner + manifeste S5) | ✅ | voir `results/verify_apk_chain*.log` |
| Démarrage du conducteur **sur Android** et cold start < 10 s | ⏳ **téléphone requis** | — |
| Membrane proof privée, zome call, signal, service 30 min en arrière-plan | ⏳ téléphone requis, et hApp de test à intégrer | — |

## Blocages rencontrés et contournements (reproductibles)

1. **Aucun binaire Holochain 0.7.0 officiel pour Android** : la release GitHub
   `holochain-0.7.0` ne fournit que Linux, macOS et Windows. Le chemin officiel
   (`holochain/android-service-runtime`, p2p Shipyard) repose sur **Nix**. Le
   passage à 0.7 n'y existe que dans une PR **brouillon** (#140, en
   `0.7.0-dev.28`). Nous avons donc tenté le NDK seul.
2. **OpenSSL vendu** (tiré par `libsqlite3-sys`/sqlcipher) appelle
   `aarch64-linux-android-ranlib`, que le NDK r27 ne fournit plus (seul
   `llvm-ranlib` existe). Contournement : un alias dans le `PATH` et
   `RANLIB_aarch64_linux_android` (voir `holochain-probe/build-android.sh`).
3. **Le crate `holochain = "=0.7.0"` ne compile pas avec une résolution libre des
   dépendances**, quelle que soit la cible : 11 erreurs, car les crates soeurs en
   0.7.1 (`holochain_conductor_api`, `holochain_types`, `holochain_zome_types`…)
   cassent l'API sans changer de version mineure. Contournement : partir du
   `Cargo.lock` **publié avec** `holochain-0.7.0`. C'est le `Cargo.lock` versionné
   de `holochain-probe/` et de `tauri-min/src-tauri/`. Il faudra garder ce lock
   en phase 1, ou passer à `holochain 0.7.1`.

Les journaux des trois tentatives sont dans
`results/holochain_android_build_attempts.txt`.

## Contenu

| Chemin | Rôle |
|---|---|
| `tauri-min/` | shell Tauri 2 (`tauri.conf.json`, frontend statique `dist/`, commandes `startup_info` et `holochain_start`). Le dossier `gen/android` est régénéré par `npx tauri android init --ci` et n'est pas versionné |
| `holochain-probe/` | bibliothèque `start_conductor(data_dir, passphrase)` (lair in-process) + symbole C `relay_holochain_probe_start`, exemple hôte `examples/start.rs`, script de compilation croisée |
| `verify_apk_chain.sh` | signature apksigner de test, manifeste `relay-release`, refus d'un APK altéré d'un octet (deux mécanismes) et refus du rejeu |
| `results/` | journaux et mesures réels |

## Reproduire

```bash
source ~/.cargo/env
export ANDROID_HOME=$HOME/android/sdk NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018 JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
# alias ranlib (voir blocage 2), nécessaire seulement avec --features holochain
TC=$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin; S=$(mktemp -d); ln -s $TC/llvm-ranlib $S/aarch64-linux-android-ranlib
export PATH=$S:$PATH RANLIB_aarch64_linux_android=$TC/llvm-ranlib
cd spike-android/tauri-min && npm install && npx tauri android init --ci
npx tauri android build --apk --target aarch64 --ci                        # APK minimal
npx tauri android build --apk --target aarch64 --ci --features holochain   # avec conducteur
# démarrage réel du conducteur sur l'hôte :
cd ../holochain-probe && cargo run --release --example start -- /tmp/relay-s1-conductor
```

## À faire sur le téléphone de Geoffroy

Un APK signé avec une clé de test (conducteur embarqué) se trouve sur la box :
`/workspace/artifacts/relay-s1-holochain-testsigned.apk` (sha256
`0b593415…b4d1`). Il n'est pas versionné.

1. `adb install relay-s1-holochain-testsigned.apk`, puis
   `adb logcat | grep "S1 "` (lignes `S1 …`).
2. **Cold start** : `adb shell am force-stop org.ainonymous.relay.spike && adb shell am start -W -n org.ainonymous.relay.spike/.MainActivity`
   (`TotalTime`) ×5, puis relever `conductor_start_ms` affiché par l'UI. G1 exige < 10 s.
3. RAM : `adb shell dumpsys meminfo org.ainonymous.relay.spike` après le démarrage du conducteur.
4. Reste du livrable S1 : hApp de test avec membrane proof privée, zome call,
   signal, service de premier plan et survie 30 min en arrière-plan (avec et
   sans optimisation de batterie).
