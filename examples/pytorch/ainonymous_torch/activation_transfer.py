"""
activation_transfer.py — Serialize/deserialize PyTorch tensors for QUIC transfer.

When a transformer model is split across HybridNode peers (pipeline-splitting),
the activation tensor (hidden state) produced by layers [0..N/2] on Node A must
be transmitted to Node B which handles layers [N/2..N].

Wire format (little-endian binary):
  [4 bytes] magic: 0xA1A0C1C0
  [1 byte]  dtype_id  (0=float32, 1=float16, 2=bfloat16, 3=int8)
  [1 byte]  ndim
  [ndim × 4 bytes] shape (uint32 per dim)
  [remaining bytes] raw tensor data (contiguous, CPU)

Design choices:
  - No pickle — safe to deserialize from untrusted peers
  - Contiguous memory layout — zero-copy mmap possible
  - dtype preserved — bfloat16 activations stay bfloat16 over the wire
"""

from __future__ import annotations

import struct
from io import BytesIO
from typing import Tuple

import torch

_MAGIC = 0xA1A0C1C0

_DTYPE_TO_ID = {
    torch.float32:  0,
    torch.float16:  1,
    torch.bfloat16: 2,
    torch.int8:     3,
    torch.int32:    4,
    torch.int64:    5,
}
_ID_TO_DTYPE = {v: k for k, v in _DTYPE_TO_ID.items()}


def pack_activation(tensor: torch.Tensor) -> bytes:
    """
    Serialize a tensor to bytes for QUIC transfer.

    Args:
        tensor: Any CPU or CUDA tensor. CUDA tensors are moved to CPU first.

    Returns:
        Packed bytes ready to send over the wire.

    Example::

        hidden = model.forward_layers_0_to_16(input_ids)
        payload = pack_activation(hidden)
        quic_stream.write(payload)
    """
    if tensor.is_cuda:
        tensor = tensor.cpu()
    if not tensor.is_contiguous():
        tensor = tensor.contiguous()

    dtype_id = _DTYPE_TO_ID.get(tensor.dtype)
    if dtype_id is None:
        raise ValueError(f"Unsupported dtype for wire transfer: {tensor.dtype}")

    buf = BytesIO()
    # Header
    buf.write(struct.pack("<I", _MAGIC))
    buf.write(struct.pack("<B", dtype_id))
    buf.write(struct.pack("<B", tensor.ndim))
    for dim in tensor.shape:
        buf.write(struct.pack("<I", dim))
    # Raw data
    buf.write(tensor.numpy().tobytes())
    return buf.getvalue()


def unpack_activation(data: bytes, device: str = "cpu") -> torch.Tensor:
    """
    Deserialize a tensor received over QUIC.

    Args:
        data:   Bytes as received from the wire.
        device: Target device ("cpu", "cuda:0", etc.)

    Returns:
        Reconstructed tensor on the specified device.

    Example::

        payload = quic_stream.read()
        hidden = unpack_activation(payload, device="cuda:0")
        output = model.forward_layers_16_to_32(hidden)
    """
    buf = BytesIO(data)

    magic, = struct.unpack("<I", buf.read(4))
    if magic != _MAGIC:
        raise ValueError(f"Bad magic: expected {_MAGIC:#010x}, got {magic:#010x}")

    dtype_id, = struct.unpack("<B", buf.read(1))
    dtype = _ID_TO_DTYPE.get(dtype_id)
    if dtype is None:
        raise ValueError(f"Unknown dtype_id: {dtype_id}")

    ndim, = struct.unpack("<B", buf.read(1))
    shape: Tuple[int, ...] = tuple(
        struct.unpack("<I", buf.read(4))[0] for _ in range(ndim)
    )

    raw = buf.read()
    tensor = torch.frombuffer(bytearray(raw), dtype=dtype).reshape(shape)
    return tensor.to(device)


class ActivationTransfer:
    """
    Higher-level helper that tracks statistics and handles chunking
    for large activations that exceed QUIC stream limits.

    Example::

        transfer = ActivationTransfer(max_chunk_mb=256)
        chunks = transfer.split(hidden_state)       # list[bytes]
        hidden_state = transfer.merge(chunks)       # reconstruct
    """

    def __init__(self, max_chunk_mb: int = 256):
        self.max_chunk_bytes = max_chunk_mb * 1024 * 1024
        self.bytes_sent: int = 0
        self.bytes_received: int = 0

    def pack(self, tensor: torch.Tensor) -> bytes:
        data = pack_activation(tensor)
        self.bytes_sent += len(data)
        return data

    def unpack(self, data: bytes, device: str = "cpu") -> torch.Tensor:
        self.bytes_received += len(data)
        return unpack_activation(data, device=device)

    def split(self, tensor: torch.Tensor) -> list[bytes]:
        """Split a large tensor into ≤ max_chunk_mb chunks along dim=0."""
        full = pack_activation(tensor)
        if len(full) <= self.max_chunk_bytes:
            return [full]
        # Split along batch / sequence dimension
        n = tensor.shape[0]
        chunk_size = max(1, n * self.max_chunk_bytes // len(full))
        return [pack_activation(tensor[i:i+chunk_size]) for i in range(0, n, chunk_size)]

    def merge(self, chunks: list[bytes], device: str = "cpu") -> torch.Tensor:
        """Reconstruct a tensor from chunks."""
        tensors = [unpack_activation(c, device=device) for c in chunks]
        return torch.cat(tensors, dim=0)

    @property
    def stats(self) -> dict:
        return {
            "bytes_sent": self.bytes_sent,
            "bytes_received": self.bytes_received,
            "mb_sent": self.bytes_sent / 1024**2,
            "mb_received": self.bytes_received / 1024**2,
        }
