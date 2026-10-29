# Chantiers v1 — de la démonstration au produit utilisable

*Document de définition. Version 1, 29 octobre 2026.*

Ce document transforme six demandes produit en chantiers définis : ce que chacun
apporte, ce qui existe déjà, ce qu'il faut construire, **comment on saura que c'est
fait**, ce dont il dépend, et ce qu'il ne promettra pas.

Il ne remplace pas les documents existants, il les articule :

| Document | Rôle |
| --- | --- |
| [`cahier-des-charges-relay-gpu.md`](cahier-des-charges-relay-gpu.md) | Exigences du module Relay (matrice d'acceptation T-01…T-10, §7.5 marché/revocations) |
| [`plan-phase-0-relay-gpu.md`](plan-phase-0-relay-gpu.md) | Portes G1–G8 et critères chiffrés des spikes |
| [`ROADMAP.md`](../ROADMAP.md) | Portes de sécurité et produit déjà suivies |
| [`SECURITY.md`](SECURITY.md) | Ce qui est garanti aujourd'hui, et les manques assumés |

**Règle appliquée partout :** un critère d'acceptation doit être **mesurable et
reproductible**. Un chantier n'est « fait » que si la mesure existe dans le dépôt
(test automatisé, rapport de mesure versionné, ou les deux).

---

## Vue d'ensemble

| # | Chantier | Livrable visible | Effort (1 personne) | Dépend de |
| --- | --- | --- | --- | --- |
| CH1 | Installeur Windows | `.exe` per-user signé, première inférence en < 5 min | **L** (6–10 sem.) | G2/G3, certificat de signature |
| CH2 | Android (SDK + APK) | Bibliothèque Android + application de référence, admise au mesh | **L** (8–12 sem.) | **G1** (porte dure), téléphone réel |
| CH3 | Calcul réparti réel | Rapport de mesure 2+ machines sur LAN, protocole reproductible | **M** (2–3 sem.) | 2ᵉ machine, décision réplique/pipeline |
| CH4 | Mesures GPU & téléphone | Chiffres S3 (Windows/CUDA+Vulkan) et S2 (Vulkan mobile) publiés | **M** (1–2 sem.) | PC GPU NVIDIA, téléphone |
| CH5 | Stealth mode | Profil de transport « sans adresse directe », modèle de menace, tests | **L** (4–8 sem.) | relais privé (Q11) |
| CH6 | Révocation temps réel | `Revocation` + deny list, arrêt d'un job < 5 s | **L** (4–6 sem.) | CH3 (banc 2 nœuds) |
| CH7 | Audit externe & pilote | Kit de pilote « non audité » + dossier d'audit prêt | **M** (2–3 sem.) + audit externe | budget audit |

Effort : **S** = 1–3 jours, **M** = 1–3 semaines, **L** = plus de 3 semaines,
pour une personne. Cumul réaliste en solo : **6 à 12 mois** — cohérent avec la revue
de Phase 0.

---

## CH1 — Version installable pour le grand public : Windows

### Objectif

Un utilisateur non technique télécharge un `.exe`, l'installe **sans droits
administrateur**, et obtient sa première réponse d'IA locale en moins de cinq
minutes. C'est le critère T-02 du cahier des charges, et la moitié « made simple »
de la mission.

### Ce qui existe déjà

- `spike-windows/` : détection GPU, compilation llama.cpp, protocole de mesure,
  vérification d'installeur (scripts non exécutés à ce jour) ;
- `crates/ainonymous-daemon`, `ainonymous-proxy` (API compatible OpenAI),
  `ainonymous-cli` ;
- les hApps packées et **reproductibles** (`deploy/containers/repro/Dockerfile`) ;
- un installeur Tauri 2 fonctionnel côté Android (`spike-android/tauri-min`), donc
  la coque UI est un problème résolu, pas un saut dans l'inconnu ;
- `scripts/setup_holochain_windows.ps1` pour l'outillage de développement.

### À construire

1. **Coque Tauri 2 Windows** : icône de zone de notification, démarrage
   automatique optionnel, écran d'état (nœud, modèles, pairs), journal consultable.
2. **Empaquetage du conducteur Holochain** : lancement du conducteur 0.7.0 avec
   Lair en processus, données sous `%LOCALAPPDATA%`, aucune écriture dans
   `Program Files` (T-09).
3. **Assistant de premier lancement** : choix d'un modèle adapté au matériel
   détecté, téléchargement avec vérification SHA-256, amorçage de l'inférence
   locale hors ligne.
4. **Installation d'un mesh privé par invitation** : import d'une invitation,
   génération de la preuve (`private-network-proof`), installation du hApp,
   admission (T-03).
5. **Installeur signé** : MSI ou NSIS per-user, signature Authenticode, mise à
   jour et retour arrière documentés.
6. **Désinstallation propre** : retrait du binaire, conservation ou purge explicite
   du keystore et des modèles (choix de l'utilisateur).

### Critères d'acceptation

| Réf. | Critère | Mesure |
| --- | --- | --- |
| T-01 | Installation per-user | Succès sans élévation, sur un compte Windows standard |
| T-02 | Première inférence locale | Réponse complète < 5 min après le lancement de l'installeur |
| T-03 | Rejoindre un mesh privé | Agent admis avec invitation, offre publiée |
| T-09 | Chemins et ACL | Tout sous `%LOCALAPPDATA%`/`%APPDATA%`, aucune écriture protégée |
| — | Signature | Windows affiche un éditeur vérifié ; le manifeste de release est signé |
| — | Désinstallation | < 5 min, aucun fichier résiduel hors choix explicite |
| — | Zéro secret en clair | Recherche automatisée dans les journaux produits |

### Dépendances et risques

- **Portes G2/G3** : sans mesure GPU (CH4), l'assistant ne peut pas recommander un
  modèle de façon fiable.
- **Certificat de signature** : décision et budget du propriétaire (≈ 200–400 €/an).
  Sans certificat, l'installeur fonctionne mais Windows affiche un avertissement.
- Risques : faux positifs antivirus sur un binaire qui lance un processus local ;
  taille des modèles (1–8 Go) ; support des machines sans GPU.

---

## CH2 — Android : SDK et application de référence

### Objectif

Permettre à un développeur Android d'intégrer « rejoindre un mesh privé et exécuter
de l'inférence » dans sa propre application (SDK), et fournir une application de
référence qui le fait, admise par preuve de membrane. C'est la porte **G1**, la
plus dure du plan.

### Ce qui existe déjà

- `spike-android/` : projet Tauri 2, sonde Holochain, **APK aarch64 construit**
  (5,6 Mo minimal ; 39,0 Mo avec Holochain), sous la barre des 200 Mo ;
- `spike-android-gpu/` : scripts de compilation llama.cpp et de mesure (jamais
  exécutés sur appareil) ;
- l'outil de preuve et le harnais d'admission (CH6 et SG1) réutilisables tels quels.

### À construire

1. **Bibliothèque Android** (AAR) exposant trois cas d'usage : `joinMesh(invitation)`,
   `infer(prompt)`, `reportMetrics()` — avec une API stable et documentée.
2. **Application de référence** : écran d'invitation, écran de conversation,
   réglages (charge, Wi-Fi uniquement, plafond batterie).
3. **Cycle de vie Android** : service de premier plan, survie 30 min en
   arrière-plan, redémarrage automatique après reboot, coupure propre.
4. **Backend d'inférence** : llama.cpp Vulkan (G6), repli CPU documenté.
5. **Publication** : APK signé, distribution hors magasin d'abord (sideload),
   procédure de mise à jour ; politique de stores traitée plus tard.

### Critères d'acceptation

| Réf. | Critère | Mesure (plan Phase 0, §5) |
| --- | --- | --- |
| G1 | Agent complet sur téléphone | APK installé, conducteur démarré, **preuve privée acceptée**, appel de zome réussi, cold start < 10 s, démarrage conducteur < 30 s, survie 30 min |
| G6 | Inférence GPU mobile | 1B Q4 ≥ 5 tok/s, 10 min sans arrêt thermique |
| T-10 | Secrets | Clés dans Android Keystore, non exportables quand c'est possible |
| — | Reprise | Après reboot, le nœud revient seul et reste admis |

### Dépendances et risques

- **G1 est une porte dure** : si le conducteur Holochain complet ne tient pas sur
  Android, ce chantier est réévalué (règle du plan : « arrêt, revue, réestimation »),
  pas contourné en silence.
- Un **téléphone réel** est indispensable ; aucun émulateur ne remplace les mesures
  thermiques et de cold start.
- Risques : restrictions de service en arrière-plan par les surcouches
  constructeurs ; batterie ; taille de l'APK avec un modèle embarqué (prévoir un
  téléchargement à la demande).

---

## CH3 — Activer la mesure du calcul réparti sur plusieurs machines réelles

### Objectif

Produire la première mesure honnête de calcul réparti sur **plusieurs machines
physiques d'un réseau réel** : équivalence numérique, latence, débit, et
comportement à la perte d'un nœud.

Aujourd'hui les preuves sont soit locales (deux processus sur une machine), soit en
boucle locale — jamais sur un réseau réel.

### Ce qui existe déjà

- `spike-pipeline/` : équivalence à deux étages (écart maximal nul sur les logits,
  16 jetons identiques) ;
- `spike-quic/` : transport mTLS et mesures en boucle locale, avec ses limites
  écrites noir sur blanc ;
- `scripts/testnet/` : testnet à deux nœuds (mock et runtime) ;
- `scripts/pipeline_server.py`, patch llama.cpp « layer-range ».

### À construire

1. **Banc de mesure reproductible** (`scripts/bench/`) : décrit l'environnement
   (CPU, GPU, RAM, OS, version du binaire), lance N nœuds, mesure TTFT, tok/s,
   latence p50/p95, et **écrit un JSON brut** plus un tableau Markdown.
2. **Deux modes mesurés** : réplication (modèle entier sur chaque nœud) et
   découpage en étages (pipeline), avec la même invite et le même modèle.
3. **Test de perte de nœud** : arrêt d'un worker en cours de génération ; le job
   doit échouer proprement (pas de blocage), avec un temps de détection mesuré.
4. **Test d'équivalence en CI** (déjà amorcé) : deux nœuds, petit modèle
   déterministe, comparaison des sorties ; le rapport réel vient en complément.
5. **Rapport versionné** : `docs/measurements/multinode-<date>.md` avec les
   commandes exactes, les résultats bruts et les limites.

### Critères d'acceptation

| Critère | Mesure |
| --- | --- |
| Deux machines physiques minimum | Rapport nommant le matériel et le réseau (LAN filaire, Wi-Fi documenté) |
| Équivalence | Sorties identiques au nœud unique sur le modèle de test (écart nul) |
| Débit et latence | TTFT et tok/s médians + p95, pour réplication et pipeline |
| Perte de nœud | Job en échec explicite < 10 s, aucune boucle de réessai infinie |
| Reproductibilité | Commandes fournies ; une seconde exécution redonne les mêmes ordres de grandeur |

### Dépendances et risques

- **Deuxième machine physique** et un réseau maîtrisé : c'est le vrai goulot.
- Décision **Q3** (réplique par défaut ou pipeline) : le rapport doit la trancher
  avec des chiffres, pas des préférences.
- Risques : hétérogénéité matérielle qui rend les chiffres non comparables ;
  tentation de publier un chiffre flatteur — le document doit inclure les limites.

---

## CH4 — Téléphone et carte graphique : les mesures doivent exister

### Objectif

Exécuter les protocoles de mesure déjà écrits et publier les chiffres, y compris
quand ils sont décevants.

### Ce qui existe déjà

- `spike-windows/detect_gpu.ps1`, `build_llama_windows.ps1`, `bench.ps1` — écrits,
  **jamais exécutés** ;
- `spike-android-gpu/build_llama_android.sh`, `bench_on_device.sh` — idem ;
- les critères chiffrés (G2 : ≥ 80 % de la baseline llama.cpp ; G6 : ≥ 5 tok/s).

### À construire

1. **Exécution S3 (Windows)** : détection GPU réelle, compilation CUDA et Vulkan,
   mesure 7B Q4, comparaison à la baseline llama.cpp, consignation du pilote et de
   la version CUDA.
2. **Exécution S2 (Android)** : compilation Vulkan, mesure 1B Q4, thermique sur
   10 min, consommation batterie par heure.
3. **Fiche de mesure** : `docs/measurements/gpu-<date>.md` — matériel exact,
   commandes, résultats, et ce que les chiffres ne disent pas.
4. **Mise à jour du tableau de spikes** du README : chaque ligne passe de
   « à mesurer » à un chiffre, ou à un échec documenté.

### Critères d'acceptation

| Critère | Mesure |
| --- | --- |
| S3/G2 | 7B Q4 ≥ 80 % de la baseline llama.cpp sur CUDA ; repli Vulkan mesuré |
| S2/G6 | 1B Q4 ≥ 5 tok/s sur téléphone de référence ; 10 min sans coupure thermique |
| Traçabilité | Version de pilote, de CUDA, de llama.cpp et empreinte du modèle |
| Honnêteté | Un échec est publié comme un échec, avec la cause probable |

### Dépendances et risques

- **Matériel** : un PC Windows avec GPU NVIDIA et un téléphone Android réel. Rien
  ne peut être simulé ici ; c'est un achat ou un prêt, pas un travail logiciel.

---

## CH5 — Stealth mode (au lieu d'une promesse d'anonymat)

### Objectif

Offrir un **profil de transport qui ne révèle pas l'adresse des participants** aux
autres pairs, sans jamais prétendre à l'anonymat.

Le dépôt écrit aujourd'hui : « le projet ne fournit pas d'anonymat ni de
zero-knowledge ». Cette position reste vraie : le mode furtif réduit la
**métadonnée exposée**, il ne rend pas anonyme face à un adversaire global.

### Ce qui existe déjà

- liaison QUIC directe avec épinglage de clé (l'identité est vérifiée, pas cachée) ;
- une clé publique Annuaire (agent-registry) qui publie l'endpoint QUIC — c'est
  précisément ce qui expose l'adresse aujourd'hui ;
- une infrastructure de relais décrite mais non implémentée (Q11).

### À construire

1. **Modèle de menace écrit d'abord** (`docs/STEALTH_THREAT_MODEL.md`) : qui
   observe quoi, ce qui est protégé, ce qui ne l'est pas. Aucun code avant ce
   document.
2. **Transport relayé uniquement** : les pairs ne se connectent jamais
   directement ; le relais ouvre le canal, l'adresse de l'émetteur n'est pas
   transmise. Test : un nœud ne peut pas apprendre l'IP d'un pair (assertion sur
   les journaux et sur les connexions observées).
3. **Minimisation des métadonnées** : trames de taille constante (bourrage),
   regroupement temporel configurable, pas d'identifiant en clair dans le
   handshake applicatif.
4. **Sortie par proxy** (optionnel) : SOCKS5 pour l'amorçage et le relais, afin de
   ne pas exposer l'IP de l'opérateur au relais lui-même.
5. **Coût assumé** : le mode furtif ajoute de la latence ; mesurer ce coût et
   l'afficher dans l'interface.

### Critères d'acceptation

| Critère | Mesure |
| --- | --- |
| Aucune adresse directe | Test automatisé : le pair ne reçoit jamais l'IP de l'émetteur |
| Tailles constantes | Test : toutes les trames d'une session ont la même taille |
| Pas d'identifiant en clair | Analyse du handshake et des journaux |
| Coût mesuré | Latence ajoutée p50/p95 vs mode direct, publiée |
| Limites écrites | Le document de menace liste explicitement ce que le mode ne cache pas |

### Ce que le mode furtif ne promettra jamais

- l'anonymat face à un adversaire qui observe tout le réseau ;
- la résistance à la corrélation temporelle ;
- l'invisibilité de l'existence du nœud (le relais voit passer le trafic) ;
- une protection « zero-knowledge » des calculs.

### Dépendances et risques

- **Relais privé** (décision Q11) : sans infrastructure, le mode ne peut pas être
  testé de bout en bout.
- Risque principal : la tentation marketing. La règle du dépôt (aucune affirmation
  sans test) s'applique ; le nom même de « stealth » doit être accompagné de ses
  limites dans l'interface.

---

## CH6 — Révocation en temps réel

### Objectif

Retirer l'accès d'un membre **en moins de cinq secondes**, sans reconstruire le
réseau ni réinstaller les pairs — critère T-08 du cahier.

### Ce qui existe déjà

- la preuve de membrane (admission) et un administrateur de réseau identifié par
  clé — la racine de confiance nécessaire à une révocation ;
- le §7.5 du cahier qui spécifie l'entrée `Revocation` (agent, portée, raison,
  date, signature) et la fonction `revoke_agent(revocation)` ;
- le harnais SG1 (deux conducteurs) qui servira de banc de test.

### À construire

1. **Registre de révocation** : entrée signée par la clé d'administration,
   vérifiée à l'écriture (auteur = signataire, portée valide, pas d'auto-révocation
   de l'admin).
2. **Deny list locale** : chaque nœud maintient une liste à jour (abonnement aux
   entrées, plus rafraîchissement périodique) et la consulte **avant** d'accepter
   une session.
3. **Application aux sessions en cours** : à la réception d'une révocation, les
   sessions actives du agent concerné sont fermées ; le job en cours est arrêté et
   marqué `failed` avec la raison.
4. **Propagation** : mesurer le délai entre la publication et l'application
   effective sur un pair (objectif < 5 s sur LAN).
5. **Refus de ré-admission** : un agent révoqué qui représente une ancienne preuve
   de membrane est refusé (la révocation prime sur la preuve).

### Critères d'acceptation

| Réf. | Critère | Mesure |
| --- | --- | --- |
| T-08 | Révocation | Job arrêté < 5 s, deny list appliquée |
| — | Session en cours | Fermeture observée côté worker et côté demandeur |
| — | Ré-admission | Preuve valide + révocation = refus, message explicite |
| — | Traçabilité | Entrée `Revocation` signée, visible et vérifiable par tous |
| — | Non-régression | SG1 (admission) reste vert |

### Dépendances et risques

- Dépend du banc CH3 (deux nœuds réels) pour mesurer un délai crédible.
- Choix d'architecture à trancher : nouvelle DNA `relay-market` (comme le propose
  le cahier, pour ne pas casser l'epoch actuelle) **ou** extension de
  `agent-registry` avec un nouvel epoch. Le cahier recommande la première.
- Risque : Holochain ne supprime pas d'entrée ; la révocation est donc un fait
  ajouté, pas un effacement — à dire explicitement.

---

## CH7 — Audit externe, et un pilote qui assume l'absence d'audit

### Objectif

Rendre possible un pilote contrôlé **tout en écrivant noir sur blanc que l'audit
n'a pas eu lieu**, et préparer le dossier qui rendra cet audit utile et finançable.

### Ce qui existe déjà

- `docs/SECURITY.md` : modèle de menace, tableau des invariants, limites connues ;
- un historique de tests négatifs (preuve volée, rejeu, mauvais émetteur, MTM) ;
- des builds reproductibles (prérequis rare et précieux pour un audit) ;
- une CI stricte (tests, Clippy `-D warnings`, CodeQL, harnais SG1).

### À construire

1. **Kit de pilote** (`docs/PILOT_KIT.md`) : périmètre autorisé et interdit (pas de
   données personnelles, pas de production), engagement de non-garantie, procédure
   d'incident, contacts.
2. **Dossier d'audit** (`docs/AUDIT_PACKAGE.md`) : périmètre proposé, invariants à
   vérifier, inventaire des tests, SBOM, procédure de reproduction des binaires,
   liste des manques connus.
3. **Cahier des charges d'audit** : prestataires, budget indicatif, durée,
   livrables attendus (rapport public + délai de correction).
4. **Télémétrie de pilote minimale et opt-in** : compteurs agrégés (nœuds actifs,
   jobs réussis/échoués, latences) — **jamais** de prompt, jamais de contenu.
5. **Bandeau d'état** dans l'interface : « alpha, non audité, ne pas utiliser pour
   des données sensibles », non désactivable par l'utilisateur final.

### Critères d'acceptation

| Critère | Mesure |
| --- | --- |
| Kit de pilote | Document relu, signé par les participants, périmètre explicite |
| Dossier d'audit | Complet : périmètre, invariants, tests, SBOM, reproduction |
| Bandeau | Présent dans l'installeur et l'application, non masquable |
| Télémétrie | Opt-in, agrégée, auditée par recherche automatique de fuite de contenu |
| Pilote | 5–10 utilisateurs, 2 semaines, rapport publié (succès **et** incidents) |

### Dépendances et risques

- **Budget d'audit** : décision du propriétaire ; sans budget, le dossier reste
  prêt mais l'audit n'a pas lieu — et le pilote reste explicitement « non audité ».
- Risque : qu'un pilote soit présenté comme une validation. Le kit doit l'interdire
  par écrit.

---

## Séquencement proposé

Trois vagues, avec ce que chacune débloque :

**Vague 1 — mesurer (2 à 4 semaines, sans nouveau logiciel produit)**
CH3 (calcul réparti réel) puis CH4 (GPU/téléphone, dès que le matériel est là).
Résultat : les chiffres qui manquent, et la décision réplique/pipeline tranchée.
*Bloqueur :* une deuxième machine, un PC GPU, un téléphone.

**Vague 2 — rendre utilisable (3 à 5 mois)**
CH6 (révocation, sur le banc de la vague 1) puis CH1 (installeur Windows).
Résultat : un produit qu'un non-technicien installe, et un réseau qu'un
administrateur peut réellement gérer.
*Décisions :* certificat de signature, hébergement du relais.

**Vague 3 — élargir et durcir (3 à 6 mois)**
CH2 (Android, si G1 est vert) puis CH5 (stealth) puis CH7 (audit + pilote).
Résultat : un pilote assumé, avec un dossier d'audit et, si le budget le permet,
un audit externe.

> Les vagues ne sont pas strictement séquentielles : CH1 et CH6 peuvent avancer en
> parallèle de CH3 si deux personnes travaillent. En solo, l'ordre ci-dessus est
> celui qui minimise le risque de construire sur des chiffres inexistants.

---

## Décisions qui appartiennent au propriétaire

| # | Décision | Pourquoi elle bloque | Défaut recommandé |
| --- | --- | --- | --- |
| D1 | Certificat de signature Windows | Sans lui, l'installeur alerte l'utilisateur | Acheter un certificat OV (~200–400 €/an) |
| D2 | Hébergement du relais | Conditionne CH5 et la moitié réseau de SG1 | Un relais dédié, opéré par l'administrateur du mesh |
| D3 | Budget d'audit | Conditionne CH7 et tout pilote public | Chiffrer avant la vague 3 ; sinon pilote privé seulement |
| D4 | Matériel de mesure | Conditionne CH3 et CH4 | Une 2ᵉ machine + un PC GPU + un téléphone, prêtés ou achetés |
| D5 | Réplique ou pipeline par défaut | Détermine la complexité de CH1 et CH3 | Réplique en v1 (le cahier le recommande déjà) |

---

## Ce que ce document ne promet pas

- Pas de date de mise en service : les durées ci-dessus sont des bandes d'effort
  pour une personne, pas un engagement.
- Pas de promesse d'anonymat : le mode furtif réduit la métadonnée, il ne rend pas
  anonyme.
- Pas de promesse de performance : les chiffres viendront des mesures de CH3 et
  CH4, et seront publiés avec leurs limites.
- Pas de conformité réglementaire : aucune certification n'est revendiquée.
