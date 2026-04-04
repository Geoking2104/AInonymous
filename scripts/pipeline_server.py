#!/usr/bin/env python3
"""
AInonymous Pipeline Server
===========================
Serveur d'exécution par tranches de couches pour le pipeline-split.
Compatible Gemma 4 (dense + MoE) via HuggingFace transformers.

Chaque nœud lance ce serveur avec sa tranche de couches :
  - Nœud 0  : embed_tokens + couches [0..k[
  - Nœuds i  : couches [k..m[ (hidden states entrants/sortants)
  - Nœud -1  : couches [m..N] + norm + lm_head → tokens

Architecture KV-cache :
  - Le KV-cache REST EN MÉMOIRE GPU sur chaque nœud
  - Seuls les hidden states transitent sur le réseau (taille : seq × hidden)
  - Le coordinator suit quel request_id est en cours sur quel nœud

Usage :
  python pipeline_server.py \\
    --model google/gemma-4-e4b-it \\
    --port 9340 \\
    --layer-start 0 --layer-end 18 \\
    --is-first-node

Prérequis :
  pip install fastapi uvicorn transformers accelerate torch numpy
"""

import argparse
import asyncio
import base64
import gc
import logging
import struct
import sys
from contextlib import asynccontextmanager
from typing import Any, Dict, List, Optional

import numpy as np
import torch
import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from transformers import AutoModelForCausalLM, AutoTokenizer

logging.basicConfig(level=logging.INFO,
                    format="%(asctime)s [%(levelname)s] %(name)s — %(message)s")
logger = logging.getLogger("ainonymous.pipeline")

# ── État global du serveur ────────────────────────────────────────────────────
_cfg: Dict[str, Any] = {}   # config (layer_start, layer_end, is_first, is_last)
_model = None               # AutoModelForCausalLM (partiel, layers découpés)
_tokenizer = None
# KV-cache par request_id : dict[str, tuple[past_key_values...]]
_kv_caches: Dict[str, Any] = {}


# ── Modèles Pydantic ──────────────────────────────────────────────────────────

class PrefillRequest(BaseModel):
    """Requête de prefill (traitement du prompt complet)."""
    request_id: str
    # Premier nœud : liste d'IDs de tokens bruts
    input_ids:        Optional[List[int]] = None
    # Nœuds suivants : hidden states [1, seq_len, hidden_size] float16 LE en base64
    hidden_states_b64: Optional[str] = None
    seq_len:  int = 0
    hidden_size: int = 0

class PrefillResponse(BaseModel):
    request_id: str
    # hidden states sérialisés (nœuds intermédiaires + dernier nœud vers prochain)
    hidden_states_b64: Optional[str] = None
    seq_len: int
    hidden_size: int
    # Seulement si is_last_node : logits du dernier token → token généré
    next_token_id:   Optional[int] = None
    next_token_text: Optional[str] = None
    # Indique si ce nœud a produit le token final
    is_last_node: bool = False

class DecodeRequest(BaseModel):
    """Requête de décodage d'un token supplémentaire (utilise le KV-cache existant)."""
    request_id: str
    # Premier nœud : [next_token_id] comme input_ids (1 token)
    input_ids:        Optional[List[int]] = None
    hidden_states_b64: Optional[str] = None
    seq_len: int = 1
    hidden_size: int = 0

class DecodeResponse(BaseModel):
    request_id: str
    hidden_states_b64: Optional[str] = None
    seq_len: int
    hidden_size: int
    next_token_id:   Optional[int] = None
    next_token_text: Optional[str] = None
    is_last_node: bool = False

class ClearRequest(BaseModel):
    request_id: str

class StatusResponse(BaseModel):
    model_id: str
    layer_start: int
    layer_end: int
    total_layers: int
    is_first_node: bool
    is_last_node: bool
    active_requests: int
    device: str
    dtype: str


# ── Chargement du modèle (partiel) ────────────────────────────────────────────

def load_partial_model(model_id: str, layer_start: int, layer_end: int,
                       is_first: bool, is_last: bool, device: str, dtype_str: str):
    """
    Charge uniquement les couches nécessaires en mémoire GPU.
    Libère les couches hors tranche pour économiser la VRAM.
    """
    global _model, _tokenizer

    dtype = torch.bfloat16 if dtype_str == "bf16" else torch.float16
    logger.info("Chargement modèle %s (layers %d–%d) sur %s [%s]…",
                model_id, layer_start, layer_end, device, dtype_str)

    # Charger en CPU d'abord pour pouvoir supprimer les couches inutiles
    _tokenizer = AutoTokenizer.from_pretrained(model_id)
    _model = AutoModelForCausalLM.from_pretrained(
        model_id,
        torch_dtype=dtype,
        device_map="cpu",       # CPU d'abord
        low_cpu_mem_usage=True,
    )
    _model.eval()

    # Supprimer les couches hors de notre tranche pour libérer la mémoire
    decoder_layers = _model.model.layers
    total = len(decoder_layers)

    logger.info("Modèle chargé — %d couches total, notre tranche : [%d, %d[",
                total, layer_start, layer_end)

    # Remplacer les couches hors tranche par des None (garbage collectées)
    for i in range(total):
        if i < layer_start or i >= layer_end:
            decoder_layers[i] = None

    # Vider le garbage collector avant le transfert GPU
    gc.collect()
    if torch.cuda.is_available():
        torch.cuda.empty_cache()

    # Si premier nœud : garder embed_tokens
    if not is_first:
        _model.model.embed_tokens = None

    # Si pas dernier nœud : libérer norm + lm_head
    if not is_last:
        _model.model.norm = None
        _model.lm_head = None

    # Déplacer sur le device cible
    _model = _model.to(device)

    _cfg["total_layers"] = total
    logger.info("Modèle prêt sur %s (%.1f GB VRAM utilisée)",
                device, _vram_used_gb())


def _vram_used_gb() -> float:
    if torch.cuda.is_available():
        return torch.cuda.memory_allocated() / 1e9
    return 0.0


# ── Serialisation tenseurs ────────────────────────────────────────────────────

def tensor_to_b64(t: torch.Tensor) -> str:
    """Tensor → float16 LE bytes → base64."""
    arr = t.detach().to(dtype=torch.float16, device="cpu").numpy()
    return base64.b64encode(arr.tobytes()).decode()

def b64_to_tensor(b64: str, shape: tuple, device: str,
                  dtype=torch.float16) -> torch.Tensor:
    """base64 → numpy → tensor float16."""
    raw = base64.b64decode(b64)
    arr = np.frombuffer(raw, dtype=np.float16).reshape(shape)
    return torch.from_numpy(arr.copy()).to(dtype=dtype, device=device)


# ── Exécution partielle ───────────────────────────────────────────────────────

@torch.inference_mode()
def run_layers(
    hidden_states: torch.Tensor,
    attention_mask: Optional[torch.Tensor] = None,
    position_ids: Optional[torch.Tensor] = None,
    past_key_values=None,
    use_cache: bool = True,
) -> tuple:
    """
    Exécute les couches [layer_start, layer_end[ du modèle sur hidden_states.
    Retourne (output_hidden_states, new_past_key_values).
    """
    device = hidden_states.device
    batch, seq_len, _ = hidden_states.shape

    if position_ids is None:
        offset = 0
        if past_key_values is not None:
            # Cas decode : la position commence après les tokens déjà traités
            first_layer = _cfg["layer_start"]
            kv_layer = past_key_values[first_layer]
            if kv_layer is not None:
                offset = kv_layer[0].shape[2]  # seq_len dans le cache
        position_ids = torch.arange(offset, offset + seq_len,
                                    dtype=torch.long, device=device).unsqueeze(0)

    if attention_mask is None:
        total_len = position_ids[0, -1].item() + 1
        attention_mask = torch.ones(batch, total_len, device=device)

    new_kvs = {}
    h = hidden_states
    decoder_layers = _model.model.layers

    for i in range(_cfg["layer_start"], _cfg["layer_end"]):
        layer = decoder_layers[i]
        if layer is None:
            continue

        past_kv_i = None
        if past_key_values is not None and i in past_key_values:
            past_kv_i = past_key_values[i]

        layer_out = layer(
            hidden_states=h,
            attention_mask=attention_mask,
            position_ids=position_ids,
            past_key_value=past_kv_i,
            use_cache=use_cache,
            output_attentions=False,
        )

        # Les couches Gemma retournent (hidden_states, [present_kv], ...)
        h = layer_out[0]
        if use_cache and len(layer_out) > 1 and layer_out[1] is not None:
            new_kvs[i] = layer_out[1]

    return h, new_kvs


@torch.inference_mode()
def run_first_node(input_ids: List[int], use_cache: bool = True) -> tuple:
    """Embedding + couches [0, layer_end[."""
    device = next(_model.parameters()).device
    ids = torch.tensor([input_ids], dtype=torch.long, device=device)
    h = _model.model.embed_tokens(ids)
    hidden_states, kvs = run_layers(h, use_cache=use_cache)
    return hidden_states, kvs


@torch.inference_mode()
def run_last_node(hidden_states: torch.Tensor,
                  past_kv=None, use_cache=True) -> tuple:
    """Couches [layer_start, N] + norm + lm_head → token_id."""
    h, kvs = run_layers(hidden_states, past_key_values=past_kv,
                        use_cache=use_cache)
    h = _model.model.norm(h)
    logits = _model.lm_head(h)
    # Greedy : token avec le plus haut logit sur le dernier token
    next_id = int(logits[0, -1, :].argmax().item())
    return next_id, h, kvs


# ── Lifespan FastAPI ──────────────────────────────────────────────────────────

@asynccontextmanager
async def lifespan(app: FastAPI):
    args = _cfg["args"]
    load_partial_model(
        model_id=args.model,
        layer_start=args.layer_start,
        layer_end=args.layer_end,
        is_first=args.is_first_node,
        is_last=args.is_last_node,
        device=args.device,
        dtype_str=args.dtype,
    )
    logger.info("Pipeline server prêt — port %d", args.port)
    yield
    logger.info("Arrêt pipeline server")


app = FastAPI(title="AInonymous Pipeline Server", lifespan=lifespan)


# ── Endpoints ─────────────────────────────────────────────────────────────────

@app.get("/status", response_model=StatusResponse)
async def status():
    return StatusResponse(
        model_id=_cfg["args"].model,
        layer_start=_cfg["args"].layer_start,
        layer_end=_cfg["args"].layer_end,
        total_layers=_cfg.get("total_layers", 0),
        is_first_node=_cfg["args"].is_first_node,
        is_last_node=_cfg["args"].is_last_node,
        active_requests=len(_kv_caches),
        device=_cfg["args"].device,
        dtype=_cfg["args"].dtype,
    )


@app.post("/prefill", response_model=PrefillResponse)
async def prefill(req: PrefillRequest):
    """
    Traite le prompt complet (prefill pass).
    - Premier nœud  : accepte input_ids (liste d'entiers)
    - Autres nœuds  : accepte hidden_states_b64
    Retourne les hidden states du dernier token de la tranche, et pour le
    dernier nœud, le premier token généré.
    """
    device = next(_model.parameters()).device
    is_first = _cfg["args"].is_first_node
    is_last  = _cfg["args"].is_last_node

    try:
        if is_first:
            if not req.input_ids:
                raise HTTPException(400, "input_ids requis pour le premier nœud")
            h, kvs = run_first_node(req.input_ids, use_cache=True)
        else:
            if not req.hidden_states_b64:
                raise HTTPException(400, "hidden_states_b64 requis pour les nœuds suivants")
            h = b64_to_tensor(
                req.hidden_states_b64,
                shape=(1, req.seq_len, req.hidden_size),
                device=device,
                dtype=torch.bfloat16 if _cfg["args"].dtype == "bf16" else torch.float16,
            )
            h, kvs = run_layers(h, use_cache=True)

        # Sauvegarder le KV-cache pour les passes de décodage suivantes
        _kv_caches[req.request_id] = kvs

        if is_last:
            next_id, _, _ = run_last_node(h, past_kv=None, use_cache=False)
            next_text = _tokenizer.decode([next_id], skip_special_tokens=True)
            return PrefillResponse(
                request_id=req.request_id,
                seq_len=h.shape[1],
                hidden_size=h.shape[2],
                next_token_id=next_id,
                next_token_text=next_text,
                is_last_node=True,
            )
        else:
            return PrefillResponse(
                request_id=req.request_id,
                hidden_states_b64=tensor_to_b64(h),
                seq_len=h.shape[1],
                hidden_size=h.shape[2],
                is_last_node=False,
            )

    except HTTPException:
        raise
    except Exception as e:
        logger.exception("Erreur prefill %s", req.request_id)
        raise HTTPException(500, str(e))


@app.post("/decode", response_model=DecodeResponse)
async def decode(req: DecodeRequest):
    """
    Génère un token supplémentaire en utilisant le KV-cache existant.
    Appelé en boucle par le coordinator pour chaque token généré.
    """
    if req.request_id not in _kv_caches:
        raise HTTPException(404, f"request_id inconnu: {req.request_id} (prefill non effectué?)")

    device = next(_model.parameters()).device
    is_first = _cfg["args"].is_first_node
    is_last  = _cfg["args"].is_last_node
    past_kv  = _kv_caches[req.request_id]

    try:
        if is_first:
            if not req.input_ids:
                raise HTTPException(400, "input_ids requis (1 token)")
            ids = torch.tensor([req.input_ids[-1:]], dtype=torch.long, device=device)
            h = _model.model.embed_tokens(ids)   # [1, 1, hidden]
            h, new_kvs = run_layers(h, past_key_values=past_kv, use_cache=True)
        else:
            if not req.hidden_states_b64:
                raise HTTPException(400, "hidden_states_b64 requis")
            h = b64_to_tensor(
                req.hidden_states_b64,
                shape=(1, 1, req.hidden_size),
                device=device,
                dtype=torch.bfloat16 if _cfg["args"].dtype == "bf16" else torch.float16,
            )
            h, new_kvs = run_layers(h, past_key_values=past_kv, use_cache=True)

        # Mettre à jour le KV-cache (fusion avec l'existant)
        for k, v in new_kvs.items():
            _kv_caches[req.request_id][k] = v

        if is_last:
            next_id, _, _ = run_last_node(h, past_kv=None, use_cache=False)
            next_text = _tokenizer.decode([next_id], skip_special_tokens=True)
            return DecodeResponse(
                request_id=req.request_id,
                seq_len=1,
                hidden_size=h.shape[2],
                next_token_id=next_id,
                next_token_text=next_text,
                is_last_node=True,
            )
        else:
            return DecodeResponse(
                request_id=req.request_id,
                hidden_states_b64=tensor_to_b64(h),
                seq_len=1,
                hidden_size=h.shape[2],
                is_last_node=False,
            )

    except HTTPException:
        raise
    except Exception as e:
        logger.exception("Erreur decode %s", req.request_id)
        raise HTTPException(500, str(e))


@app.post("/clear")
async def clear(req: ClearRequest):
    """Libère le KV-cache d'une requête terminée."""
    _kv_caches.pop(req.request_id, None)
    gc.collect()
    if torch.cuda.is_available():
        torch.cuda.empty_cache()
    return {"cleared": req.request_id}


# ── Entrée CLI ────────────────────────────────────────────────────────────────

def parse_args():
    p = argparse.ArgumentParser(description="AInonymous Pipeline Server")
    p.add_argument("--model", required=True,
                   help="HuggingFace model ID, ex: google/gemma-4-e4b-it")
    p.add_argument("--port", type=int, default=9340)
    p.add_argument("--layer-start", type=int, default=0)
    p.add_argument("--layer-end",   type=int, default=18)
    p.add_argument("--is-first-node", action="store_true",
                   help="Ce nœud est le premier (exécute l'embedding)")
    p.add_argument("--is-last-node", action="store_true",
                   help="Ce nœud est le dernier (retourne des tokens, pas des activations)")
    p.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    p.add_argument("--dtype", choices=["fp16", "bf16"], default="bf16")
    p.add_argument("--host", default="0.0.0.0")
    return p.parse_args()


if __name__ == "__main__":
    args = parse_args()

    if args.is_first_node and args.layer_start != 0:
        logger.warning("is-first-node actif mais layer-start=%d (attendu 0)", args.layer_start)

    _cfg["args"] = args

    logger.info(
        "Démarrage — modèle: %s | couches: [%d, %d[ | premier: %s | dernier: %s | %s",
        args.model, args.layer_start, args.layer_end,
        args.is_first_node, args.is_last_node, args.device,
    )

    uvicorn.run(app, host=args.host, port=args.port, log_level="info")
