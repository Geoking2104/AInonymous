"""
AInonymousClient — HTTP client for the HybridNode OpenAI-compatible API.

The HybridNode daemon exposes an OpenAI-compatible endpoint at:
    http://localhost:9337/v1

This client wraps it and adds:
  - Peer status polling
  - Model routing hints (preferred_node, layer_range)
  - Streaming support
"""

from __future__ import annotations

import json
import time
from dataclasses import dataclass, field
from typing import Generator, List, Optional

import requests


@dataclass
class ChatMessage:
    role: str   # "user" | "assistant" | "system"
    content: str


@dataclass
class InferenceOptions:
    """Extra routing hints passed via HybridNode extension headers."""
    preferred_node: Optional[str] = None     # AgentPubKey hex of preferred peer
    max_latency_ms: Optional[int] = None     # Reject peers above this RTT
    redundancy_mode: Optional[str] = None    # "none"|"primary_shadow"|"n_of_m_quorum"
    require_attestation: bool = True         # Only use attested nodes


@dataclass
class NodeStatus:
    agent_pub_key: str
    site_id: str
    vram_mb: int
    reputation: float
    held_models: List[str]
    latency_ms: Optional[float] = None
    has_warrant: bool = False


class AInonymousClient:
    """
    Thin client for the AInonymous HybridNode inference API.

    Example::

        client = AInonymousClient()
        reply = client.chat("Explique le pipeline-splitting en 3 lignes.")
        print(reply)
    """

    def __init__(
        self,
        base_url: str = "http://localhost:9337",
        model: str = "gemma4-31b",
        timeout: int = 120,
    ):
        self.base_url = base_url.rstrip("/")
        self.default_model = model
        self.timeout = timeout
        self._session = requests.Session()
        self._session.headers.update({"Content-Type": "application/json"})

    # ------------------------------------------------------------------
    # Chat
    # ------------------------------------------------------------------

    def chat(
        self,
        prompt: str,
        system: Optional[str] = None,
        model: Optional[str] = None,
        max_tokens: int = 512,
        temperature: float = 0.7,
        options: Optional[InferenceOptions] = None,
    ) -> str:
        """Single-turn chat. Returns the assistant reply as a string."""
        messages = []
        if system:
            messages.append({"role": "system", "content": system})
        messages.append({"role": "user", "content": prompt})

        body = {
            "model": model or self.default_model,
            "messages": messages,
            "max_tokens": max_tokens,
            "temperature": temperature,
        }
        headers = self._routing_headers(options)

        resp = self._session.post(
            f"{self.base_url}/v1/chat/completions",
            json=body,
            headers=headers,
            timeout=self.timeout,
        )
        resp.raise_for_status()
        return resp.json()["choices"][0]["message"]["content"]

    def stream_chat(
        self,
        prompt: str,
        model: Optional[str] = None,
        max_tokens: int = 512,
    ) -> Generator[str, None, None]:
        """Streaming chat — yields token chunks as they arrive."""
        body = {
            "model": model or self.default_model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
            "stream": True,
        }
        with self._session.post(
            f"{self.base_url}/v1/chat/completions",
            json=body,
            stream=True,
            timeout=self.timeout,
        ) as resp:
            resp.raise_for_status()
            for line in resp.iter_lines():
                if not line or line == b"data: [DONE]":
                    continue
                if line.startswith(b"data: "):
                    chunk = json.loads(line[6:])
                    delta = chunk["choices"][0].get("delta", {})
                    if "content" in delta:
                        yield delta["content"]

    # ------------------------------------------------------------------
    # Embeddings
    # ------------------------------------------------------------------

    def embed(self, texts: List[str], model: Optional[str] = None) -> List[List[float]]:
        """Return embeddings for a list of texts."""
        body = {"model": model or self.default_model, "input": texts}
        resp = self._session.post(
            f"{self.base_url}/v1/embeddings", json=body, timeout=self.timeout
        )
        resp.raise_for_status()
        return [item["embedding"] for item in resp.json()["data"]]

    # ------------------------------------------------------------------
    # Mesh / HybridNode status
    # ------------------------------------------------------------------

    def list_nodes(self) -> List[NodeStatus]:
        """Return known peers from the local HybridNode DHT snapshot."""
        resp = self._session.get(f"{self.base_url}/v1/nodes", timeout=10)
        resp.raise_for_status()
        nodes = []
        for n in resp.json().get("nodes", []):
            nodes.append(NodeStatus(
                agent_pub_key=n["agent_pub_key"],
                site_id=n.get("site_id", "unknown"),
                vram_mb=n.get("vram_mb", 0),
                reputation=n.get("reputation", 1.0),
                held_models=n.get("held_models", []),
                latency_ms=n.get("latency_ms"),
                has_warrant=n.get("has_warrant", False),
            ))
        return nodes

    def health(self) -> dict:
        """Return daemon health status."""
        resp = self._session.get(f"{self.base_url}/health", timeout=5)
        resp.raise_for_status()
        return resp.json()

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _routing_headers(self, options: Optional[InferenceOptions]) -> dict:
        headers = {}
        if options is None:
            return headers
        if options.preferred_node:
            headers["X-AInonymous-Preferred-Node"] = options.preferred_node
        if options.max_latency_ms is not None:
            headers["X-AInonymous-Max-Latency-Ms"] = str(options.max_latency_ms)
        if options.redundancy_mode:
            headers["X-AInonymous-Redundancy"] = options.redundancy_mode
        if not options.require_attestation:
            headers["X-AInonymous-Require-Attestation"] = "false"
        return headers
