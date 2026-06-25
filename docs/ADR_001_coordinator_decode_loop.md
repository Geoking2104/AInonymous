# ADR-001 — Boucle de décodage pilotée par le coordinateur (pipeline 2 nœuds)

- Statut : **Accepté** (à implémenter — Phase 2/3)
- Date : 2026-06-25
- Portée : chemin d'inférence distribuée du testnet 2 nœuds

## Contexte

L'audit du code a révélé deux problèmes structurels dans le chemin d'inférence
distribué :

1. **Pas d'initiation côté coordinateur.** `proxy/handlers/chat.rs` ne fait que
   du solo (appel direct à llama-server). Aucun composant n'orchestre :
   plan d'exécution → ouverture QUIC vers le 1er étage → envoi des token_ids →
   réception du flux de tokens → détokenisation → réponse OpenAI.

2. **Boucle de décodage incorrecte pour un vrai pipeline.**
   `daemon/conductor.rs::stream_tokens_to_coordinator` exécute la boucle
   `decode` **localement sur le dernier nœud** (`for _ in 0..max_new_tokens { pipeline.decode(...) }`).
   Ce n'est correct que si le dernier nœud possède **toutes** les couches
   (cas dégénéré = solo). En vrai pipeline-split, le dernier nœud n'a que les
   couches hautes : il ne peut pas régénérer seul le token suivant, qui dépend
   du forward pass complet (embed → couches basses → … → lm_head).

   De plus, le token de sortie d'un nœud non-terminal n'est jamais relayé en
   amont vers le coordinateur (`handle_pipeline_session` forwarde les
   activations puis retourne sans attendre).

## Décision

**Le coordinateur pilote la génération token par token. Chaque token = un
aller-retour complet du pipeline.** La tokenisation et la détokenisation sont
**déléguées** au `pipeline_server.py` du premier/dernier nœud (endpoints
`/tokenize` et `/detokenize`, ajoutés), pour garantir l'alignement exact avec
le tokenizer du modèle et éviter de réimplémenter un tokenizer en Rust.

### Flux cible (2 étages : A = couches `[0,k[`, B = couches `[k,N[`, dernier)

```
Coordinateur C                Nœud A (0..k)             Nœud B (k..N, last)
──────────────                ─────────────             ───────────────────
POST A:/tokenize (prompt) ───►
        ◄─── input_ids
negotiate(A, next=B) ────────► register session
QUIC connect A ──────────────►
PREFILL: token_ids ──────────► prefill[0..k] ──QUIC──► prefill[k..N]
                                                        argmax → tok0 (+texte)
        ◄──────────────── tok0 (relayé par A) ◄────────
yield tok0
   boucle decode (par token):
   send(tok_prev) ──────────► decode[0..k] ───QUIC──► decode[k..N]
                                                        argmax → tok_i (+texte)
        ◄──────────────── tok_i ◄─────────────────────
   yield tok_i ; stop si EOS ou max_tokens
POST B:/clear (fin) ────────►                           clear KV-cache
```

### Conséquences sur le code

- **`pipeline_server.py`** : ✅ fait — `/tokenize`, `/detokenize` ajoutés
  (commit présent).
- **`conductor.rs` (worker)** : `handle_pipeline_session` doit
  (a) pour un nœud non-terminal : après forward des activations, **lire le
  TokenStream du nœud suivant et le relayer** sur sa session entrante ;
  (b) pour le dernier nœud : renvoyer **un token par passe** (supprimer la
  boucle locale `0..max_new_tokens`). La boucle vit chez le coordinateur.
- **Nouveau module coordinateur (daemon)** : `run_pipeline_inference(plan, messages, params)`
  qui implémente le flux ci-dessus et expose un stream de `GeneratedToken`.
- **Daemon** : endpoint `POST /mesh/infer` (SSE) consommé par le proxy.
- **`proxy/chat.rs`** : router Solo (llama-server actuel) vs PipelineSplit
  (→ `/mesh/infer`) selon le plan d'exécution.

### KV-cache

Chaque nœud conserve son KV-cache local indexé par `request_id` (déjà le cas
dans `pipeline_server.py`). En phase decode, seul le dernier token (ou les
hidden states) transite ; le cache évite de recalculer le prompt. À valider en
conditions réelles (risque T3.2 du plan).

## Alternatives écartées

- **Boucle sur le dernier nœud** (état actuel) : incorrecte hors cas solo.
- **Tokenizer Rust (crate `tokenizers`)** : ajoute le téléchargement des
  fichiers modèle + risque de désalignement de template chat. Délégation
  préférée.

## Pourquoi ce n'est pas encore codé en entier

Le reste de la Phase 2 (module coordinateur + réécriture de la boucle worker +
SSE) demande une **validation à l'exécution** (2 nœuds + `cargo`) impossible
dans l'environnement actuel (pas de toolchain Rust). Coder la boucle distribuée
à l'aveugle produirait du code non vérifié au comportement probablement
incorrect. Cet ADR fige le contrat pour une implémentation rapide dès que la
compilation et 2 nœuds sont disponibles.
