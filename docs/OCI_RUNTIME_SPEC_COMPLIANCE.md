# Conformité à opencontainers/runtime-spec

Référence : https://github.com/opencontainers/runtime-spec

## Ce que ce document couvre — et ce qu'il ne couvre pas

`runtime-spec` définit le format d'un **bundle OCI** (`config.json` + rootfs)
et le cycle de vie normatif (`create` → `start` → `kill` → `delete`) qu'un
**runtime** (runc, crun, etc.) doit implémenter pour être conforme. Ni
AInonymous ni HybridNode n'implémentent un runtime : on s'appuie sur
Docker/containerd + runc, qui sont déjà des implémentations conformes.

Ce que ce projet contrôle, en revanche, c'est **la forme du bundle produit**
(via l'image Docker et sa config au lancement) et **le comportement de
l'application à l'intérieur** de ce bundle. C'est ce périmètre que ce
document couvre : chaque décision de packaging ci-dessous est reliée à un
champ concret du `config.json` OCI tel que défini par la spec, pour que la
conformité soit vérifiable plutôt qu'affirmée.

Fichiers concernés :
- `docker/ainonymous-daemon.Dockerfile`
- `docker/hybridnode-daemon.Dockerfile`
- `docker-compose.yml`
- `crates/ainonymous-daemon/src/main.rs` (shutdown_signal)
- `crates/hybridnode-core/src/lib.rs` (HybridNode::run)

## `process` — le process exécuté dans le conteneur

| Champ OCI (`config.json`) | Origine dans ce repo | Détail |
|---|---|---|
| `process.args` | `ENTRYPOINT`/`CMD` du Dockerfile | Binaire seul, pas de shell wrapper (`ENTRYPOINT ["/usr/local/bin/..."]` en forme exec, pas en forme shell) — le process du binaire est bien PID 1 dans le conteneur, il reçoit directement les signaux du runtime. |
| `process.env` | `ENV` du Dockerfile + `environment:` du compose | `AINON_CONFIG`, `RUST_LOG`. |
| `process.cwd` | `WORKDIR` du Dockerfile | `/data` (ainonymous-daemon), `/config` (hybridnode-daemon). |
| `process.user.uid` / `.gid` | `USER` du Dockerfile | Non-root dans les deux images (uid 10001 / 10002). Le runtime OCI place ces valeurs dans `process.user` du bundle ; aucune étape (y compris l'installation de paquets apt) ne s'exécute en root après le changement d'utilisateur. |
| `process.terminal` | (implicite, non fixé) | `false` par défaut chez Docker pour un conteneur sans `-t` — correct pour un daemon sans TTY. |
| `process.capabilities` | `cap_drop: [ALL]` (compose) | Aucune capability Linux ajoutée : ni bind sur port privilégié, ni accès réseau bas niveau, ni ptrace. Le set effectif dans `config.json` est vide. |
| `process.noNewPrivileges` | `security_opt: [no-new-privileges:true]` (compose) | Empêche un binaire setuid dans l'image d'élever ses privilèges au-delà de l'utilisateur du conteneur — non pertinent ici (aucun setuid dans l'image) mais defense-in-depth standard. |
| `process.rlimits` | non fixé | Pas de limite applicative particulière identifiée à ce jour ; laissé aux valeurs par défaut du runtime. À revisiter si `ulimit -n` (fds ouverts par les sessions QUIC) devient un facteur limitant en charge. |

## `root` — le rootfs du conteneur

`root.readonly: true` (via `read_only: true` dans le compose) : le rootfs
issu de l'image est monté en lecture seule par le runtime. Seuls les
répertoires explicitement montés (`/data`, `/tmp` en tmpfs) sont
inscriptibles. Ceci limite la surface si le process est compromis (aucune
écriture possible dans `/usr/local/bin`, `/etc`, etc.) et est directement
vérifiable dans le `config.json` généré par Docker (`root.readonly: true`).

## `mounts` — montages du bundle

| Volume | Type OCI | Usage |
|---|---|---|
| `./config/*.{toml,yaml}:/config/*:ro` | bind mount, `ro` | Config applicative — jamais bakée dans l'image (secrets/topologie séparés du binaire, conforme à la pratique "config externalisée"). |
| `ainonymous-models:/data/models` | volume nommé | Modèles GGUF — trop volumineux et non déterministes pour vivre dans l'image. |
| `/tmp` (tmpfs) | tmpfs mount | Compense `root.readonly` pour les besoins d'écriture temporaire (le binaire lui-même n'écrit rien de connu en dehors de `/data`/`/config` à ce jour). |

**Limite connue et volontairement non corrigée par ce travail** :
`ainonymous-daemon` lie son serveur REST interne sur `127.0.0.1` en dur
(`main.rs` : `let addr = format!("127.0.0.1:{}", config.daemon_port)`). Un
`EXPOSE`/port-map Docker sur ce port ne le rendra donc **jamais** joignable
depuis l'extérieur du conteneur — y compris par les health checks HTTP d'un
orchestrateur externe (Kubernetes `livenessProbe.httpGet` cible l'IP du Pod,
pas la loopback du conteneur). Le `HEALTHCHECK` du Dockerfile fonctionne
car Docker l'exécute *dans* le namespace réseau du conteneur (donc
`127.0.0.1` y est valide), mais un `livenessProbe` Kubernetes classique ne
fonctionnerait pas tel quel. Correction possible si nécessaire : rendre le
bind configurable (`0.0.0.0` par défaut ou via `DaemonConfig`) — décision
volontairement laissée à l'équipe plutôt que changée silencieusement dans
le cadre de ce packaging, car elle a des implications de sécurité (exposer
le plan de contrôle REST).

## `linux.resources` — cgroups

`deploy.resources.limits/reservations` dans `docker-compose.yml` (CPU,
mémoire) se traduisent en `linux.resources.cpu` et `linux.resources.memory`
dans le `config.json` généré. Valeurs de départ raisonnables pour un nœud
de mesh (ainonymous-daemon : jusqu'à 8 Go / 4 CPU, dimensionné pour
héberger un modèle quantifié via `llama-server` en enfant du process ;
hybridnode-daemon : 512 Mo / 1 CPU, charge légère observabilité + SD-WAN).
À ajuster selon la taille réelle des modèles déployés.

## Cycle de vie (`runtime.md` — create/start/kill/delete) et signaux

C'est le point qui nécessitait un vrai changement de code, pas seulement du
packaging. Le cycle de vie normatif prévoit que l'opération `kill` envoie un
signal (SIGTERM par défaut pour un arrêt propre) au process du conteneur, et
que le runtime passe à `delete` — via SIGKILL si besoin — après un délai de
grâce si le process n'a pas quitté. `docker stop` et la terminaison de Pod
Kubernetes suivent ce même modèle.

Avant ce travail, ni `ainonymous-daemon` ni `hybridnode-daemon` n'installaient
de handler SIGTERM explicite :
- `hybridnode-core::HybridNode::run()` n'attendait que `tokio::signal::ctrl_c()`
  (SIGINT uniquement sous Unix — ne réagit pas à SIGTERM).
- `ainonymous-daemon::main()` bloquait sur `axum::serve(...).await` sans
  mécanisme d'arrêt, et surtout : `ainonymous-daemon` démarre `llama-server`
  comme **process enfant** (`LlamaManager`, `crates/ainonymous-daemon/src/llama.rs`).
  `LlamaManager` a bien un `impl Drop` qui tue ce process enfant — mais Drop
  ne s'exécute que si le process Rust termine via un retour normal de
  `main()`, pas sur SIGTERM par défaut (action par défaut du signal =
  terminaison immédiate du process, sans unwind). Résultat concret sans ce
  correctif : `docker stop` sur `ainonymous-daemon` aurait laissé
  `llama-server` orphelin dans le conteneur jusqu'au SIGKILL du groupe de
  process par le runtime (fonctionnellement récupéré par le runtime au
  niveau du conteneur, mais pas par un arrêt propre applicatif — pas de
  flush, pas de déconnexion propre du conducteur Holochain, etc.).

Correctif appliqué dans les deux binaires : un handler explicite
`tokio::signal::unix::signal(SignalKind::terminate())` couplé à `ctrl_c()`
via `tokio::select!`. Pour `ainonymous-daemon`, ce signal déclenche
`axum::serve(...).with_graceful_shutdown(...)`, qui retourne proprement une
fois le shutdown terminé ; `main()` se termine alors normalement et
`LlamaManager` (variable locale toujours en scope) est droppé — ce qui
exécute `child.kill()` sur `llama-server`.

`init: true` dans `docker-compose.yml` ajoute une couche de défense
supplémentaire, indépendante du code applicatif : Docker insère `tini` comme
PID 1 réel, qui relaie les signaux et *reap* les zombies — utile si un futur
sous-process échappe au chemin de nettoyage applicatif décrit ci-dessus.

## `hooks` (prestart/createRuntime/.../poststop)

Aucun hook OCI personnalisé n'est nécessaire pour ce déploiement : pas de
préparation de rootfs spécifique, pas de VPN/side-car à armorcer avant le
démarrage du process. Non utilisé — mentionné ici pour être exhaustif sur
les sections de la spec, pas parce qu'un hook serait requis.

## Limites de cette vérification

- Aucun moteur Docker n'est disponible dans le sandbox où ce travail a été
  effectué : les deux Dockerfiles et le compose n'ont été validés que
  **structurellement** (syntaxe, cohérence des chemins/ports avec le code
  source réellement lu), pas par un build réel. À confirmer par
  `docker build` + `docker compose up` côté utilisateur avant tout usage en
  production.
- `hybridnode-daemon` avec la feature `vmanage` (SD-WAN réel, désactivée par
  défaut) n'a pas été packagée spécifiquement — le Dockerfile ne construit
  que la configuration par défaut (`mock-sdwan`).
- Le `HEALTHCHECK` d'`ainonymous-daemon` cible le port par défaut (8890) ;
  si `AINON_CONFIG` définit un `daemon_port` différent, il faut ajuster la
  commande `HEALTHCHECK` en conséquence (ou la piloter par variable
  d'environnement dans un futur ajustement — non fait ici pour rester
  simple).
