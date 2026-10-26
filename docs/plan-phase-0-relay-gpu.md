# Plan d'exécution — Phase 0 « Spikes »

## AInonymous Relay — validation des hypothèses avant engagement

| Élément | Valeur |
|---|---|
| Version | 0.1 |
| Date | 21 octobre 2026 |
| Décisions amont | **D6** mesh privé · **D7** Android agent complet via Tauri · **D8** distribution directe signée |
| Durée | 4 semaines de spikes + 1 semaine de préparation |
| Document parent | [`cahier-des-charges-relay-gpu.md`](cahier-des-charges-relay-gpu.md) |
| Règle | Phase 0 **timeboxée** : on ne code pas le produit, on prouve ou on invalide les hypothèses |

---

## 1. Objectif

La Phase 0 doit répondre à quatre questions **avant** d'engager 150 homme-semaines :

1. **Android** : un agent Holochain complet peut-il tourner dans une application Tauri 2, avec un conductor 0.7, une membrane proof privée et un service de premier plan stable ?
2. **GPU mobile** : llama.cpp Vulkan peut-il exécuter un modèle 1B Q4 sur un téléphone réel avec un débit et une thermique acceptables ?
3. **Windows** : les backends CUDA et Vulkan fonctionnent-ils sur au moins deux GPU, et un installeur per-user signé est-il réaliste ?
4. **Réseau et pipeline** : deux nœuds peuvent-ils échanger des activations en QUIC sur LAN/VPN, et le patch layer-range produit-il un résultat correct ?

### 1.1 Ce que la Phase 0 ne fait pas

- Pas de produit fini, pas d'UI complète, pas de zome `relay-market` complet.
- Pas de scheduler, pas de quotas, pas de comptabilité.
- Pas de sécurité production, pas d'audit.
- Pas de promesse de date de release.

### 1.2 Critère de sortie global

Un **rapport de Phase 0** avec :

- les mesures reproductibles ;
- une décision **go / no-go / go conditionnel** par spike ;
- une mise à jour des estimations ;
- une décision explicite sur D7 (Android Tauri) et sur le pipeline ;
- les prérequis et le backlog révisé pour la Phase 1.

---

## 2. Audit de l'environnement local

Relevé effectué le 21 octobre 2026 sur la machine de travail Windows. Ces éléments conditionnent le démarrage : **la Phase 0 Android ne peut pas commencer sans installation préalable**.

| Élément | État constaté | Action |
|---|---|---|
| OS | Windows 11, build 26300 | OK |
| Rust | 1.95.0 | OK (≥ 1.91) |
| Cibles Rust installées | `wasm32-unknown-unknown`, `x86_64-pc-windows-msvc` | Ajouter `aarch64-linux-android` |
| MSVC / Build Tools | Visual Studio Build Tools 18 présent | OK, vérifier `cl` en shell développeur |
| CMake / Ninja | 3.29.2 / 1.12.0 | OK |
| Node / pnpm | 24.14.0 / 11.1.2 | OK |
| Python | 3.11.15 | OK |
| Git | 2.53.0 | OK |
| **Holochain** | **0.4.1** | **Installer 0.7.0** (le dépôt exige 0.7.0) |
| **hc** | **0.4.1** | **Installer 0.7.0** |
| Lair | 0.7.1 | OK |
| **Tauri CLI** | **1.6.6** | **Installer Tauri 2.x** |
| **JDK** | Absent | Installer JDK 17+ |
| **Android SDK / NDK / ADB** | Absents (`ANDROID_HOME` vide) | Installer Android Studio/SDK, NDK, platform-tools |
| **Nix** | Absent | Installer via WSL2 ou utiliser un host Linux/CI |
| WSL2 | Non vérifiable depuis le sandbox | Vérifier manuellement |
| **GPU / CUDA** | `nvidia-smi` absent du PATH ; GPU non inspectable | Identifier la machine de test GPU ; installer driver + CUDA/Vulkan SDK |
| Signature Windows | Certificat non vérifié | Prévoir un certificat EV ou un certificat de test |
| Signature Android | Keystore non vérifié | Créer un keystore de test, puis un keystore de release |

### 2.1 Conséquence

- **Chemin critique Android** : il faut 1 à 3 jours d'installation (JDK, SDK, NDK, Tauri 2, Nix/WSL2) avant le premier spike.
- **Holochain 0.4.1** doit être remplacé ou isolé : un `holochain` 0.4.1 dans le PATH fera échouer les tests 0.7.
- **Tauri CLI 1.6.6** ne permet pas de builder Tauri 2 : installer la CLI 2.x.
- **Pas de GPU NVIDIA visible** : le spike Windows CUDA nécessite une machine de test dédiée ; sinon, le spike se limite à Vulkan/CPU et le risque CUDA reste ouvert.

---

## 3. Workstreams

| ID | Workstream | Question | Durée | Dépendances | Sortie |
|---|---|---|---|---|---|
| **S1** | Android Tauri + Holochain | Un agent complet tourne-t-il dans Tauri 2 ? | 3 semaines | JDK, SDK, NDK, Tauri 2, Nix/WSL | APK de spike, mesures |
| **S2** | Android GPU | Vulkan/llama.cpp est-il utilisable sur téléphone ? | 2 semaines | S1 (app), NDK, appareil réel | Benchmark 1B/3B |
| **S3** | Windows GPU + packaging | CUDA/Vulkan et installeur per-user ? | 2 semaines | Machine GPU, Tauri 2, MSVC | Binaires, installeur |
| **S4** | QUIC / pipeline / réseau | Deux nœuds échangent-ils en QUIC ? Le layer-range est-il correct ? | 2 semaines | S3 ou deux PCs | Rapport réseau |
| **S5** | Signature / distribution | Un manifeste signé et un installeur/APK vérifiable ? | 1 semaine | S3 | Prototype de release |

**S1 est la porte dure** : si S1 échoue, D7 est rouverte et le planning est révisé. Les autres spikes peuvent être partiels sans bloquer la Phase 1, mais leurs résultats conditionnent le périmètre.

---

## 4. Planning sur 5 semaines

### Semaine 0 — Préparation (indispensable)

| Jour | Tâche | Critère |
|---|---|---|
| J1 | Installer JDK 17, Android Studio, SDK 35, NDK 27/28, platform-tools ; configurer `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `PATH` | `adb devices` répond |
| J1 | Installer Tauri 2 CLI (`cargo install tauri-cli --version "^2" --locked` ou `pnpm add -D @tauri-apps/cli@^2`) ; désinstaller/renommer la 1.6.6 pour éviter les collisions | `cargo tauri --version` ≥ 2 |
| J1 | Installer Holochain 0.7.0 et `hc` 0.7.0 ; isoler 0.4.1 | `holochain --version` = 0.7.0 |
| J2 | Ajouter la cible `aarch64-linux-android` | `rustup target list --installed` |
| J2 | Vérifier WSL2 + Nix ; sinon provisionner un host Linux/CI | `nix --version` |
| J2 | Identifier la machine GPU de test (NVIDIA + AMD/Intel) | driver + outil de détection |
| J3 | Créer les keystores de test Windows/Android | signature de test vérifiée |
| J3 | Créer un compte de test et une invitation de mesh privé | preuve de membrane générée |
| J4 | Geler la matrice de matériel de test | 2 Windows + 1 Android minimum |
| J5 | Revue de lancement : risques, accès, planning | go interne |

**Livrable** : `phase-0-env.md` avec versions, chemins, matériel, accès.

### Semaine 1 — S1 démarrage + S3

| Workstream | Tâches | Sortie |
|---|---|---|
| S1 | Scaffold Tauri 2 Android ; build APK vide ; lancer sur appareil | APK installé, UI minimale |
| S1 | Intégrer le plugin Holochain / p2p Shipyard ; démarrer le conductor | conductor 0.7 démarre |
| S1 | Installer le hApp de test ; genesis avec membrane proof privée | agent admis |
| S3 | Détection GPU Windows (DXGI/NVML) ; lancer `llama-server` CUDA et Vulkan | 7B Q4 répond |
| S3 | Mesurer le débit de référence llama.cpp | baseline |

### Semaine 2 — S1 fin + S2

| Workstream | Tâches | Sortie |
|---|---|---|
| S1 | Zome call depuis l'UI ; signal ; persistance de l'identité | preuve de bout en bout |
| S1 | Service de premier plan : démarrage, arrêt, redémarrage, survie 30 min en arrière-plan ; tester avec et sans optimisation batterie | test de lifecycle |
| S1 | Mesurer cold start, RAM, taille APK, consommation conductor | chiffres |
| S2 | Compiler llama.cpp Vulkan pour `arm64-v8a` ; JNI minimal | `.so` chargé |
| S2 | Exécuter un 1B Q4 sur appareil réel | tokens générés |
| S2 | Mesurer tok/s, thermique 10 min, batterie | chiffres |

### Semaine 3 — S2 fin + S3 fin + S4

| Workstream | Tâches | Sortie |
|---|---|---|
| S2 | Test 3B Q4 ; fallback CPU ; limites mémoire | matrice de faisabilité |
| S3 | Installeur per-user Tauri 2 ; signature de test ; installation/désinstallation | installeur vérifié |
| S4 | Deux nœuds Windows en QUIC sur LAN ; échange d'activations | transfert réussi |
| S4 | Test Tailscale/VPN ; mesure RTT/débit/perte | chiffres |
| S4 | Patch layer-range : petit modèle, 2 étages, sortie comparée | équivalence ou écart documenté |

### Semaine 4 — S4 fin + S5 + restitution

| Workstream | Tâches | Sortie |
|---|---|---|
| S4 | Test pipeline 2 nœuds, mesure TTFT/tok/s ; test d'échec d'un étage | décision pipeline |
| S5 | Manifeste signé, vérification d'installateur/APK, rollback | prototype de release |
| Tous | Consolidation des mesures, nettoyage, rapport | `phase-0-report.md` |
| Tous | Revue go/no-go ; mise à jour des estimations | décision |

---

## 5. Critères go/no-go

### 5.1 Portes dures

| Porte | Critère de succès | Échec |
|---|---|---|
| **G1 — Android agent complet** | APK installé ; conductor 0.7 démarre ; membrane proof privée acceptée ; zome call réussi ; service survit 30 min en arrière-plan ; cold start < 10 s ; APK < 200 Mo | **Stop** : D7 rouverte, pas de bascule silencieuse |
| **G2 — Windows GPU** | CUDA et Vulkan exécutent un 7B Q4 ; détection GPU correcte ; débit ≥ 80 % de la baseline llama.cpp | **Conditionnel** : un seul backend → périmètre réduit |
| **G3 — Windows packaging** | Installeur per-user signé ; installation/désinstallation < 5 min ; aucune élévation pour l'usage local | **Conditionnel** : packaging manuel en attendant |
| **G4 — QUIC deux nœuds** | Session mTLS + token ; transfert d'activations ; RTT et débit mesurés ; échec propre | **Stop Phase 1 mesh** : revoir le transport |
| **G5 — Distribution** | Manifeste signé vérifié ; installateur/APK refusé si hash/signature invalide ; rollback | **Conditionnel** : release manuelle |

### 5.2 Portes souples

| Porte | Critère | Si échec |
|---|---|---|
| **G6 — Android GPU** | 1B Q4 ≥ 5 tok/s sur appareil de référence ; 10 min sans arrêt thermique | Android CPU-only ; valeur réduite, D7 reste valide |
| **G7 — Pipeline layer-range** | Sortie équivalente au modèle entier sur petit modèle déterministe ; 2 étages stables | **Réplique uniquement en v1** ; pipeline en phase ultérieure |
| **G8 — NAT** | LAN + Tailscale OK ; WAN direct non requis | Documenter LAN/VPN ; relais en phase 2 |

### 5.3 Règle de décision

- **G1 + G2 + G3 + G4 + G5 verts** → Phase 1 complète.
- **G1 rouge** → arrêt, revue de D7, réestimation.
- **G4 rouge** → arrêt du volet mesh, retour au Local-only, revue d'architecture.
- **G6/G7/G8 rouges** → Phase 1 réduite (Windows local + réplique LAN), pas d'arrêt.

---

## 6. Mesures à collecter

### 6.1 Android

| Mesure | Méthode | Cible |
|---|---|---|
| Cold start | `adb shell am start -W` | < 10 s |
| Taille APK | fichier | < 200 Mo |
| RAM au repos / en job | `dumpsys meminfo` | < 500 Mo / < 2 Go |
| Démarrage conductor | logs | < 30 s |
| Latence zome call | chrono | < 2 s |
| Tokens/s 1B Q4 | benchmark | ≥ 5 tok/s |
| Température 10 min | `PowerManager` / logs | pas d'arrêt |
| Batterie | `dumpsys batterystats` | < 10 %/h en contribution |

### 6.2 Windows

| Mesure | Méthode | Cible |
|---|---|---|
| Détection GPU | probe | correcte (NVIDIA/AMD/Intel) |
| Tokens/s 7B Q4 | benchmark | ≥ 80 % baseline |
| VRAM utilisée | `llama-server` / NVML | < 90 % |
| Installation | chrono | < 5 min |
| Signature | `signtool verify` | OK |

### 6.3 Réseau / pipeline

| Mesure | Méthode | Cible |
|---|---|---|
| RTT | ping/QUIC | LAN < 2 ms ; VPN < 50 ms |
| Débit | transfert 100 Mo | ≥ 500 Mb/s LAN |
| Setup session | chrono | < 1 s LAN |
| TTFT pipeline | chrono | < 2 s (2k prompt) |
| Tokens/s pipeline | benchmark | ≥ 15 tok/s (30B Q4, 2 nœuds LAN) |
| Équivalence | comparaison de sortie | identique ou écart documenté |

---

## 7. Prérequis techniques détaillés

### 7.1 Android

- JDK 17+ (Microsoft OpenJDK ou Temurin).
- Android Studio, SDK Platform 35, Build-Tools, Platform-Tools, NDK 27/28.
- `ANDROID_HOME` et `ANDROID_SDK_ROOT` définis ; `adb` dans le PATH.
- `rustup target add aarch64-linux-android`.
- Tauri 2 CLI et dépendances Android (`tauri android init`).
- Nix + p2p Shipyard sur WSL2/Linux, ou CI Linux avec cache Nix.
- Appareil réel arm64 (pas seulement l'émulateur), Android 10+.

### 7.2 Windows

- Visual Studio Build Tools avec charge de travail C++.
- CMake/Ninja (présents).
- CUDA Toolkit + driver NVIDIA sur la machine de test.
- Vulkan SDK + driver AMD/Intel.
- Certificat de signature (test puis EV).
- Machine avec au moins 16 Go RAM et 50 Go libres.

### 7.3 Holochain

- `holochain` et `hc` **0.7.0** (pas 0.4.1).
- Lair 0.7.1 (présent).
- hApp de test construit avec `hc` 0.7.0.
- Bootstrap/signal privés ou locaux pour le mesh de test.
- Une preuve de membrane générée pour l'agent de test.

### 7.4 Comptes et accès

- Un administrateur de mesh de test.
- Un appareil Android réel.
- Deux PC Windows (idéalement un NVIDIA, un AMD/Intel).
- Un réseau LAN contrôlé + un compte Tailscale/WireGuard.

---

## 8. Risques de la Phase 0 et contingences

| Risque | Signal | Contingence |
|---|---|---|
| Tauri 2 Android instable | build ou lifecycle échoue | réouverture D7 ; option Kotlin/JNI ; pas de bascule silencieuse |
| p2p Shipyard/Nix impossible sur Windows | pas de build Android | host Linux/CI ; si indisponible, blocage |
| Holochain 0.7 ne compile pas pour Android | erreur NDK/link | conductor FFI ou build custom ; sinon réouverture D7 |
| Pas de GPU NVIDIA de test | `nvidia-smi` absent | machine dédiée ou spike CUDA reporté ; Vulkan/CPU en attendant |
| Vulkan mobile décevant | < 3 tok/s ou arrêt thermique | Android CPU-only, périmètre réduit |
| Patch layer-range incorrect | sortie divergente | pipeline abandonné en v1 ; réplique uniquement |
| Manque de temps | semaine 4 | réduire le scope aux portes G1/G4/G5, reporter G7 |
| Signature non disponible | pas de certificat | signature de test + release interne ; EV avant public |

---

## 9. Livrables de la Phase 0

| Livrable | Contenu | Responsable |
|---|---|---|
| `phase-0-env.md` | Versions, chemins, matériel, accès | DevOps |
| `spike-android/` | Projet Tauri 2 + conductor + APK de test | Android/Rust |
| `spike-android-gpu/` | JNI llama.cpp Vulkan + benchmark | Android/C++ |
| `spike-windows/` | Détection GPU, backends, installeur | Windows/C++ |
| `spike-quic/` | Deux nœuds, transfert, mesures | Rust |
| `spike-pipeline/` | Patch layer-range, test 2 étages | Rust/C++ |
| `phase-0-report.md` | Mesures, décisions, estimations | Tous |
| `decision-memo.md` | D7, pipeline, périmètre Phase 1 | Produit/Tech |

---

## 10. Critères d'entrée en Phase 1

La Phase 1 ne démarre que si :

- [ ] G1 (Android agent complet) est **vert**.
- [ ] G2 et G3 (Windows GPU/packaging) sont **verts ou conditionnels** avec un plan.
- [ ] G4 (QUIC deux nœuds) est **vert**.
- [ ] G5 (distribution) est **vert**.
- [ ] Le rapport de Phase 0 est relu et accepté.
- [ ] Les estimations de la Phase 1 sont révisées avec les mesures réelles.
- [ ] Les décisions D6/D7/D8 sont confirmées ou explicitement révisées.
- [ ] Les accès (GPU, Android, signatures, réseau) sont disponibles pour toute la Phase 1.

---

## 11. Prochaine action

**Semaine 0, J1** : installer la chaîne Android et Tauri 2, remplacer Holochain 0.4.1 par 0.7.0, et vérifier WSL2/Nix. Sans cela, le spike S1 ne peut pas démarrer.

Une fois ces prérequis en place, le premier test à lancer est **S1/G1** : une application Tauri 2 Android vide, un conductor Holochain 0.7, une membrane proof privée et un zome call. C'est la preuve minimale qui justifie (ou invalide) la décision D7.
