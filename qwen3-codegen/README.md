# qwen3-codegen — démonstrateur de génération de code

Petite application web autonome : un backend Express qui relaie un modèle Qwen3
(Ollama local par défaut, ou toute API compatible OpenAI) vers un frontend
React + Monaco avec streaming de la réponse.

> **Statut : démonstrateur.** Cette application n'est reliée ni au maillage
> Holochain, ni au data plane QUIC, ni à aucun crate du workspace Rust. Elle ne
> fait l'objet d'aucun test automatisé et n'est pas maintenue comme un composant
> du produit ; elle illustre une intégration Qwen3 côté application.

## Architecture

```text
qwen3-codegen/
  backend/    Express (port 3001) — /api/*, streaming SSE vers le frontend
  frontend/   Vite + React 18 + Monaco — éditeur et chat de génération de code
```

Le backend expose un endpoint de chat qui diffuse la réponse token par token
(SSE). Deux modes, choisis par variables d'environnement :

- **Ollama local** (défaut) : `OLLAMA_BASE_URL` + `MODEL`, pas de clé API ;
- **API compatible OpenAI** : définir `API_BASE_URL` (+ `API_KEY` si requis) et
  `MODEL`.

## Lancer

```bash
# Backend
cd qwen3-codegen/backend
cp .env.example .env        # ajuster MODEL / OLLAMA_BASE_URL
npm install
npm start                   # http://localhost:3001

# Frontend (autre terminal)
cd qwen3-codegen/frontend
npm install
npm run dev                 # Vite, proxy vers le backend
```

Prérequis : Node.js ≥ 18 et, en mode local, un serveur Ollama avec le modèle
Qwen3 tiré (`ollama pull qwen3:32b` par exemple).

## Limites assumées

- Aucune authentification : le backend est prévu pour un usage local.
- Aucune mesure de performance ni benchmark n'est fourni ici.
- Le prompt système est orienté génération de code ; l'application ne participe
  pas au routage, à la négociation de session ni à la vérification mTLS du
  maillage.
