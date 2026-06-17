"""
pipeline.py — Pipeline-split a PyTorch transformer across HybridNode peers.

Inspired by torch.distributed.pipeline.sync.Pipe but uses:
  - HybridNode QUIC/mTLS for inter-peer activation transfer (no NCCL/RDMA)
  - Holochain DHT for peer discovery and capability lookup
  - SD-WAN topology for latency-aware peer selection

Architecture::

    ┌───────────────┐    QUIC/mTLS      ┌───────────────┐
    │  Local node   │  activation blob  │  Remote peer  │
    │  layers [0,N) │ ─────────────────▶│  layers [N,M) │
    │  GPU A        │    ed25519 auth   │  GPU B        │
    └───────────────┘                   └───────────────┘

Usage::

    from ainonymous_torch import PipelineSplit, LayerRange
    from transformers import AutoModelForCausalLM

    model = AutoModelForCausalLM.from_pretrained("google/gemma-2-9b")

    pipe = PipelineSplit(
        local_layers=LayerRange(model.model.layers, start=0, end=18),
        remote_url="http://peer-b:9337",
        remote_layers=LayerRange(start=18, end=36),
    )
    output = pipe.forward(input_ids, attention_mask)
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import List, Optional, Tuple

import torch
import torch.nn as nn
import requests

from .activation_transfer import ActivationTransfer, pack_activation, unpack_activation

logger = logging.getLogger(__name__)


@dataclass
class LayerRange:
    """Describes which transformer layers a node is responsible for."""
    layers: Optional[nn.ModuleList] = None  # None on the remote side
    start: int = 0
    end: int = 0

    @property
    def count(self) -> int:
        return self.end - self.start


class PipelineSplit(nn.Module):
    """
    Split a transformer model across two HybridNode peers.

    The local node runs layers [local.start, local.end) and sends the
    resulting hidden state to the remote peer which runs [remote.start, remote.end).

    The remote peer must be running the AInonymous daemon with the complementary
    layers loaded and the /v1/pipeline/forward endpoint enabled.

    Args:
        local_layers:   LayerRange with the nn.ModuleList of local layers.
        remote_url:     Base URL of the remote HybridNode peer.
        remote_layers:  LayerRange describing which layers the remote runs.
        device:         Local device ("cpu", "cuda:0", …).
        transfer_dtype: Cast activations to this dtype before transfer (saves bandwidth).

    Example::

        pipe = PipelineSplit(
            local_layers=LayerRange(model.model.layers[:18], 0, 18),
            remote_url="http://192.168.1.42:9337",
            remote_layers=LayerRange(start=18, end=36),
            transfer_dtype=torch.float16,  # bfloat16 → float16 for smaller payload
        )
        logits = pipe(input_ids)
    """

    def __init__(
        self,
        local_layers: LayerRange,
        remote_url: str,
        remote_layers: LayerRange,
        device: str = "cpu",
        transfer_dtype: Optional[torch.dtype] = None,
        max_chunk_mb: int = 256,
        timeout_s: int = 30,
    ):
        super().__init__()
        self.local_layers = local_layers
        self.remote_url = remote_url.rstrip("/")
        self.remote_layers = remote_layers
        self.device = device
        self.transfer_dtype = transfer_dtype
        self.transfer = ActivationTransfer(max_chunk_mb=max_chunk_mb)
        self.timeout_s = timeout_s

        if local_layers.layers is not None:
            self.layers = local_layers.layers

    def forward(
        self,
        hidden_states: torch.Tensor,
        attention_mask: Optional[torch.Tensor] = None,
        **kwargs,
    ) -> torch.Tensor:
        """
        Run local layers, transfer activation, receive remote output.

        Args:
            hidden_states:   Input tensor [batch, seq_len, hidden_dim]
            attention_mask:  Optional attention mask

        Returns:
            Output tensor from the final remote layer.
        """
        # 1. Local forward pass
        h = hidden_states.to(self.device)
        for layer in self.layers:
            layer_out = layer(h, attention_mask=attention_mask, **kwargs)
            h = layer_out[0] if isinstance(layer_out, tuple) else layer_out

        logger.debug(
            "Local layers [%d,%d) done — shape=%s dtype=%s",
            self.local_layers.start, self.local_layers.end, h.shape, h.dtype,
        )

        # 2. Serialize activation for QUIC transfer
        send_tensor = h.to(self.transfer_dtype) if self.transfer_dtype else h
        payload = self.transfer.pack(send_tensor.cpu())

        # 3. Send to remote peer via HybridNode pipeline endpoint
        resp = requests.post(
            f"{self.remote_url}/v1/pipeline/forward",
            data=payload,
            headers={
                "Content-Type": "application/octet-stream",
                "X-AInonymous-Layer-Start": str(self.remote_layers.start),
                "X-AInonymous-Layer-End":   str(self.remote_layers.end),
                "X-AInonymous-Seq-Len":     str(h.shape[1]),
            },
            timeout=self.timeout_s,
        )
        resp.raise_for_status()

        # 4. Deserialize remote output
        remote_hidden = self.transfer.unpack(resp.content, device=self.device)

        logger.debug(
            "Remote layers [%d,%d) done — transfer stats: %s",
            self.remote_layers.start, self.remote_layers.end, self.transfer.stats,
        )
        return remote_hidden

    @property
    def transfer_stats(self) -> dict:
        return self.transfer.stats


class MultiNodePipeline(nn.Module):
    """
    Chain N nodes into a pipeline. Each node handles a contiguous range of layers.

    Example::

        nodes = [
            PipelineSplit(local_0_12, "http://node-b:9337", remote_12_24),
            PipelineSplit(local_12_24, "http://node-c:9337", remote_24_36),
        ]
        pipeline = MultiNodePipeline(nodes, final_lm_head=model.lm_head)
        logits = pipeline(input_ids, attention_mask)
    """

    def __init__(self, stages: List[PipelineSplit], final_lm_head: Optional[nn.Module] = None):
        super().__init__()
        self.stages = nn.ModuleList(stages)
        self.final_lm_head = final_lm_head

    def forward(self, hidden_states: torch.Tensor, attention_mask: Optional[torch.Tensor] = None) -> torch.Tensor:
        h = hidden_states
        for stage in self.stages:
            h = stage(h, attention_mask=attention_mask)
        if self.final_lm_head is not None:
            h = self.final_lm_head(h)
        return h
