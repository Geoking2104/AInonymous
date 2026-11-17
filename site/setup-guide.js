/*
 * AInonymous — guide de mise en route interactif / interactive setup guide.
 *
 * 100 % côté client : aucun appel réseau par défaut, aucune clé d'API.
 * L'« agent » par défaut est un assistant scripté (règles + base de
 * connaissances tirée du dépôt). Un mode optionnel peut interroger un
 * point d'accès OpenAI-compatible LOCAL choisi par l'utilisateur
 * (ex. le proxy du dépôt sur 127.0.0.1:9337) ; en cas d'échec, retour
 * automatique au mode scripté.
 *
 * Toutes les commandes et chemins proviennent du dépôt :
 *   README.md, docs/HOLOCHAIN_BUILD.md, deploy/containers/README.md,
 *   scripts/testnet/conductor_t51.yaml, scripts/testnet/daemon_t51.toml,
 *   deploy/containers/holochain/entrypoint.sh, scripts/sg1/,
 *   tools/private-network-proof/src/main.rs, docs/TESTNET_2NODES.md.
 * Source de vérité textuelle : docs/JOIN_NETWORK.md.
 */
(function () {
  'use strict';

  var REPO = 'https://github.com/Geoking2104/AInonymous';
  var BLOB = REPO + '/blob/main/';
  var STORE_KEY = 'ainonymous-setup-guide-v1';

  // ── Commandes (identiques pour FR/EN) ────────────────────────────────────
  var C = {
    clone: 'git clone https://github.com/Geoking2104/AInonymous.git\ncd AInonymous',
    rust: 'rustup target add wasm32-unknown-unknown\nrustc --version   # 1.91 ou plus récent / 1.91 or newer',
    hcLinux: 'bash scripts/setup_holochain_wsl.sh\nhc --version          # 0.7.0\nholochain --version   # 0.7.0',
    hcWin: 'powershell -ExecutionPolicy Bypass -File scripts/setup_holochain_windows.ps1\nhc --version\nholochain --version',
    build: 'cargo build --workspace\nbash scripts/build-happ.sh release',
    conductor: 'holochain -c scripts/testnet/conductor_t51.yaml',
    install: 'hc client call --port 65000 new-agent\n# → copier la clé uhCAk… / copy the uhCAk… key\nhc client call --port 65000 install-app --app-id ainonymous \\\n  --agent-key <uhCAk...> dnas/ainonymous-core/ainonymous-core.happ\nhc client call --port 65000 enable-app ainonymous\nhc client call --port 65000 add-app-ws 65001 --allowed-origins "*" \\\n  --installed-app-id ainonymous',
    daemon: 'AINON_CONFIG=scripts/testnet/daemon_t51.toml ./target/debug/ainonymous-daemon\n# autre terminal / other terminal\ncurl http://127.0.0.1:18890/health',
    proxy: 'AINON_DAEMON_URL=http://127.0.0.1:18890 ./target/debug/ainonymous-proxy\n# autre terminal / other terminal\ncurl http://127.0.0.1:9337/health\ncurl http://127.0.0.1:9337/v1/ainonymous/mesh/status',
    dockerPre: 'docker compose version\n/absolute/path/to/hc --version   # 0.7.0',
    dockerSh: 'HC_BIN=/absolute/path/to/hc ./scripts/reprovision-containers.sh --confirm-reset',
    dockerPs: '.\\scripts\\reprovision-containers.ps1 -ConfirmReset -HcBin C:\\absolute\\path\\to\\hc.exe',
    dockerCheck: 'docker compose --project-name ainonymous-hc07-v3 ps\ndocker compose --project-name ainonymous-hc07-v3 logs --follow holochain\ncurl http://127.0.0.1:8890/health',
    memberKey: 'hc client call --port <ADMIN_PORT> new-agent\n# → envoyer la clé uhCAk… à l’administrateur / send the uhCAk… key to the admin',
    memberVerify: 'cargo run -p private-network-proof -- verify \\\n  --network-id <network-id> --agent-key <uhCAk...> \\\n  --admin-pubkey-hex <raw 36-byte hex> --proof proof.b64',
    memberRoles: "python3 - <<'PY'\nimport base64, pathlib\nraw = base64.b64decode(pathlib.Path('proof.b64').read_bytes().strip())\nnums = ', '.join(str(b) for b in raw)\npathlib.Path('roles.yaml').write_text(\n    f'hybridnode-core:\\n  type: provisioned\\n  membrane_proof: [{nums}]\\n')\nPY",
    memberInstall: 'hc client call --port <ADMIN_PORT> install-app <hybridnode.happ> <network-seed> roles.yaml \\\n  --app-id hybridnode --agent-key <uhCAk...>\nhc client call --port <ADMIN_PORT> enable-app hybridnode',
    adminKey: 'cargo run -p private-network-proof -- admin-pubkey --seed-hex <64 hex chars>',
    adminDna: '# dnas/hybridnode/dnas/hybridnode-core/workdir/dna.yaml\n#   network_id: "<network-id>"\n#   network_admin_pubkey: [<36 octets / bytes>]\nbash scripts/build-happ.sh release',
    adminProof: 'cargo run -p private-network-proof -- proof \\\n  --network-id <network-id> --agent-key <uhCAk...> \\\n  --seed-hex <64 hex chars> --out proof.b64',
    adminValidate: 'bash scripts/hybridnode/init_project.sh my-project\npython scripts/hybridnode/validate_config.py --production my-project/hybridnode.yaml\npython3 scripts/sg1/run_sg1.py',
    devTest: 'cargo test --workspace\npython3 scripts/sg1/run_sg1.py',
    devTestnet: 'cargo build --workspace\nBIN="$PWD/target/debug" TOTAL_LAYERS=18 \\\n  bash scripts/testnet/run_testnet_2_mock.sh'
  };

  // ── Textes ───────────────────────────────────────────────────────────────
  var T = {
    fr: {
      title: 'Guide IA de mise en route',
      subtitle: 'Assistant scripté, 100 % dans votre navigateur : rien n’est envoyé nulle part.',
      progress: 'Progression',
      reset: 'Recommencer',
      placeholder: 'Posez une question (ports, hash, preuve, Windows…)',
      send: 'Envoyer',
      copy: 'Copier', copied: 'Copié ✓',
      you: 'Vous', bot: 'Guide',
      llmToggle: 'Mode LLM local (optionnel)',
      llmHelp: 'Envoie vos questions à un point d’accès OpenAI-compatible sur votre machine. Le proxy du dépôt (127.0.0.1:9337) n’envoie pas encore d’en-têtes CORS : depuis le site en ligne, le navigateur bloquera probablement l’appel et le guide reviendra au mode scripté.',
      endpoint: 'Point d’accès', model: 'Modèle',
      llmFail: 'Le point d’accès local n’a pas répondu (CORS, serveur arrêté ou accès réseau privé bloqué par le navigateur). Je repasse en mode scripté :',
      hello: 'Bonjour ! Je vous accompagne pas à pas pour faire tourner un nœud AInonymous. Tout ce que je propose vient du dépôt. Rappel honnête : c’est un logiciel **alpha**, le mode public est expérimental et il n’y a **pas d’anonymat réseau**. Que voulez-vous faire ?',
      goals: {
        local: 'Essayer un nœud en local',
        docker: 'Nœud persistant (Docker)',
        member: 'Rejoindre un mesh privé',
        admin: 'Créer un mesh privé',
        dev: 'Tester / contribuer au code'
      },
      askOs: 'Sur quel système ?',
      os: { linux: 'Linux x86_64', wsl: 'Windows + WSL2', win: 'Windows natif', mac: 'macOS', other: 'Autre / ARM' },
      osWarn: 'Attention : le dépôt ne fournit des scripts d’installation Holochain épinglés que pour **x86_64 Linux/WSL2** et **x86_64 Windows**. Sur votre système, installez vous-même `hc` et `holochain` **0.7.0** ; le reste des étapes est identique.',
      winNative: 'Sous Windows natif, compilez les zomes avec un `CARGO_TARGET_DIR` court (ex. `C:\\wz-ain`), sinon `link.exe` échoue avec `LNK1104`. Les commandes Bash des étapes suivantes se lancent depuis WSL ou Git Bash.',
      askHw: 'Quel matériel pour l’inférence ?',
      hw: { gpu: 'GPU NVIDIA', cpu: 'CPU seulement', unk: 'Je ne sais pas' },
      hwNote: 'Noté. Aucune mesure GPU (Windows/CUDA ou téléphone) n’est encore publiée dans le dépôt (chantier CH4) : je ne peux pas vous promettre de performances.',
      plan: 'Voici votre parcours ({n} étapes). Je les affiche une par une ; cochez-les au fur et à mesure (la progression est gardée dans ce navigateur).',
      done: '✅ C’est fait', err: '❌ J’ai une erreur', skip: '⏭ Passer', back: '↩ Étape précédente',
      finished: 'Parcours terminé 🎉 Prochaine étape côté projet : la mesure sur plusieurs machines réelles (CH3). Posez-moi d’autres questions si besoin.',
      noTrouble: 'Pas de piste spécifique pour cette étape. Consultez [docs/JOIN_NETWORK.md](' + BLOB + 'docs/JOIN_NETWORK.md) ou posez votre question avec le message d’erreur.',
      troubleIntro: 'Pistes de dépannage :',
      fallback: 'Je n’ai pas de réponse sûre à cela. Essayez avec un mot-clé (port, hash, preuve, genesis, Windows, Docker, anonymat, révocation, statut) ou lisez [docs/JOIN_NETWORK.md](' + BLOB + 'docs/JOIN_NETWORK.md).',
      stepWord: 'Étape'
    },
    en: {
      title: 'AI setup guide',
      subtitle: 'Scripted assistant running 100% in your browser: nothing is sent anywhere.',
      progress: 'Progress',
      reset: 'Start over',
      placeholder: 'Ask a question (ports, hash, proof, Windows…)',
      send: 'Send',
      copy: 'Copy', copied: 'Copied ✓',
      you: 'You', bot: 'Guide',
      llmToggle: 'Local LLM mode (optional)',
      llmHelp: 'Sends your questions to an OpenAI-compatible endpoint on your own machine. The repository proxy (127.0.0.1:9337) does not send CORS headers yet: from the live site, the browser will probably block the call and the guide falls back to scripted mode.',
      endpoint: 'Endpoint', model: 'Model',
      llmFail: 'The local endpoint did not answer (CORS, server stopped, or private-network access blocked by the browser). Falling back to scripted mode:',
      hello: 'Hi! I will walk you through running an AInonymous node, step by step. Everything I suggest comes from the repository. Honest reminder: this is **alpha** software, public mode is experimental and there is **no network anonymity**. What would you like to do?',
      goals: {
        local: 'Try a local node',
        docker: 'Persistent node (Docker)',
        member: 'Join a private mesh',
        admin: 'Create a private mesh',
        dev: 'Test / contribute code'
      },
      askOs: 'Which operating system?',
      os: { linux: 'Linux x86_64', wsl: 'Windows + WSL2', win: 'Native Windows', mac: 'macOS', other: 'Other / ARM' },
      osWarn: 'Note: the repository only ships pinned Holochain install scripts for **x86_64 Linux/WSL2** and **x86_64 Windows**. On your system, install `hc` and `holochain` **0.7.0** yourself; the remaining steps are the same.',
      winNative: 'On native Windows, build the zomes with a short `CARGO_TARGET_DIR` (e.g. `C:\\wz-ain`), otherwise `link.exe` fails with `LNK1104`. Run the Bash commands of the next steps from WSL or Git Bash.',
      askHw: 'What hardware for inference?',
      hw: { gpu: 'NVIDIA GPU', cpu: 'CPU only', unk: 'Not sure' },
      hwNote: 'Noted. No GPU (Windows/CUDA or phone) measurement is published in the repository yet (workstream CH4), so I cannot promise any performance.',
      plan: 'Here is your path ({n} steps). I will show them one at a time; tick them off as you go (progress is kept in this browser).',
      done: '✅ Done', err: '❌ I got an error', skip: '⏭ Skip', back: '↩ Previous step',
      finished: 'Path complete 🎉 Next project milestone: measuring on several real machines (CH3). Ask me anything else.',
      noTrouble: 'No specific hint for this step. See [docs/JOIN_NETWORK.md](' + BLOB + 'docs/JOIN_NETWORK.md) or ask with the error message.',
      troubleIntro: 'Troubleshooting hints:',
      fallback: 'I have no reliable answer to that. Try a keyword (port, hash, proof, genesis, Windows, Docker, anonymity, revocation, status) or read [docs/JOIN_NETWORK.md](' + BLOB + 'docs/JOIN_NETWORK.md).',
      stepWord: 'Step'
    }
  };

  // ── Étapes ───────────────────────────────────────────────────────────────
  // text/trouble : [fr, en]
  var S = {
    clone: { t: ['Cloner le dépôt', 'Clone the repository'], cmd: [C.clone],
      x: ['Récupérez les sources.', 'Get the sources.'] },
    rust: { t: ['Rust 1.91+ et cible WASM', 'Rust 1.91+ and WASM target'], cmd: [C.rust],
      x: ['Les zomes Holochain 0.7 exigent Rust 1.91 ou plus récent (`str::floor_char_boundary`).', 'Holochain 0.7 zomes need Rust 1.91 or newer (`str::floor_char_boundary`).'],
      k: [['`rustup update stable` si la version est trop ancienne.', '`rustup update stable` if the version is too old.']] },
    holochain: { t: ['Installer Holochain 0.7.0', 'Install Holochain 0.7.0'], cmd: 'HC',
      x: ['Les scripts téléchargent les binaires officiels et vérifient leurs empreintes SHA-256. N’utilisez pas un `hc` plus ancien : les formats de bundle en dépendent.', 'The scripts download the official binaries and verify their SHA-256 digests. Do not use an older `hc`: bundle formats depend on it.'],
      k: [['Plusieurs versions installées : sélectionnez la bonne avec `HC_BIN=/chemin/absolu/vers/hc`.', 'Several versions installed: select the right one with `HC_BIN=/absolute/path/to/hc`.'],
          ['`hc --version` doit afficher 0.7.0 ; sinon vérifiez votre `PATH` (`~/.local/bin` sous Linux).', '`hc --version` must print 0.7.0; otherwise check your `PATH` (`~/.local/bin` on Linux).']] },
    build: { t: ['Compiler les crates et packer les hApps', 'Build the crates and pack the hApps'], cmd: [C.build],
      x: ['Produit `dnas/ainonymous-core/ainonymous-core.happ` et `dnas/hybridnode/hybridnode.happ`. Une recompilation locale change l’empreinte (hash) des DNA : suffisant pour un essai local, pas pour rejoindre un réseau épinglé.', 'Produces `dnas/ainonymous-core/ainonymous-core.happ` and `dnas/hybridnode/hybridnode.happ`. A local rebuild changes the DNA hash: fine for a local trial, not for joining a pinned network.'],
      k: [['`Manifest rejected` : vérifiez `hc --version` (0.7.0).', '`Manifest rejected`: check `hc --version` (0.7.0).'],
          ['Windows `LNK1104` : utilisez un `CARGO_TARGET_DIR` court (ex. `C:\\wz-ain`).', 'Windows `LNK1104`: use a short `CARGO_TARGET_DIR` (e.g. `C:\\wz-ain`).']] },
    conductor: { t: ['Démarrer un conducteur de test', 'Start a test conductor'], cmd: [C.conductor],
      x: ['Configuration jetable du dépôt : admin sur le port 65000, données sous `/tmp/ainonymous-t51`, keystore de test (`danger_test_keystore`) **non adapté à une identité persistante**, bootstrap public `bootstrap.holo.host`. Laissez ce terminal ouvert.', 'Disposable repository configuration: admin on port 65000, data under `/tmp/ainonymous-t51`, test keystore (`danger_test_keystore`) **not suitable for a persistent identity**, public bootstrap `bootstrap.holo.host`. Keep this terminal open.'],
      k: [['Données d’une ancienne version : Holochain 0.7 n’est pas compatible avec une base 0.6 ; supprimez `/tmp/ainonymous-t51`.', 'Old data: Holochain 0.7 cannot read a 0.6 database; remove `/tmp/ainonymous-t51`.']] },
    installApp: { t: ['Installer le hApp ainonymous', 'Install the ainonymous hApp'], cmd: [C.install],
      x: ['Mêmes commandes que le conteneur de référence (`deploy/containers/holochain/entrypoint.sh`), adaptées aux ports de test 65000/65001.', 'Same commands as the reference container (`deploy/containers/holochain/entrypoint.sh`), adapted to test ports 65000/65001.'],
      k: [['`No cell found` : vérifiez l’app ID (`ainonymous`), que l’app est activée et son rôle provisionné.', '`No cell found`: check the app ID (`ainonymous`), that the app is enabled and its role provisioned.'],
          ['`Unauthorized app WebSocket` : émettez un nouveau jeton via l’interface admin locale.', '`Unauthorized app WebSocket`: issue a fresh token through the local admin interface.']] },
    daemon: { t: ['Lancer le daemon', 'Start the daemon'], cmd: [C.daemon],
      x: ['Le daemon lit `scripts/testnet/daemon_t51.toml` (API HTTP sur 18890, QUIC sur 9010). Sans `AINON_CONFIG`, il crée une config par défaut dans `~/.config/ainonymous/config.toml` (API sur 8890). Une panne du conducteur est fatale : pas de repli silencieux.', 'The daemon reads `scripts/testnet/daemon_t51.toml` (HTTP API on 18890, QUIC on 9010). Without `AINON_CONFIG` it creates a default config in `~/.config/ainonymous/config.toml` (API on 8890). A conductor failure is fatal: no silent fallback.'],
      k: [['Erreur de connexion au conducteur : le conducteur de l’étape précédente doit tourner et l’app `ainonymous` être activée.', 'Conductor connection error: the conductor from the previous step must be running and the `ainonymous` app enabled.'],
          ['L’inférence réelle demande `llama-server` (champ `llama_server_bin`) et un modèle dans `models_dir`.', 'Real inference needs `llama-server` (`llama_server_bin`) and a model in `models_dir`.']] },
    proxy: { t: ['Lancer le proxy OpenAI-compatible', 'Start the OpenAI-compatible proxy'], cmd: [C.proxy],
      x: ['Le proxy écoute sur `127.0.0.1:9337` et joint l’API HTTP du daemon via `AINON_DAEMON_URL` (défaut `http://127.0.0.1:8890` ; 18890 avec la config de test). Variante : `./target/debug/ainonymous start` lance le daemon puis le proxy et attend leur `/health`.', 'The proxy listens on `127.0.0.1:9337` and reaches the daemon HTTP API through `AINON_DAEMON_URL` (default `http://127.0.0.1:8890`; 18890 with the test config). Alternative: `./target/debug/ainonymous start` launches the daemon then the proxy and waits for their `/health`.'],
      k: [['`mesh/status` répond « connecting » : le daemon n’obtient pas de plan d’exécution (aucun nœud annoncé pour ce modèle).', '`mesh/status` says “connecting”: the daemon gets no execution plan (no node announced for that model).'],
          ['`/v1/ainonymous/mesh/nodes` renvoie 503 : le proxy ne joint pas le daemon (vérifiez `AINON_DAEMON_URL`) ou le daemon ne joint pas le conducteur.', '`/v1/ainonymous/mesh/nodes` returns 503: the proxy cannot reach the daemon (check `AINON_DAEMON_URL`) or the daemon cannot reach the conductor.']] },
    dockerPre: { t: ['Prérequis Docker', 'Docker prerequisites'], cmd: [C.dockerPre],
      x: ['Docker Engine + Compose v2, Rust 1.91+, cible WASM, `hc` 0.7.0 local (ou via `HC_BIN`), OpenSSL pour le script Bash.', 'Docker Engine + Compose v2, Rust 1.91+, WASM target, local `hc` 0.7.0 (or via `HC_BIN`), OpenSSL for the Bash script.'] },
    dockerUp: { t: ['Provisionner la pile conteneurs', 'Provision the container stack'], cmd: 'DOCKER',
      x: ['**Destructif** : supprime les volumes nommés du projet `ainonymous-hc07-v3`, recompile les hApps et démarre une pile neuve (conducteur + deux daemons). Le mot de passe du keystore est créé dans `deploy/secrets/` (ignoré par Git).', '**Destructive**: removes the `ainonymous-hc07-v3` project named volumes, rebuilds the hApps and starts a fresh stack (conductor + two daemons). The keystore password is created under `deploy/secrets/` (ignored by Git).'],
      k: [['Le script refuse tout `hc` autre que 0.7.0.', 'The script refuses any `hc` other than 0.7.0.']] },
    dockerCheck: { t: ['Vérifier la pile', 'Check the stack'], cmd: [C.dockerCheck],
      x: ['Seule l’API du daemon est exposée, sur `127.0.0.1:8890` ; QUIC sur 9000/UDP. La config de référence utilise les bootstrap/relais publics de Holochain.', 'Only the daemon API is exposed, on `127.0.0.1:8890`; QUIC on 9000/UDP. The reference config uses Holochain’s public bootstrap/relay.'],
      k: [['Le conducteur n’est pas « healthy » : `docker compose --project-name ainonymous-hc07-v3 logs --follow holochain`.', 'Conductor not healthy: `docker compose --project-name ainonymous-hc07-v3 logs --follow holochain`.']] },
    memberKey: { t: ['Générer votre clé d’agent', 'Generate your agent key'], cmd: [C.memberKey],
      x: ['Le port admin dépend de votre conducteur (65000 pour la config de test). Envoyez la clé `uhCAk…` à l’administrateur du mesh ; il vous renverra une preuve (`proof.b64`), l’identifiant réseau, le seed réseau et le bundle `hybridnode.happ` du réseau.', 'The admin port depends on your conductor (65000 for the test config). Send the `uhCAk…` key to the mesh administrator; they return a proof (`proof.b64`), the network id, the network seed and the network’s `hybridnode.happ` bundle.'] },
    memberVerify: { t: ['Vérifier la preuve hors ligne', 'Verify the proof offline'], cmd: [C.memberVerify],
      x: ['Vérifie que la preuve est signée par la clé admin, pour votre agent et ce réseau, avant toute installation.', 'Checks the proof is signed by the admin key, for your agent and this network, before installing anything.'] },
    memberRoles: { t: ['Préparer les role settings', 'Prepare the role settings'], cmd: [C.memberRoles],
      x: ['La preuve est fournie à l’installation (genèse) sous forme de suite d’entiers YAML, comme `roles_yaml()` dans `scripts/sg1/run_sg1.py`. Le nom du rôle est celui du bundle de votre réseau (`hybridnode-core` dans `dnas/hybridnode/happ.yaml`).', 'The proof is supplied at install (genesis) time as a YAML integer sequence, like `roles_yaml()` in `scripts/sg1/run_sg1.py`. The role name is the one in your network bundle (`hybridnode-core` in `dnas/hybridnode/happ.yaml`).'] },
    memberInstall: { t: ['Installer avec la preuve', 'Install with the proof'], cmd: [C.memberInstall],
      x: ['Si la preuve est valide, l’app s’installe et s’active ; sinon la genèse est refusée.', 'With a valid proof the app installs and enables; otherwise genesis is rejected.'],
      k: [['`Genesis rejected` : vérifiez la clé d’agent de la preuve, l’identifiant réseau, la signature et les propriétés DNA du bundle.', '`Genesis rejected`: check the proof’s agent key, network id, signature and the bundle DNA properties.'],
          ['Les pairs ne se voient pas : DNA hashes, seeds réseau et bootstrap/relais doivent être identiques.', 'Peers do not see each other: DNA hashes, network seeds and bootstrap/relay must be identical.']] },
    adminKey: { t: ['Clé d’administration du réseau', 'Network admin key'], cmd: [C.adminKey],
      x: ['Le seed (32 octets hex) est **secret** : il signe toutes les preuves. La commande affiche la clé publique brute de 36 octets à mettre dans `dna.yaml`.', 'The seed (32 hex bytes) is **secret**: it signs every proof. The command prints the raw 36-byte public key for `dna.yaml`.'] },
    adminDna: { t: ['Propriétés DNA et bundle privé', 'DNA properties and private bundle'], cmd: [C.adminDna],
      x: ['Changer `network_id` ou `network_admin_pubkey` change le hash : c’est un nouvel epoch. Distribuez ce bundle à tous les membres.', 'Changing `network_id` or `network_admin_pubkey` changes the hash: it is a new epoch. Distribute this bundle to every member.'] },
    adminProof: { t: ['Émettre une preuve par membre', 'Issue one proof per member'], cmd: [C.adminProof],
      x: ['Une preuve est liée à **un** agent et **un** réseau. L’expiration n’est qu’un contrôle à l’émission ; la révocation en temps réel est en cours (PR #57).', 'A proof is bound to **one** agent and **one** network. Expiry is only an issuance control; real-time revocation is in progress (PR #57).'] },
    adminValidate: { t: ['Valider la config et l’admission', 'Validate config and admission'], cmd: [C.adminValidate],
      x: ['`--production` refuse la config de développement (adaptateur SD-WAN mock). SG1 démarre deux conducteurs réels et vérifie admission et refus. Reste ouvert : bootstrap/relais privés.', '`--production` rejects the development config (mock SD-WAN adapter). SG1 starts two real conductors and checks admission and refusals. Still open: private bootstrap/relay.'] },
    devTest: { t: ['Tests et porte SG1', 'Tests and SG1 gate'], cmd: [C.devTest],
      x: ['Ce que la CI exécute : tests natifs et admission à deux conducteurs.', 'What CI runs: native tests and two-conductor admission.'] },
    devTestnet: { t: ['Testnet 2 nœuds (mock)', 'Two-node testnet (mock)'], cmd: [C.devTestnet],
      x: ['Valide le flux de contrôle et le découpage des trames, pas l’équivalence numérique d’un vrai modèle.', 'Validates control flow and framing only, not numerical equivalence of a real model.'] }
  };

  var PATHS = {
    local: ['clone', 'rust', 'holochain', 'build', 'conductor', 'installApp', 'daemon', 'proxy'],
    docker: ['clone', 'rust', 'dockerPre', 'dockerUp', 'dockerCheck'],
    member: ['clone', 'rust', 'holochain', 'build', 'conductor', 'memberKey', 'memberVerify', 'memberRoles', 'memberInstall'],
    admin: ['clone', 'rust', 'holochain', 'adminKey', 'adminDna', 'adminProof', 'adminValidate'],
    dev: ['clone', 'rust', 'holochain', 'devTest', 'devTestnet']
  };

  // ── FAQ (mots-clés → réponse) ────────────────────────────────────────────
  var FAQ = [
    { k: ['inscription', 'compte', 'account', 'signup', 'sign up', 'permission', 'invitation'],
      a: ['Aucun compte en ligne : votre identité est une clé générée localement. Mais pour un **mesh privé**, il faut une preuve de membrane signée par l’administrateur du réseau. Le mode public, lui, est expérimental (développement uniquement).',
          'No online account: your identity is a locally generated key. A **private mesh** however requires a membrane proof signed by the network administrator. Public mode is experimental (development only).'] },
    { k: ['anonym', 'anonymat', 'privacy', 'tor', 'ip'],
      a: ['AInonymous ne fournit **pas** d’anonymat réseau : relais, endpoints QUIC, timing et tailles de transfert peuvent révéler des métadonnées. Voir [SECURITY.md](' + BLOB + 'docs/SECURITY.md). Un mode furtif (CH5) est défini mais pas implémenté.',
          'AInonymous does **not** provide network anonymity: relays, QUIC endpoints, timing and transfer sizes can reveal metadata. See [SECURITY.md](' + BLOB + 'docs/SECURITY.md). A stealth mode (CH5) is defined but not implemented.'] },
    { k: ['hash', 'installed_hash', 'epoch', 'reproduc'],
      a: ['Une recompilation locale change l’empreinte des DNA (chemins absolus embarqués dans le WASM). Pour un réseau épinglé, utilisez les bundles CI/release ou l’image reproductible : `docker build -f deploy/containers/repro/Dockerfile -t ainonymous-repro .` puis `docker run --rm ainonymous-repro`. Voir [DNA_HASHES.md](' + BLOB + 'docs/DNA_HASHES.md).',
          'A local rebuild changes the DNA hash (absolute paths embedded in the WASM). For a pinned network use CI/release bundles or the reproducible image: `docker build -f deploy/containers/repro/Dockerfile -t ainonymous-repro .` then `docker run --rm ainonymous-repro`. See [DNA_HASHES.md](' + BLOB + 'docs/DNA_HASHES.md).'] },
    { k: ['port', '8888', '8889', '8890', '9337', '9000', '65000', '18890'],
      a: ['Ports : admin Holochain 8888 (65000 en test), WebSocket app `ainonymous` 8889 (65001 en test), `hybridnode` 8891, API HTTP du daemon 8890 (18890 en test), QUIC 9000/UDP, proxy OpenAI 9337. Le proxy trouve le daemon via `AINON_DAEMON_URL` (défaut `http://127.0.0.1:8890`).',
          'Ports: Holochain admin 8888 (65000 in test), `ainonymous` app WebSocket 8889 (65001 in test), `hybridnode` 8891, daemon HTTP API 8890 (18890 in test), QUIC 9000/UDP, OpenAI proxy 9337. The proxy finds the daemon through `AINON_DAEMON_URL` (default `http://127.0.0.1:8890`).'] },
    { k: ['genesis', 'genèse', 'proof', 'preuve', 'membrane', 'refus', 'reject'],
      a: ['La preuve est un justificatif d’installation (genèse), jamais un champ d’appel de zome. Refus à la genèse : vérifiez la clé d’agent ciblée, l’identifiant réseau, la signature et les propriétés DNA. Outil : `cargo run -p private-network-proof -- verify …`. Voir [scripts/sg1](' + BLOB + 'scripts/sg1/README.md).',
          'The proof is an install-time (genesis) credential, never a zome-call field. Genesis rejected: check the target agent key, network id, signature and DNA properties. Tool: `cargo run -p private-network-proof -- verify …`. See [scripts/sg1](' + BLOB + 'scripts/sg1/README.md).'] },
    { k: ['windows', 'lnk1104', 'powershell', 'wsl'],
      a: ['Windows : `scripts/setup_holochain_windows.ps1` installe Holochain 0.7.0 (x86_64). Compilez les zomes avec un `CARGO_TARGET_DIR` court (`C:\\wz-ain`) pour éviter `LNK1104`. Les scripts Bash passent par WSL ou Git Bash. Les builds Windows donnent des hash DNA différents : installez les bundles CI.',
          'Windows: `scripts/setup_holochain_windows.ps1` installs Holochain 0.7.0 (x86_64). Build the zomes with a short `CARGO_TARGET_DIR` (`C:\\wz-ain`) to avoid `LNK1104`. Bash scripts run through WSL or Git Bash. Windows builds produce different DNA hashes: install the CI bundles.'] },
    { k: ['mac', 'macos', 'arm', 'aarch64', 'raspberry'],
      a: ['Le dépôt n’a pas de script d’installation Holochain pour macOS ou ARM. Installez `hc`/`holochain` 0.7.0 depuis les releases officielles Holochain puis suivez les mêmes étapes.',
          'The repository has no Holochain install script for macOS or ARM. Install `hc`/`holochain` 0.7.0 from the official Holochain releases, then follow the same steps.'] },
    { k: ['docker', 'compose', 'conteneur', 'container'],
      a: ['Pile Docker : `HC_BIN=/absolute/path/to/hc ./scripts/reprovision-containers.sh --confirm-reset` (destructif pour les volumes du projet). Seule l’API daemon `127.0.0.1:8890` est exposée. Voir [deploy/containers](' + BLOB + 'deploy/containers/README.md).',
          'Docker stack: `HC_BIN=/absolute/path/to/hc ./scripts/reprovision-containers.sh --confirm-reset` (destroys the project volumes). Only the daemon API `127.0.0.1:8890` is exposed. See [deploy/containers](' + BLOB + 'deploy/containers/README.md).'] },
    { k: ['revoc', 'révoc', 'ban', 'exclure', 'remove member'],
      a: ['La révocation en temps réel (deny list, arrêt du job < 5 s) est en brouillon dans la [PR #57](' + REPO + '/pull/57). Aujourd’hui, pour une révocation dure : changer d’identifiant réseau et réinstaller.',
          'Real-time revocation (deny list, job stop < 5 s) is a draft in [PR #57](' + REPO + '/pull/57). Today, hard revocation means rotating the network id and reinstalling.'] },
    { k: ['statut', 'status', 'état', 'roadmap', 'reste', 'remaining', 'mesh'],
      a: ['Fait : Holochain 0.7, admission privée testée avec deux conducteurs réels, mTLS strict, découpage 2 étages équivalent sur une machine. Reste : mesure sur plusieurs machines réelles (CH3), relais privés, révocation. Voir [ROADMAP.md](' + BLOB + 'ROADMAP.md).',
          'Done: Holochain 0.7, private admission tested with two real conductors, strict mTLS, 2-stage split equivalent on one machine. Remaining: measurement on several real machines (CH3), private relays, revocation. See [ROADMAP.md](' + BLOB + 'ROADMAP.md).'] },
    { k: ['gpu', 'cuda', 'vulkan', 'nvidia', 'perf', 'tok/s', 'vitesse', 'speed'],
      a: ['Aucune mesure GPU n’est encore publiée (CH4) et le calcul réparti n’a pas été mesuré sur plusieurs machines (CH3). Plus de nœuds ne garantit donc pas encore des inférences plus rapides.',
          'No GPU measurement is published yet (CH4) and distributed inference has not been measured across machines (CH3), so more nodes do not yet guarantee faster inference.'] },
    { k: ['modèle', 'model', 'gemma', 'llama', 'gguf'],
      a: ['Le daemon lance `llama-server` (champ `llama_server_bin`) avec les modèles de `models_dir`. Le découpage natif utilise le patch llama.cpp `patches/llama-cpp-pipeline-split/` (`make build-native-pipeline-server`).',
          'The daemon starts `llama-server` (`llama_server_bin`) with models from `models_dir`. Native splitting uses the llama.cpp patch in `patches/llama-cpp-pipeline-split/` (`make build-native-pipeline-server`).'] },
    { k: ['bootstrap', 'relay', 'relais', 'pair', 'peer', 'gossip'],
      a: ['Les configs de référence utilisent le bootstrap/relais publics de Holochain. Des services privés opérés par l’administrateur restent à mettre en place (porte SG1, moitié réseau).',
          'Reference configs use Holochain’s public bootstrap/relay. Private, operator-controlled services are still to be set up (SG1 gate, network half).'] }
  ];

  // ── Utilitaires DOM ──────────────────────────────────────────────────────
  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }
  // Mise en forme minimale et sûre : **gras**, `code`, [lien](https://…)
  function rich(target, text) {
    var re = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\(https:\/\/[^)\s]+\))/g;
    var last = 0, m;
    while ((m = re.exec(text)) !== null) {
      if (m.index > last) target.appendChild(document.createTextNode(text.slice(last, m.index)));
      var tok = m[0];
      if (tok.charAt(0) === '*') target.appendChild(el('strong', null, tok.slice(2, -2)));
      else if (tok.charAt(0) === '`') target.appendChild(el('code', null, tok.slice(1, -1)));
      else {
        var lab = tok.slice(1, tok.indexOf(']'));
        var href = tok.slice(tok.indexOf('(') + 1, -1);
        var a = el('a', null, lab); a.href = href; a.target = '_blank'; a.rel = 'noopener';
        target.appendChild(a);
      }
      last = m.index + tok.length;
    }
    if (last < text.length) target.appendChild(document.createTextNode(text.slice(last)));
    return target;
  }

  function SetupGuide(root) {
    this.root = root;
    this.lang = (document.documentElement.lang || 'fr').slice(0, 2) === 'en' ? 'en' : 'fr';
    this.L = T[this.lang];
    this.li = this.lang === 'en' ? 1 : 0;
    this.state = this.load() || { goal: null, os: null, hw: null, idx: 0, done: {} };
    this.build();
    this.restore();
  }

  SetupGuide.prototype.load = function () {
    try { return JSON.parse(window.localStorage.getItem(STORE_KEY)); } catch (e) { return null; }
  };
  SetupGuide.prototype.save = function () {
    try { window.localStorage.setItem(STORE_KEY, JSON.stringify(this.state)); } catch (e) { /* stockage indisponible */ }
  };

  SetupGuide.prototype.build = function () {
    var L = this.L, self = this;
    this.root.textContent = '';
    var head = el('div', 'sg-head');
    var avatar = el('div', 'sg-avatar', '✦');
    var ht = el('div');
    ht.appendChild(el('div', 'sg-title', L.title));
    ht.appendChild(el('div', 'sg-sub', L.subtitle));
    head.appendChild(avatar); head.appendChild(ht);
    var reset = el('button', 'sg-reset', L.reset);
    reset.type = 'button';
    reset.addEventListener('click', function () { self.reset(); });
    head.appendChild(reset);
    this.root.appendChild(head);

    var body = el('div', 'sg-body');
    var chatCol = el('div', 'sg-chat-col');
    this.log = el('div', 'sg-log');
    this.log.setAttribute('aria-live', 'polite');
    this.chips = el('div', 'sg-chips');
    var form = el('form', 'sg-form');
    this.input = el('input', 'sg-input');
    this.input.type = 'text';
    this.input.placeholder = L.placeholder;
    this.input.setAttribute('aria-label', L.placeholder);
    var send = el('button', 'sg-send', L.send); send.type = 'submit';
    form.appendChild(this.input); form.appendChild(send);
    form.addEventListener('submit', function (ev) {
      ev.preventDefault();
      var q = self.input.value.trim();
      if (!q) return;
      self.input.value = '';
      self.ask(q);
    });
    chatCol.appendChild(this.log);
    chatCol.appendChild(this.chips);
    chatCol.appendChild(form);

    // Mode LLM local optionnel
    var llm = el('details', 'sg-llm');
    llm.appendChild(el('summary', null, L.llmToggle));
    var lw = el('div', 'sg-llm-body');
    lw.appendChild(el('p', 'sg-llm-help', L.llmHelp));
    var row = el('label', 'sg-llm-row');
    this.llmOn = el('input'); this.llmOn.type = 'checkbox';
    row.appendChild(this.llmOn); row.appendChild(document.createTextNode(' ' + L.llmToggle));
    lw.appendChild(row);
    var r2 = el('label', 'sg-llm-row', L.endpoint + ' ');
    this.llmUrl = el('input', 'sg-input sg-small'); this.llmUrl.value = 'http://127.0.0.1:9337/v1';
    r2.appendChild(this.llmUrl); lw.appendChild(r2);
    var r3 = el('label', 'sg-llm-row', L.model + ' ');
    this.llmModel = el('input', 'sg-input sg-small'); this.llmModel.value = 'gemma4-e4b';
    r3.appendChild(this.llmModel); lw.appendChild(r3);
    llm.appendChild(lw);
    chatCol.appendChild(llm);

    var side = el('aside', 'sg-side');
    side.appendChild(el('div', 'sg-side-title', L.progress));
    this.bar = el('div', 'sg-bar'); this.barFill = el('div', 'sg-bar-fill'); this.bar.appendChild(this.barFill);
    side.appendChild(this.bar);
    this.pct = el('div', 'sg-pct', '0 %');
    side.appendChild(this.pct);
    this.list = el('ol', 'sg-steps');
    side.appendChild(this.list);

    body.appendChild(chatCol); body.appendChild(side);
    this.root.appendChild(body);
  };

  SetupGuide.prototype.say = function (text, who) {
    var msg = el('div', 'sg-msg ' + (who === 'user' ? 'sg-user' : 'sg-bot'));
    msg.appendChild(el('div', 'sg-who', who === 'user' ? this.L.you : this.L.bot));
    var p = el('div', 'sg-text');
    if (who === 'user') p.textContent = text; else rich(p, text);
    msg.appendChild(p);
    this.log.appendChild(msg);
    this.log.scrollTop = this.log.scrollHeight;
    return msg;
  };

  SetupGuide.prototype.code = function (msg, code) {
    var L = this.L;
    var wrap = el('div', 'sg-code');
    var pre = el('pre'); pre.textContent = code;
    var b = el('button', 'sg-copy', L.copy); b.type = 'button';
    b.addEventListener('click', function () {
      var done = function () { b.textContent = L.copied; setTimeout(function () { b.textContent = L.copy; }, 1500); };
      if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(code).then(done, done);
      else done();
    });
    wrap.appendChild(b); wrap.appendChild(pre);
    msg.appendChild(wrap);
    this.log.scrollTop = this.log.scrollHeight;
  };

  SetupGuide.prototype.setChips = function (items) {
    var self = this;
    this.chips.textContent = '';
    items.forEach(function (it) {
      var b = el('button', 'sg-chip', it.label); b.type = 'button';
      b.setAttribute('data-chip', it.id || '');
      b.addEventListener('click', function () {
        self.say(it.label, 'user');
        it.fn();
      });
      self.chips.appendChild(b);
    });
  };

  SetupGuide.prototype.steps = function () { return this.state.goal ? PATHS[this.state.goal] : []; };

  SetupGuide.prototype.render = function () {
    var self = this, steps = this.steps(), li = this.li;
    this.list.textContent = '';
    var n = 0;
    steps.forEach(function (id, i) {
      var item = el('li', 'sg-step' + (self.state.done[id] ? ' is-done' : '') + (i === self.state.idx ? ' is-current' : ''));
      var b = el('button', null, S[id].t[li]); b.type = 'button';
      b.addEventListener('click', function () { self.state.idx = i; self.save(); self.showStep(); });
      item.appendChild(b);
      self.list.appendChild(item);
      if (self.state.done[id]) n++;
    });
    var pct = steps.length ? Math.round(100 * n / steps.length) : 0;
    this.barFill.style.width = pct + '%';
    this.pct.textContent = pct + ' %' + (steps.length ? ' · ' + n + '/' + steps.length : '');
  };

  SetupGuide.prototype.restore = function () {
    if (this.state.goal && this.state.os) {
      this.say(this.L.plan.replace('{n}', this.steps().length));
      this.showStep();
    } else {
      this.start();
    }
  };

  SetupGuide.prototype.reset = function () {
    this.state = { goal: null, os: null, hw: null, idx: 0, done: {} };
    this.save();
    this.log.textContent = '';
    this.render();
    this.start();
  };

  SetupGuide.prototype.start = function () {
    var self = this, L = this.L;
    this.say(L.hello);
    this.setChips(Object.keys(PATHS).map(function (g) {
      return { id: 'goal-' + g, label: L.goals[g], fn: function () { self.state.goal = g; self.save(); self.askOs(); } };
    }));
    this.render();
  };

  SetupGuide.prototype.askOs = function () {
    var self = this, L = this.L;
    this.render();
    this.say(L.askOs);
    this.setChips(Object.keys(L.os).map(function (o) {
      return { id: 'os-' + o, label: L.os[o], fn: function () {
        self.state.os = o; self.save();
        if (o === 'mac' || o === 'other') self.say(L.osWarn);
        if (o === 'win') self.say(L.winNative);
        self.askHw();
      } };
    }));
  };

  SetupGuide.prototype.askHw = function () {
    var self = this, L = this.L;
    this.say(L.askHw);
    this.setChips(Object.keys(L.hw).map(function (h) {
      return { id: 'hw-' + h, label: L.hw[h], fn: function () {
        self.state.hw = h; self.state.idx = 0; self.save();
        self.say(L.hwNote);
        self.say(L.plan.replace('{n}', self.steps().length));
        self.showStep();
      } };
    }));
  };

  SetupGuide.prototype.cmdsFor = function (id) {
    var s = S[id], os = this.state.os;
    if (s.cmd === 'HC') return [os === 'win' ? C.hcWin : C.hcLinux];
    if (s.cmd === 'DOCKER') return [os === 'win' ? C.dockerPs : C.dockerSh];
    return s.cmd;
  };

  SetupGuide.prototype.showStep = function () {
    var self = this, L = this.L, li = this.li, steps = this.steps();
    this.render();
    if (this.state.idx >= steps.length) {
      this.say(L.finished);
      this.setChips([{ id: 'reset', label: L.reset, fn: function () { self.reset(); } }]);
      return;
    }
    var id = steps[this.state.idx], s = S[id];
    var msg = this.say('**' + L.stepWord + ' ' + (this.state.idx + 1) + '/' + steps.length + ' — ' + s.t[li] + '.** ' + s.x[li]);
    this.cmdsFor(id).forEach(function (c) { self.code(msg, c); });
    var chips = [
      { id: 'done', label: L.done, fn: function () { self.state.done[id] = true; self.state.idx++; self.save(); self.showStep(); } },
      { id: 'error', label: L.err, fn: function () { self.trouble(id); } },
      { id: 'skip', label: L.skip, fn: function () { self.state.idx++; self.save(); self.showStep(); } }
    ];
    if (this.state.idx > 0) chips.push({ id: 'back', label: L.back, fn: function () { self.state.idx--; self.save(); self.showStep(); } });
    this.setChips(chips);
  };

  SetupGuide.prototype.trouble = function (id) {
    var L = this.L, li = this.li, s = S[id];
    if (!s.k || !s.k.length) { this.say(L.noTrouble); return; }
    this.say(L.troubleIntro + '\n' + s.k.map(function (k) { return '• ' + k[li]; }).join('\n'));
  };

  SetupGuide.prototype.answer = function (q) {
    var low = q.toLowerCase(), best = null, score = 0, li = this.li;
    FAQ.forEach(function (f) {
      var sc = 0;
      f.k.forEach(function (k) { if (low.indexOf(k) !== -1) sc += k.length; });
      if (sc > score) { score = sc; best = f; }
    });
    return best ? best.a[li] : this.L.fallback;
  };

  SetupGuide.prototype.ask = function (q) {
    var self = this;
    this.say(q, 'user');
    if (!this.llmOn.checked) { this.say(this.answer(q)); return; }
    var url = this.llmUrl.value.replace(/\/+$/, '') + '/chat/completions';
    if (!/^https?:\/\/(127\.0\.0\.1|localhost|\[::1\])(:\d+)?\//.test(url)) {
      this.say(this.L.llmFail); this.say(this.answer(q)); return;
    }
    var steps = this.steps(), cur = steps[this.state.idx];
    var sys = (this.lang === 'en' ? 'You are the AInonymous setup guide. Answer briefly. Only use these repository facts; say when unsure. ' : 'Tu es le guide de mise en route AInonymous. Réponds brièvement en français. N’utilise que ces faits du dépôt ; dis-le si tu ne sais pas. ') +
      FAQ.map(function (f) { return f.a[self.li]; }).join(' ') +
      (cur ? ' Current step: ' + S[cur].t[1] + ' — ' + this.cmdsFor(cur).join(' ; ') : '');
    var ctrl = typeof AbortController !== 'undefined' ? new AbortController() : null;
    var timer = setTimeout(function () { if (ctrl) ctrl.abort(); }, 20000);
    var pending = this.say('…');
    fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model: this.llmModel.value, max_tokens: 400,
        messages: [{ role: 'system', content: sys }, { role: 'user', content: q }] }),
      signal: ctrl ? ctrl.signal : undefined
    }).then(function (r) {
      if (!r.ok) throw new Error('HTTP ' + r.status);
      return r.json();
    }).then(function (j) {
      clearTimeout(timer);
      var txt = j && j.choices && j.choices[0] && j.choices[0].message && j.choices[0].message.content;
      if (!txt) throw new Error('empty');
      pending.querySelector('.sg-text').textContent = txt;
    }).catch(function () {
      clearTimeout(timer);
      pending.remove();
      self.say(self.L.llmFail);
      self.say(self.answer(q));
    });
  };

  function init() {
    var root = document.getElementById('setup-guide');
    if (root) window.ainonymousSetupGuide = new SetupGuide(root);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
