"""
Tests for activation_transfer.py — binary wire format integrity.

Run:
    pytest tests/test_activation_transfer.py -v
"""

import struct
import pytest
import torch

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from ainonymous_torch.activation_transfer import (
    pack_activation,
    unpack_activation,
    ActivationTransfer,
    MAGIC,
)


# ------------------------------------------------------------------
# Helpers
# ------------------------------------------------------------------

def make_tensor(shape, dtype=torch.float32):
    return torch.randn(*shape, dtype=dtype) if dtype.is_floating_point else \
           torch.randint(-128, 127, shape, dtype=dtype)


# ------------------------------------------------------------------
# Round-trip tests — all supported dtypes
# ------------------------------------------------------------------

class TestRoundTrip:
    @pytest.mark.parametrize("dtype", [
        torch.float32,
        torch.float16,
        torch.bfloat16,
        torch.int8,
        torch.int32,
        torch.int64,
    ])
    def test_dtype_roundtrip(self, dtype):
        t = make_tensor((2, 16, 64), dtype)
        payload = pack_activation(t)
        restored = unpack_activation(payload)
        assert restored.dtype == dtype
        assert restored.shape == t.shape
        if dtype != torch.bfloat16:
            # bfloat16 has limited precision; exact equality holds for stored types
            assert torch.equal(t, restored), f"Mismatch for dtype={dtype}"

    def test_scalar_tensor(self):
        """1-D edge case."""
        t = torch.tensor([1.0, 2.0, 3.0], dtype=torch.float32)
        assert torch.equal(t, unpack_activation(pack_activation(t)))

    def test_4d_tensor(self):
        """Typical attention mask shape."""
        t = torch.randn(2, 8, 32, 32, dtype=torch.float16)
        payload = pack_activation(t)
        restored = unpack_activation(payload)
        assert torch.equal(t, restored)

    def test_large_tensor(self):
        """~16 MB float32 blob — verifies no off-by-one in size calculation."""
        t = torch.randn(4, 128, 4096, dtype=torch.float32)
        payload = pack_activation(t)
        restored = unpack_activation(payload)
        assert torch.allclose(t, restored)


# ------------------------------------------------------------------
# Wire format inspection
# ------------------------------------------------------------------

class TestWireFormat:
    def test_magic_header(self):
        payload = pack_activation(torch.randn(2, 4))
        magic = struct.unpack_from(">I", payload, 0)[0]
        assert magic == MAGIC, f"Bad magic: {hex(magic)}"

    def test_payload_length_float32(self):
        t = torch.randn(3, 4, dtype=torch.float32)
        payload = pack_activation(t)
        # magic(4) + dtype_id(1) + ndim(1) + shape(ndim*8) + data(numel*itemsize)
        expected = 4 + 1 + 1 + 2 * 8 + 3 * 4 * 4
        assert len(payload) == expected

    def test_payload_length_float16(self):
        t = torch.randn(3, 4, dtype=torch.float16)
        payload = pack_activation(t)
        expected = 4 + 1 + 1 + 2 * 8 + 3 * 4 * 2
        assert len(payload) == expected

    def test_corrupt_magic_raises(self):
        payload = bytearray(pack_activation(torch.randn(2, 2)))
        payload[0] ^= 0xFF  # Corrupt first byte
        with pytest.raises(ValueError, match="[Mm]agic"):
            unpack_activation(bytes(payload))

    def test_truncated_payload_raises(self):
        payload = pack_activation(torch.randn(4, 4))
        with pytest.raises((ValueError, struct.error)):
            unpack_activation(payload[:10])  # Cut off mid-header


# ------------------------------------------------------------------
# Chunking / ActivationTransfer
# ------------------------------------------------------------------

class TestActivationTransfer:
    def test_split_merge_identity(self):
        t = torch.randn(8, 64, 256, dtype=torch.float16)
        at = ActivationTransfer(max_chunk_mb=1)
        chunks = at.split(t)
        assert len(chunks) > 1, "Expected chunking for ~8 MB tensor with 1 MB limit"
        reconstructed = at.merge(chunks)
        assert torch.equal(t, reconstructed)

    def test_single_chunk_when_small(self):
        t = torch.randn(1, 8, 16, dtype=torch.float32)  # tiny
        at = ActivationTransfer(max_chunk_mb=64)
        chunks = at.split(t)
        assert len(chunks) == 1
        assert torch.equal(t, at.merge(chunks))

    def test_stats_populated(self):
        t = torch.randn(4, 32, 128, dtype=torch.float16)
        at = ActivationTransfer(max_chunk_mb=2)
        chunks = at.split(t)
        at.merge(chunks)
        stats = at.stats
        assert "chunks" in stats
        assert "total_bytes" in stats
        assert stats["total_bytes"] > 0

    def test_empty_chunks_raises(self):
        at = ActivationTransfer()
        with pytest.raises((ValueError, IndexError)):
            at.merge([])

    @pytest.mark.parametrize("dtype", [torch.float32, torch.float16, torch.bfloat16])
    def test_chunked_dtype_preservation(self, dtype):
        t = make_tensor((2, 64, 128), dtype)
        at = ActivationTransfer(max_chunk_mb=0.5)
        chunks = at.split(t)
        reconstructed = at.merge(chunks)
        assert reconstructed.dtype == dtype
        assert reconstructed.shape == t.shape


# ------------------------------------------------------------------
# Bandwidth / size assertions (non-functional but informative)
# ------------------------------------------------------------------

class TestBandwidthCharacteristics:
    def test_float16_half_the_size_of_float32(self):
        t32 = torch.randn(4, 128, 4096, dtype=torch.float32)
        t16 = t32.half()
        p32 = pack_activation(t32)
        p16 = pack_activation(t16)
        ratio = len(p32) / len(p16)
        assert 1.9 < ratio < 2.1, f"Expected ~2x size ratio, got {ratio:.2f}"

    def test_bfloat16_half_the_size_of_float32(self):
        t32 = torch.randn(2, 64, 2048, dtype=torch.float32)
        tb16 = t32.bfloat16()
        assert len(pack_activation(t32)) == pytest.approx(len(pack_activation(tb16)) * 2, rel=0.01)
