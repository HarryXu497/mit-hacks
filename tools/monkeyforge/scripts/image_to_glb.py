"""Turn one sprite into a GLB with TRELLIS.2, without going through the worker HTTP server.

The worker exists to serve the game; this is for building an accessory by hand and looking at it.
It reuses the same generator so there is one TRELLIS code path, not two that drift apart.

Run (on the GX10):
    python scripts/image_to_glb.py --image cap.png --output cap3d.glb --seed 1
"""

from __future__ import annotations

import argparse
import asyncio
import sys
from pathlib import Path

WORKER_ROOT = Path.home() / "monkeyforge-worker"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--worker-root", type=Path, default=WORKER_ROOT)
    args = parser.parse_args()

    sys.path.insert(0, str(args.worker_root))
    from worker.generators.trellis import TrellisGenerator

    generator = TrellisGenerator()
    payload = args.image.read_bytes()

    async def run() -> bytes:
        await generator.warm()
        # `prompt`, `target_triangles` and `part_schema` are part of the worker's request contract
        # and are ignored by the TRELLIS path itself -- the Blender compiler owns the budget.
        return await generator.generate(
            payload, prompt="", seed=args.seed, target_triangles=0, part_schema=[]
        )

    glb = asyncio.run(run())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(glb)
    print(f"wrote {args.output} ({len(glb)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
