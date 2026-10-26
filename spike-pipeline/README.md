# Spike S4 (partie calcul) — équivalence numérique par plage de couches sur 2 étages (#30)

Ce dossier contient un harnais reproductible **sur CPU, sans GPU** pour le patch
`patches/llama-cpp-pipeline-split` (llama.cpp épinglé au commit
`42fc243060709331ff9b158a9ed2cbe37219ae83`). Il compare l'exécution d'un nœud
unique et une exécution découpée en deux étages : couches `[0, 1)` puis `[1, 2)`.

```bash
WORKDIR=/tmp/llama.cpp PY=/chemin/venv/bin/python bash spike-pipeline/run_equivalence.sh
```

Le script :

1. compile llama.cpp patché et `pipeline_server` (`build_native.sh`) ;
2. convertit en GGUF F32 le modèle `katuni4ka/tiny-random-gemma3-text`
   (Gemma3ForCausalLM, 2 couches, hidden 8, vocabulaire 262 144, poids aléatoires,
   GGUF de 14,9 Mo, sha256 dans `results/model.sha256`) ;
3. lance les trois tests en processus du patch (`pipeline_test*.cpp`) ;
4. lance 3 processus `pipeline_server` (nœud unique, node0, node1). Il génère
   ensuite 16 tokens en mode glouton et compare les deux chemins. L'état caché
   passe d'un processus à l'autre en HTTP (F32 base64).

## Résultats réels (box de dev, 27/10/2026)

| Test | Résultat |
|---|---|
| `pipeline_test` (prefill, logits) | argmax identique (153824), **écart max des logits = 0**, L2 relatif = 0 |
| `pipeline_test2` (prefill + 1 decode, KV cache) | 2 tokens identiques, écart max = 0 aux deux étapes : PASS |
| `pipeline_test3_earlyexit` | PASS (pas de crash), tokens 153824, 115854 |
| Chaîne HTTP 2 processus, 16 tokens | **identique** au nœud unique (aucune divergence) |

Les journaux et le JSON se trouvent dans `results/`.

## Limites : ce que ce résultat ne prouve pas

- Le modèle a des poids **aléatoires**, 2 couches et hidden 8. Le résultat prouve
  que la coupure est correcte (bit à bit sur CPU F32 dans le même binaire), pas
  la qualité ni la performance.
- Il n'y a ni GPU, ni quantification Q4, ni backends hétérogènes (CUDA d'un côté,
  Vulkan/OpenCL de l'autre) : sur du matériel réel, un écart flottant non nul est
  attendu. Le critère du plan est « identique **ou écart documenté** ».
- Le transport utilisé ici est le HTTP du PoC, pas QUIC. L'intégration de
  `pipeline_server` derrière `ainonymous-quic` (`spike-quic`) reste à faire.
- TTFT < 2 s et ≥ 15 tok/s (30B Q4, 2 nœuds) **ne peuvent pas être mesurés ici** :
  il faut le PC GPU de Geoffroy plus un second nœud sur le LAN.

## Protocole sur le matériel réel

1. Compiler `pipeline_server` avec le backend GPU de chaque nœud.
2. Prendre le même GGUF (Gemma 3, Q4_K_M) sur les deux nœuds. Lancer node0
   (`--layer-start 0 --layer-end N/2 --is-first-node`) sur le nœud A et node1
   (`--layer-start N/2 --is-last-node`) sur le nœud B, puis une référence en nœud
   unique sur A.
3. Lancer `two_stage_driver.py --single http://A:9100 --node0 http://A:9101 --node1 http://B:9102 --tokens 64`
   sur 5 prompts. Noter `identical` et `first_divergence`, ainsi que le TTFT et
   les tok/s mesurés de bout en bout.
