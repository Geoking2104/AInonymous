"""
ainonymous_torch — PyTorch integration for AInonymous HybridNode

Provides:
  - DistributedInferenceModule  : wrap any nn.Module for HybridNode routing
  - ActivationTransfer          : serialize/deserialize tensors for QUIC transfer
  - PipelineSplit               : split a transformer across HybridNode peers
  - AInonymousClient            : HTTP client for the OpenAI-compat API
  - export_to_gguf              : PyTorch → GGUF workflow (via llama.cpp)

Requires:
  pip install torch ainonymous-torch
  HybridNode daemon running at localhost:9337
"""

from .client import AInonymousClient
from .activation_transfer import ActivationTransfer, pack_activation, unpack_activation
from .pipeline import PipelineSplit, LayerRange
from .distributed_inference import DistributedInferenceModule

__version__ = "0.1.0"
__all__ = [
    "AInonymousClient",
    "ActivationTransfer",
    "pack_activation",
    "unpack_activation",
    "PipelineSplit",
    "LayerRange",
    "DistributedInferenceModule",
]
