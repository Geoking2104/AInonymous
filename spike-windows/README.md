# Spike S3 — GPU Windows et installeur per-user (#29)

**Statut : outillage uniquement, aucune mesure.** Il faut la machine Windows à
GPU de Geoffroy : la box de développement est sous Linux, sans GPU ni MSVC.

## Portes visées

- **G2** : CUDA **et** Vulkan exécutent un 7B Q4. La détection du GPU est
  correcte. Débit ≥ 80 % de la baseline llama.cpp.
- **G3** : installeur per-user signé, installation et désinstallation < 5 min,
  aucune élévation pour l'usage local.

## Outils

| Fichier | Rôle |
|---|---|
| `detect_gpu.ps1` | inventaire GPU : WMI/DXGI (VRAM réelle via le registre), `nvidia-smi` (NVML), `vulkaninfo` |
| `build_llama_windows.ps1 -Variant cuda\|vulkan\|cpu` | compile llama.cpp au même commit que le patch pipeline-split |
| `bench.ps1 -Model … -Baseline …` | `llama-bench` pp512/tg128 (5 répétitions) : candidat vs binaire officiel, pour chaque backend |
| `installer_check.ps1 -Installer …` | session non admin, signature Authenticode, durée d'installation et de désinstallation silencieuses, dossier `%LOCALAPPDATA%` |

## Protocole

1. `detect_gpu.ps1 > results\gpu.json` : vérifier que le GPU attendu et sa VRAM sont détectés.
2. Compiler les variantes `cuda` et `vulkan`. Télécharger le binaire officiel
   correspondant comme baseline.
3. Lancer `bench.ps1` avec un 7B Q4_K_M (même fichier GGUF pour tous les
   backends), machine branchée sur secteur, en profil « performances ».
4. Installeur : `npx tauri build` du shell Relay avec
   `bundle.windows.nsis.installMode = "currentUser"`, signé avec un certificat de
   test (`signtool sign /fd sha256 …`), puis `installer_check.ps1` depuis un
   compte standard.
5. Croiser avec S5 : produire le manifeste de release de l'installeur avec
   `relay-release manifest --platform windows-x86_64 …` et vérifier qu'un
   installeur altéré est refusé (même scénario que `spike-android/verify_apk_chain.sh`).

| Machine / GPU | Backend | Modèle | pp512 tok/s | tg128 tok/s | Baseline tg128 | Ratio | OK G2 |
|---|---|---|---|---|---|---|---|
| _à mesurer_ | CUDA | 7B Q4_K_M | | | | | |
| _à mesurer_ | Vulkan | 7B Q4_K_M | | | | | |
