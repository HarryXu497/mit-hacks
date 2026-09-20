"""TRELLIS.2 adapter.

Written against the real TRELLIS.2 API as installed on the GX10, not the v1 API the first draft of
this file assumed. The differences that matter:

    v1 (wrong)                              TRELLIS.2 (actual)
    trellis.pipelines                       trellis2.pipelines
    TrellisImageTo3DPipeline                Trellis2ImageTo3DPipeline
    "microsoft/TRELLIS-image-large"         "microsoft/TRELLIS.2-4B"
    run() -> dict with "mesh"/"gaussian"    run() -> list[MeshWithVoxel]
    trellis.utils.postprocessing_utils      o_voxel.postprocess (separate compiled package)
    to_glb(gaussian, mesh, ...)             to_glb(vertices=, faces=, attr_volume=, coords=, ...)

Background removal is not done here: `pipeline.run` preprocesses the image itself, so a rembg pass
would be redundant work on the critical path.

Sparse attention needs a backend. flash-attn has no aarch64/Blackwell wheel and xformers ships only
an sdist, so this deployment uses a hand-written sdpa path patched into
`trellis2/modules/sparse/attention/`. Both backend switches must be set -- the dense one
(`ATTN_BACKEND`) and the sparse one (`SPARSE_ATTN_BACKEND`), which are independent globals in
different config modules.
"""

from __future__ import annotations

import asyncio
import io
import os
import tempfile
from pathlib import Path

MODEL_ID = os.environ.get("MONKEYFORGE_TRELLIS_MODEL", "microsoft/TRELLIS.2-4B")
TRELLIS_ROOT = os.environ.get("MONKEYFORGE_TRELLIS_ROOT", os.path.expanduser("~/TRELLIS.2"))

# nvdiffrast cannot handle more than this many faces; upstream's example clamps to it.
NVDIFFRAST_FACE_LIMIT = 16_777_216

# The orchestrator's Blender compiler owns the final triangle budget, so this only has to be low
# enough to keep the GLB transfer sane while leaving the compiler something to decimate from.
DECIMATION_TARGET = int(os.environ.get("MONKEYFORGE_TRELLIS_DECIMATION", "100000"))
TEXTURE_SIZE = int(os.environ.get("MONKEYFORGE_TRELLIS_TEXTURE", "1024"))


def _configure_backends() -> None:
    """Select attention backends before trellis2 is imported.

    Both config modules read their environment at import time, so this must run first. sdpa is
    absent from the sparse whitelist upstream; this deployment patches it in.
    """
    os.environ.setdefault("ATTN_BACKEND", "sdpa")
    os.environ.setdefault("SPARSE_ATTN_BACKEND", "sdpa")
    os.environ.setdefault("SPCONV_ALGO", "native")
    os.environ.setdefault("PYTORCH_CUDA_ALLOC_CONF", "expandable_segments:True")
    os.environ.setdefault("OPENCV_IO_ENABLE_OPENEXR", "1")


class TrellisGenerator:
    name = "trellis"
    version = f"{MODEL_ID}@{DECIMATION_TARGET}"
    ready = False

    def __init__(self) -> None:
        self._pipeline = None

    async def warm(self) -> None:
        """Load weights once, at startup.

        TRELLIS.2-4B is ~16 GB. A cold load inside a request costs minutes and would destroy any
        latency budget, so this runs from the worker's startup hook.
        """
        loop = asyncio.get_running_loop()
        self._pipeline = await loop.run_in_executor(None, self._load)
        self.ready = True

    def _load(self):
        _configure_backends()
        import sys

        if TRELLIS_ROOT not in sys.path:
            sys.path.insert(0, TRELLIS_ROOT)

        import torch
        from trellis2.pipelines import Trellis2ImageTo3DPipeline

        if not torch.cuda.is_available():
            raise RuntimeError("no CUDA device visible; check the driver and container GPU access")

        pipeline = Trellis2ImageTo3DPipeline.from_pretrained(MODEL_ID)
        pipeline.cuda()
        return pipeline

    async def generate(
        self,
        image: bytes,
        prompt: str,
        seed: int,
        target_triangles: int,
        part_schema: list[str],
    ) -> bytes:
        if self._pipeline is None:
            raise RuntimeError("pipeline not warmed")
        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(None, self._generate, image, seed)

    def _generate(self, image: bytes, seed: int) -> bytes:
        import o_voxel
        from PIL import Image

        reference = Image.open(io.BytesIO(image)).convert("RGB")

        # run() preprocesses the image internally, hence no rembg pass here. The seed is honoured,
        # which is what makes the orchestrator's build digest meaningful.
        mesh = self._pipeline.run(reference, num_samples=1, seed=seed)[0]
        mesh.simplify(NVDIFFRAST_FACE_LIMIT)

        glb = o_voxel.postprocess.to_glb(
            vertices=mesh.vertices,
            faces=mesh.faces,
            attr_volume=mesh.attrs,
            coords=mesh.coords,
            attr_layout=mesh.layout,
            voxel_size=mesh.voxel_size,
            aabb=[[-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]],
            decimation_target=DECIMATION_TARGET,
            texture_size=TEXTURE_SIZE,
            remesh=True,
            remesh_band=1,
            remesh_project=0,
            verbose=False,
        )

        # to_glb returns a trimesh scene; export through a temp file because that is the path
        # upstream exercises, then hand the orchestrator raw bytes as the contract requires.
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "accessory.glb"
            glb.export(str(out))
            return out.read_bytes()
