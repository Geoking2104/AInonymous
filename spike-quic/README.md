# Spike S4 (partie réseau) — deux nœuds QUIC mTLS (porte G4, #30, #32)

Crate `relay-quic-spike` (bibliothèque + CLI `quic-spike`). Il réutilise la pile
existante `ainonymous-quic` : mTLS Ed25519 avec épinglage des deux clés, token de
session à usage unique (TTL 30 s) et `ActivationTransfer`. Le plan de contrôle
Holochain est remplacé par un échange de l'offre de session **hors bande** (un
fichier JSON).

## Tests automatisés (boucle locale)

```bash
cargo test -p relay-quic-spike   # 7 tests
```

| Test | Vérifie |
|---|---|
| `session_nominale_echo_et_activations_integres` | handshake mTLS, token, écho, 4 Mio d'activations accusées avec SHA-256 et plage de couches transmise |
| `client_non_autorise_refuse_meme_avec_token_vole` | un autre nœud qui détient le token est refusé (clé client ≠ `client_pubkey`) |
| `rejeu_du_token_refuse` | le token est consommé à la première utilisation |
| `token_altere_refuse` | un token inconnu est refusé |
| `faux_worker_refuse_par_le_client` | MITM : le client refuse un serveur dont la clé ≠ `peer_pubkey` |
| `offre_expiree_refusee` | une offre expirée est refusée côté client |
| `token_vole_non_brule_par_un_refus_mtls` | un refus mTLS ne consomme pas le token : le client légitime réutilise ensuite la même offre |

Constat S4 (corrigé) : dans `QuicListener`, le token était retiré du registre
**avant** la vérification de la clé client. Un tiers qui avait intercepté le
token pouvait donc le « brûler » (déni de service sur cette session), sans
pouvoir l'utiliser. Corrigé côté production (`crates/ainonymous-quic`) : la clé
client mTLS est vérifiée avant toute consommation du token, avec un test de
régression (`token_vole_non_brule_par_un_refus_mtls`).

## Mesures en boucle locale (box de développement, 8 vCPU Xeon, sans GPU)

`cargo build --release -p relay-quic-spike && ./target/release/quic-spike loopback`,
3 exécutions le 27/10/2026. **127.0.0.1 n'est pas représentatif d'un LAN** : ces
chiffres bornent seulement le coût CPU de la pile (TLS + QUIC + copie). Ils ne
mesurent ni le réseau ni le Wi-Fi.

| Exécution | Setup session (handshake + token + 1er écho) | RTT p50 / p95 (écho 64 o, n=200) | 100 Mio | Débit |
|---|---|---|---|---|
| 1 | 1,36 ms | 0,043 / 0,285 ms | 260,8 ms | 3 217 Mb/s |
| 2 | 1,27 ms | 0,040 / 0,093 ms | 267,9 ms | 3 131 Mb/s |
| 3 | 1,61 ms | 0,049 / 0,064 ms | 276,1 ms | 3 039 Mb/s |

Les mesures brutes sont dans [`results/loopback.json`](results/loopback.json).

## Mesure sur le vrai LAN / Tailscale (matériel de Geoffroy)

Seuils §6.3 : RTT LAN < 2 ms (VPN < 50 ms), débit ≥ 500 Mb/s sur 100 Mo,
établissement de session LAN < 1 s.

```bash
# Machine A (coordinateur, ex. PC Windows ou Linux)
quic-spike pubkey --key-file client.key            # -> CLIENT_KEY
# Machine B (worker)
quic-spike serve --bind 0.0.0.0:7700 --advertise <IP_B>:7700 \
    --key-file worker.key --client-key <CLIENT_KEY> --offer-out offer.json
# copier offer.json de B vers A dans les 30 s (régénéré toutes les 20 s), puis sur A :
quic-spike client --offer offer.json --key-file client.key --mode lan
# Tailscale : --advertise <IP_100.x_de_B>:7700 et --mode tailscale
```

Il faut ouvrir le port UDP 7700 sur le pare-feu de B. Faire au moins 3 exécutions
par lien (Ethernet, Wi-Fi, Tailscale) et noter le matériel.
