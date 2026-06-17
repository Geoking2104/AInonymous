"""
distributed_inference.py — Wrap any PyTorch nn.Module for HybridNode routing.

DistributedInferenceModule is a drop-in wrapper:
  - If a suitable remote peer is available (held model + SLA OK), the forward
    pass is offloaded via the HybridNode API.
  - If no peer is found (or latency budget exceeded), falls back to local execution.

This is the simplest integration point — no layer splitting required.
The full model must fit on each node (or be run via llama.cpp GGUF).

Usage::

    from ainonymous_torch import DistributedInferenceModule

    # Wrap a local model
    local_model = AutoModelForCausalLM.from_pretrained("google/gemma-2-2b")
    dist_model = DistributedInferenceModule(
        local_model,
        model_name="gemma4-e2b",
        ainonymous_url="http://localhost:9337",
        latency_budget_ms=50,
    )
    # Forward call: tries HybridNode first, falls back to local
    logits = dist_model(input_ids)
"""

from __future__ import annotations

import logging
from typing import Optional

import torch
import torch.nn as nn
import requests

logger = logging.getLogger(__name__)


class DistributedInferenceModule(nn.Module):
    """
    Transparent wrapper that routes forward() through HybridNode when possible.

    Args:
        local_module:       The local nn.Module to fall back to.
        model_name:         Model name as registered in HybridNode DHT (for peer selection).
        ainonymous_url:     Base URL of the local HybridNode daemon.
        latency_budget_ms:  Skip remote if estimated peer latency exceeds this value.
        prefer_remote:      Always attempt remote before local (default: False).
        device:             Device for local fallback execution.

    The wrapper adds forward_source to the output dict indicating whether
    the result came from "local" or "remote:<peer_id>".
    """

    def __init__(
        self,
        local_module: nn.Module,
        model_name: str,
        ainonymous_url: str = "http://localhost:9337",
        latency_budget_ms: int = 100,
        prefer_remote: bool = False,
        device: str = "cpu",
    ):
        super().__init__()
        self._local = local_module
        self.model_name = model_name
        self.ainonymous_url = ainonymous_url.rstrip("/")
        self.latency_budget_ms = latency_budget_ms
        self.prefer_remote = prefer_remote
        self.device = device
        self._remote_calls: int = 0
        self._local_calls: int = 0

    def forward(self, input_ids: torch.Tensor, **kwargs) -> torch.Tensor:
        """
        Route inference:
          1. Query HybridNode scheduler for a suitable peer.
          2. If found and latency OK → remote inference via REST.
          3. Otherwise → local nn.Module forward.
        """
        peer = self._select_peer()
        if peer and (self.prefer_remote or not self._local_available()):
            try:
                result = self._remote_forward(peer, input_ids, **kwargs)
                self._remote_calls += 1
                return result
            except Exception as exc:
                logger.warning("Remote inference failed (%s), falling back to local", exc)

        # Local fallback
        self._local_calls += 1
        return self._local(input_ids.to(self.device), **kwargs)

    def _select_peer(self) -> Optional[str]:
        """Ask the local HybridNode scheduler for a peer URL."""
        try:
            resp = requests.post(
                f"{self.ainonymous_url}/v1/schedule",
                json={
                    "model_name": self.model_name,
                    "latency_budget_ms": self.latency_budget_ms,
                    "strategy": "local_first",
                },
                timeout=2,
            )
            if resp.ok:
                data = resp.json()
                return data.get("peer_url")  # e.g. "http://192.168.1.10:9337"
        except requests.exceptions.ConnectionError:
            logger.debug("HybridNode daemon not reachable at %s", self.ainonymous_url)
        return None

    def _remote_forward(self, peer_url: str, input_ids: torch.Tensor, **kwargs) -> torch.Tensor:
        """Send token IDs to a remote peer and get logits back."""
        # Encode input_ids as a simple list (small, no need for binary format)
        payload = {
            "model": self.model_name,
            "input_ids": input_ids.tolist(),
            "kwargs": {k: v.tolist() if isinstance(v, torch.Tensor) else v for k, v in kwargs.items()},
        }
        resp = requests.post(
            f"{peer_url}/v1/torch/forward",
            json=payload,
            timeout=30,
        )
        resp.raise_for_status()
        data = resp.json()
        logits = torch.tensor(data["logits"], dtype=torch.float32)
        logger.debug("Remote forward from %s — shape=%s", peer_url, logits.shape)
        return logits

    def _local_available(self) -> bool:
        """True if the local model has parameters (not a stub)."""
        try:
            return next(self._local.parameters()) is not None
        except StopIteration:
            return False

    @property
    def stats(self) -> dict:
        total = self._remote_calls + self._local_calls
        return {
            "remote_calls": self._remote_calls,
            "local_calls": self._local_calls,
            "remote_ratio": self._remote_calls / total if total else 0.0,
        }
