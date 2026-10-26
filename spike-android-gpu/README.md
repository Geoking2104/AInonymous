# Spike S2 — llama.cpp GPU (Vulkan) sur Android (#28)

**Statut : outillage uniquement, aucune mesure.** Ce spike demande un téléphone
réel (GPU Adreno ou Mali et pilote Vulkan du constructeur). Un émulateur ne
convient pas : son GPU est virtualisé et sa thermique n'est pas réaliste. Aucun
chiffre n'est donc publié ici.

## Questions (plan §3, semaines 2 et 3)

1. llama.cpp Vulkan compile-t-il pour `arm64-v8a`, et le `.so` se charge-t-il depuis l'app ?
2. Un modèle 1B Q4 génère-t-il des tokens sur GPU ? Un 3B Q4 aussi ?
3. Quel débit (tok/s) obtient-on, comment évolue-t-il sur 10 min de charge continue
   (thermique, batterie), et quel est le repli CPU ?

## Outils

| Fichier | Rôle |
|---|---|
| `build_llama_android.sh [vulkan\|cpu]` | compilation croisée NDK 27 (API 28), même commit llama.cpp que le patch pipeline-split |
| `bench_on_device.sh <build> <modèle.gguf>` | `adb push`, puis `llama-bench` GPU (`-ngl 99`) et CPU (`-ngl 0`), et 10 min de charge avec relevé des températures et de la batterie |

## Protocole de mesure (à suivre sur l'appareil)

1. Téléphone chargé à 100 %, débranché, écran allumé à luminosité minimale,
   température ambiante notée. Mode avion **désactivé**.
2. `bench_on_device.sh out/vulkan gemma-3-1b-it-Q4_K_M.gguf 1b-vulkan`, puis la même chose avec `out/cpu`.
3. Recommencer avec un 3B Q4_K_M. Si le chargement échoue (OOM), noter la RAM
   libre (`/proc/meminfo`).
4. Remplir la matrice ci-dessous. **Seules les valeurs mesurées sont reportées.**

| Appareil / SoC | Modèle | Backend | pp512 tok/s | tg128 tok/s | tg après 10 min | T max °C | Batterie −% / 10 min | Remarques |
|---|---|---|---|---|---|---|---|---|
| _à mesurer_ | 1B Q4_K_M | Vulkan | | | | | | |
| _à mesurer_ | 1B Q4_K_M | CPU | | | | | | |
| _à mesurer_ | 3B Q4_K_M | Vulkan | | | | | | |

## Risques connus à vérifier

- Les pilotes Vulkan mobiles sont hétérogènes. Sur certains Adreno ou Mali, le
  backend Vulkan de llama.cpp est plus lent que le CPU, voire échoue
  (extensions fp16/int8 absentes). Le repli CPU fait donc partie du livrable.
- Le throttling thermique peut faire chuter le débit soutenu bien en dessous du
  débit d'un test court. C'est pour cela que la mesure dure 10 min.
