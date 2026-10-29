# Pourquoi AInonymous ?

*Document de présentation — public non technique. Version française.*

---

## En une phrase

**AInonymous permet à plusieurs machines — des PC, des serveurs, plus tard des
téléphones — de mettre en commun leur puissance pour faire tourner des modèles
d'IA, entre membres d'un même réseau privé, sans passer par un serveur central.**

Aujourd'hui, c'est un **prototype de recherche** : les fondations techniques sont
réelles et testées, mais il n'existe pas encore de version que Monsieur ou Madame
Tout-le-monde installe en trois clics. Ce document dit exactement où en est le
projet, ce qu'il permet déjà, et ce qu'il ne permet pas encore.

---

## Le problème

**1. L'IA utile est concentrée.** Faire tourner un modèle de langage demande de la
mémoire et de la puissance. Résultat : soit on loue un service en ligne (les
données sortent, on paie à l'usage, on dépend d'un fournisseur), soit on achète une
machine dédiée coûteuse — que l'on utilise souvent quelques heures par jour.

**2. Le matériel existant dort.** Une petite entreprise, un laboratoire, une
association disposent souvent de plusieurs ordinateurs corrects. Additionnés, ils
représentent une capacité réelle. Séparément, aucun ne suffit à faire tourner le
modèle dont ils auraient besoin.

**3. Le « local » et le « partagé » sont présentés comme incompatibles.** Soit on
garde tout chez soi et on est limité par sa machine, soit on externalise et on perd
la maîtrise. Peu d'options existent entre les deux.

---

## L'idée

Un **maillage** : chaque machine garde son identité, ses clés et ses données ; le
réseau sert à se trouver, à s'autoriser et à se passer du travail.

L'image la plus juste n'est pas « le cloud », c'est **un atelier partagé**. Chaque
membre a sa clé, son établi et ses outils. Le règlement de l'atelier dit qui peut
entrer et ce qu'il peut utiliser. Quand une commande demande plus de bras qu'un
seul établi n'en a, les membres se la répartissent — sans qu'un contremaître
central ne surveille chaque geste.

Trois principes en découlent :

- **On n'entre pas dans le maillage par hasard.** L'accès se fait sur invitation,
  avec une preuve signée par l'administrateur du réseau.
- **Les machines se parlent directement**, par un canal chiffré, sans intermédiaire
  qui lirait le contenu.
- **Chaque membre reste maître de sa machine** : ce qu'il partage, quand, et avec
  qui.

---

## Comment ça marche, sans jargon

Quatre briques, et à quoi elles servent concrètement :

| La brique | En clair | Ce que ça évite |
| --- | --- | --- |
| **L'annuaire partagé** (Holochain) | Un carnet de membres et de capacités, tenu collectivement, où chacun ne peut écrire qu'en son nom | Un serveur central qui décide qui existe et qui a le droit |
| **Le canal direct** (QUIC + TLS) | Un tuyau chiffré, de machine à machine, avec vérification d'identité aux deux bouts | Qu'un intermédiaire lise ou modifie ce qui circule |
| **L'admission sur preuve** | L'administrateur du réseau signe une autorisation pour une machine précise ; sans elle, l'entrée est refusée | Les réseaux « ouverts à tous » où n'importe qui peut se brancher |
| **Le calcul réparti** | Un modèle peut être découpé en étages, chaque machine traitant une partie, ou simplement copié sur plusieurs machines pour servir plus de demandes | Devoir acheter une machine surdimensionnée pour un usage intermittent |

---

## À qui cela s'adresse

- **PME et professions réglementées** qui veulent utiliser un modèle de langage sur
  des documents internes, sans les envoyer chez un prestataire.
- **Laboratoires, collectivités, associations** qui disposent d'un parc
  informatique et veulent une capacité d'IA mutualisée, administrée par elles.
- **Équipes techniques** qui préfèrent un outil open source, auditable, à un
  service opaque — et qui acceptent, pour l'instant, de travailler avec un
  prototype.
- **Propriétaires de machines puissantes** (PC avec carte graphique) qui cherchent
  à valoriser une capacité inutilisée dans un cadre de confiance.

Ce que cela ne remplace pas : un service d'IA grand public prêt à l'emploi, ni une
solution « anonyme ». Voir la section suivante.

---

## Ce qui est déjà vrai (et vérifiable)

Le projet a une règle simple : **une affirmation sans test ne compte pas.** Voici
ce qui est démontré aujourd'hui, avec le test correspondant dans le dépôt.

- **Deux vrais nœuds s'admettent (ou se refusent) correctement.** Deux logiciels
  Holochain 0.7 démarrent, reçoivent une autorisation signée, et rejoignent le
  réseau privé. Sans autorisation, avec une autorisation destinée à une autre
  machine, à un autre réseau, ou signée par une autre clé : le refus est net.
  → `scripts/sg1/run_sg1.py`, exécuté à chaque modification du code.
- **Le canal chiffré vérifie réellement l'identité des deux côtés**, et un
  intrus ne peut pas « griller » l'accès d'un membre légitime.
  → `crates/ainonymous-quic/tests/mtls_handshake.rs`.
- **Le découpage d'un modèle entre deux machines donne le même résultat** qu'un
  calcul sur une seule machine (à l'octet près, sur le banc d'essai).
  → `spike-pipeline/`.
- **Les paquets logiciels sont reproductibles** : n'importe qui peut reconstruire
  exactement les mêmes fichiers que l'équipe, et le vérifier.
  → `deploy/containers/repro/Dockerfile`.
- **La qualité est contrôlée automatiquement** à chaque changement : compilation,
  tests, analyse de sécurité, et interdiction des avertissements.

## Ce qui n'existe pas encore

Dire le contraire serait malhonnête, et c'est précisément ce que le projet refuse
de faire :

- **Pas encore de version installable pour le grand public.** Il n'y a ni
  installeur, ni version publiée : aujourd'hui, il faut être technicien pour faire
  tourner le logiciel.
- **Le calcul réparti sur plusieurs machines réelles n'est pas encore mesuré.**
  Les preuves existantes sont soit en local sur une machine, soit sur deux
  processus ; la mesure sur un vrai réseau et sur des machines grand public reste
  à faire.
- **Téléphone et carte graphique** : les bancs d'essai existent, les mesures non.
- **Pas d'anonymat, pas de « zero-knowledge ».** Le projet ne le prétend nulle
  part. Les identités sont pseudonymes, mais un observateur du réseau peut voir
  qui parle à qui.
- **Pas de révocation en temps réel** : retirer un membre demande aujourd'hui de
  reconstruire le réseau.
- **Pas d'audit de sécurité externe.** Le projet s'interdit tout pilote public
  avant cet audit.

---

## Pourquoi c'est différent

- **« Privé d'abord ».** Le mode par défaut visé est un réseau fermé, sur
  invitation, pas une place de marché publique. C'est moins spectaculaire, c'est
  beaucoup plus utile pour une organisation réelle.
- **Pas de jetons, pas de spéculation.** Aucune crypto-monnaie dans le modèle : la
  contrepartie entre membres est une question organisationnelle, pas financière.
- **Tout est ouvert et vérifiable.** Licence Apache 2.0, code et tests publics,
  chiffres mesurés publiés avec leurs limites.
- **L'honnêteté comme méthode.** Chaque document du dépôt indique ce qui est
  prouvé et ce qui ne l'est pas — y compris cette page.

---

## À quoi ressemblerait un premier pilote

L'étape suivante n'est pas un lancement commercial, c'est un **pilote accompagné** :

- **Ce qu'il faut** : 3 à 10 machines (PC ou serveurs) sur un réseau maîtrisé, un
  cas d'usage précis (par exemple : recherche documentaire interne, assistant de
  rédaction sur documents confidentiels), et deux semaines.
- **Ce que l'on mesure** : le temps d'installation, le temps jusqu'à la première
  réponse utile, la qualité des réponses sur le cas d'usage, et les incidents.
- **Ce que l'on produit** : un rapport de pilote et une liste de priorités — pas
  une promesse de gain financier, que le projet s'interdit d'avancer sans mesures.

Si vous voulez en discuter, ouvrez une discussion sur le dépôt
[GitHub](https://github.com/Geoking2104/AInonymous) ; si vous êtes technique, le
[README](../README.md) et la [documentation](README.md) donnent le détail exact.

---

## Lexique express

| Terme | Traduction |
| --- | --- |
| **Nœud** | Une machine qui participe au réseau. |
| **Maillage / mesh** | L'ensemble des nœuds qui se connaissent et se font confiance. |
| **Holochain** | La technologie qui tient l'annuaire partagé, sans serveur central. |
| **QUIC / TLS** | Le « tuyau » chiffré qui relie deux machines. |
| **Preuve de membrane** | L'autorisation signée qui permet à une machine d'entrer. |
| **Modèle (LLM)** | Le programme d'IA qui produit du texte. |
| **Pipeline (découpage)** | Répartir un modèle en étages sur plusieurs machines. |
| **Alpha** | Version de recherche, instable, pas prête pour un usage grand public. |
