"""Print the real TRELLIS API surface on this box, so the adapter can be corrected against it.

Run on the GX10 after installing TRELLIS.2, before trusting `generators/trellis.py`:

    python probe_trellis.py
"""

from __future__ import annotations

import inspect
import sys


def section(title: str) -> None:
    print(f"\n{'=' * 70}\n{title}\n{'=' * 70}")


section("platform")
print("python:", sys.version)
print("machine:", __import__("platform").machine())

section("torch / cuda")
try:
    import torch

    print("torch:", torch.__version__)
    print("cuda available:", torch.cuda.is_available())
    if torch.cuda.is_available():
        print("device:", torch.cuda.get_device_name(0))
        free, total = torch.cuda.mem_get_info()
        print(f"memory: {free / 1e9:.1f} GB free / {total / 1e9:.1f} GB total")
        print("capability:", torch.cuda.get_device_capability(0))
except Exception as exc:
    print("FAILED:", type(exc).__name__, exc)

section("trellis import")
try:
    import trellis

    print("trellis at:", getattr(trellis, "__file__", "?"))
    from trellis.pipelines import TrellisImageTo3DPipeline

    print("pipeline class:", TrellisImageTo3DPipeline)
    print("\nfrom_pretrained signature:")
    print(" ", inspect.signature(TrellisImageTo3DPipeline.from_pretrained))
    print("\nrun signature:")
    print(" ", inspect.signature(TrellisImageTo3DPipeline.run))
    print("\npublic methods:")
    for name in dir(TrellisImageTo3DPipeline):
        if not name.startswith("_"):
            print("  -", name)
except Exception as exc:
    print("FAILED:", type(exc).__name__, exc)

section("postprocessing")
try:
    from trellis.utils import postprocessing_utils

    print("to_glb signature:")
    print(" ", inspect.signature(postprocessing_utils.to_glb))
except Exception as exc:
    print("FAILED:", type(exc).__name__, exc)

section("custom CUDA extensions (the usual aarch64 casualties)")
for module in (
    "flash_attn",
    "spconv",
    "nvdiffrast",
    "diff_gaussian_rasterization",
    "xformers",
    "kaolin",
    "utils3d",
    "rembg",
):
    try:
        loaded = __import__(module)
        print(f"  OK       {module} {getattr(loaded, '__version__', '')}")
    except Exception as exc:
        print(f"  MISSING  {module}: {type(exc).__name__}: {exc}")
