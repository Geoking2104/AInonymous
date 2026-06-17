"""
model_export.py — Export a PyTorch model to GGUF for AInonymous inference.

GGUF is the format consumed by llama.cpp, which AInonymous uses for local
inference on each HybridNode peer. This module wraps the llama.cpp conversion
scripts and generates a ModelManifest (SHA-256) ready for Holochain attestation.

Workflow::

    1. Load PyTorch / Hugging Face model
    2. Save as safetensors (intermediate)
    3. Run llama.cpp convert_hf_to_gguf.py → .gguf
    4. (Optional) Run llama-quantize → quantized .gguf (e.g. Q4_K_M)
    5. Compute SHA-256 → ModelManifest dict for Holochain DHT

Requirements:
    pip install torch transformers safetensors
    git clone https://github.com/ggerganov/llama.cpp  (for convert scripts)

Usage::

    manifest = export_to_gguf(
        model_id="google/gemma-2-9b-it",
        output_dir="~/.ainonymous/models",
        quantization="Q4_K_M",
        llama_cpp_dir="~/llama.cpp",
    )
    print(manifest)
    # → {"model_name": "gemma-2-9b-it", "sha256": "ab12...", "size_bytes": 5368709120, ...}
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Optional


@dataclass
class ModelManifest:
    """Mirrors the ModelManifest Holochain entry in the attestation DNA."""
    model_name: str
    version: str
    sha256: str
    size_bytes: int
    architecture: str
    quant_format: str
    num_layers: int
    gguf_path: str

    def to_dict(self) -> dict:
        return asdict(self)

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=2)


def sha256_file(path: Path) -> str:
    """Compute SHA-256 of a file, streaming in 8 MB chunks."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def export_to_gguf(
    model_id: str,
    output_dir: str = "~/.ainonymous/models",
    quantization: Optional[str] = "Q4_K_M",
    llama_cpp_dir: Optional[str] = None,
    num_layers: int = 0,
    architecture: str = "transformer",
    version: str = "1.0",
) -> ModelManifest:
    """
    Convert a Hugging Face model to GGUF and return a ModelManifest.

    Args:
        model_id:       HF model ID (e.g. "google/gemma-2-9b-it") or local path.
        output_dir:     Where to save the .gguf file.
        quantization:   llama-quantize type ("Q4_K_M", "Q8_0", "F16", None=skip).
        llama_cpp_dir:  Path to a llama.cpp checkout (needs convert_hf_to_gguf.py).
        num_layers:     Number of transformer layers (read from config if 0).
        architecture:   Architecture name for the manifest.
        version:        Version tag for the manifest.

    Returns:
        ModelManifest with SHA-256 ready for Holochain attestation.

    Raises:
        FileNotFoundError: If llama_cpp_dir or conversion script not found.
        subprocess.CalledProcessError: If conversion fails.
    """
    output_path = Path(output_dir).expanduser()
    output_path.mkdir(parents=True, exist_ok=True)

    model_name = model_id.split("/")[-1]

    # ------------------------------------------------------------------
    # Step 1: Detect llama.cpp convert script
    # ------------------------------------------------------------------
    if llama_cpp_dir is None:
        llama_cpp_dir = os.environ.get("LLAMA_CPP_DIR", "~/llama.cpp")
    llama_cpp = Path(llama_cpp_dir).expanduser()
    convert_script = llama_cpp / "convert_hf_to_gguf.py"
    if not convert_script.exists():
        raise FileNotFoundError(
            f"llama.cpp convert script not found at {convert_script}. "
            f"Clone https://github.com/ggerganov/llama.cpp and set LLAMA_CPP_DIR."
        )

    # ------------------------------------------------------------------
    # Step 2: Convert HF → F16 GGUF
    # ------------------------------------------------------------------
    f16_path = output_path / f"{model_name}-f16.gguf"
    print(f"[ainonymous_torch] Converting {model_id} → {f16_path} ...")
    subprocess.run(
        [
            sys.executable,
            str(convert_script),
            model_id,
            "--outtype", "f16",
            "--outfile", str(f16_path),
        ],
        check=True,
    )

    # ------------------------------------------------------------------
    # Step 3: (Optional) Quantize
    # ------------------------------------------------------------------
    final_path = f16_path
    if quantization and quantization.upper() != "F16":
        quant_path = output_path / f"{model_name}-{quantization.lower()}.gguf"
        quantize_bin = llama_cpp / "build" / "bin" / "llama-quantize"
        if not quantize_bin.exists():
            quantize_bin = llama_cpp / "llama-quantize"  # older layout
        if not quantize_bin.exists():
            print(f"[ainonymous_torch] WARNING: llama-quantize not found at {quantize_bin}, skipping quantization.")
        else:
            print(f"[ainonymous_torch] Quantizing {quantization} → {quant_path} ...")
            subprocess.run(
                [str(quantize_bin), str(f16_path), str(quant_path), quantization.upper()],
                check=True,
            )
            final_path = quant_path

    # ------------------------------------------------------------------
    # Step 4: Read num_layers from HF config if not provided
    # ------------------------------------------------------------------
    if num_layers == 0:
        try:
            import json as _json
            config_candidates = [
                Path(model_id) / "config.json",
                Path(model_id).expanduser() / "config.json",
            ]
            for cfg_path in config_candidates:
                if cfg_path.exists():
                    cfg = _json.loads(cfg_path.read_text())
                    num_layers = cfg.get("num_hidden_layers", cfg.get("n_layer", 0))
                    break
        except Exception:
            pass

    # ------------------------------------------------------------------
    # Step 5: Compute SHA-256 and return manifest
    # ------------------------------------------------------------------
    print(f"[ainonymous_torch] Computing SHA-256 of {final_path} ...")
    digest = sha256_file(final_path)
    size = final_path.stat().st_size

    manifest = ModelManifest(
        model_name=model_name,
        version=version,
        sha256=digest,
        size_bytes=size,
        architecture=architecture,
        quant_format=quantization or "f16",
        num_layers=num_layers,
        gguf_path=str(final_path),
    )

    # Save manifest alongside the model
    manifest_path = final_path.with_suffix(".manifest.json")
    manifest_path.write_text(manifest.to_json())
    print(f"[ainonymous_torch] Manifest saved to {manifest_path}")
    print(f"[ainonymous_torch] SHA-256: {digest}")

    return manifest


def verify_gguf(gguf_path: str, expected_sha256: str) -> bool:
    """
    Verify a GGUF file against its expected SHA-256 (from the Holochain ModelManifest).

    Use this before serving a model to confirm it hasn't been tampered with.

    Example::

        ok = verify_gguf("~/.ainonymous/models/gemma-2-9b-q4_k_m.gguf", manifest["sha256"])
        if not ok:
            raise RuntimeError("Model hash mismatch — refusing to serve")
    """
    path = Path(gguf_path).expanduser()
    actual = sha256_file(path)
    match = actual == expected_sha256
    if not match:
        print(f"[ainonymous_torch] HASH MISMATCH: expected={expected_sha256} actual={actual}")
    return match
