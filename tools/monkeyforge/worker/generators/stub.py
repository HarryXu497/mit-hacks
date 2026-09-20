"""No-GPU generator that produces a real, deterministic GLB.

Exists to validate the whole network path -- orchestrator multipart request, auth, GLB response,
Blender compilation, measurement, features -- before a real image-to-3D model is installed. Swap
`MONKEYFORGE_WORKER_GENERATOR` to `trellis` once the wire is proven.
"""

from __future__ import annotations

import asyncio
import math
import random

from generators.glb import write_glb


class StubGenerator:
    name = "stub"
    version = "stub-1"
    ready = False

    async def warm(self) -> None:
        self.ready = True

    async def generate(
        self,
        image: bytes,
        prompt: str,
        seed: int,
        target_triangles: int,
        part_schema: list[str],
    ) -> bytes:
        # A token delay so latency plumbing and timeouts are exercised rather than bypassed.
        await asyncio.sleep(0.2)

        rng = random.Random(seed)
        segments = 10
        rings = [
            (0.04, 0.48 + rng.uniform(-0.05, 0.05)),
            (0.20, 0.52 + rng.uniform(-0.05, 0.05)),
            (0.43, 0.39 + rng.uniform(-0.05, 0.05)),
            (0.60, 0.18 + rng.uniform(-0.03, 0.03)),
        ]

        vertices: list[tuple[float, float, float]] = []
        for height, radius in rings:
            for index in range(segments):
                angle = math.tau * index / segments
                vertices.append((radius * math.cos(angle), radius * math.sin(angle), height))

        apex = len(vertices)
        vertices.append((0.0, 0.0, 0.69))

        triangles: list[tuple[int, int, int]] = []
        for ring in range(len(rings) - 1):
            lower = ring * segments
            upper = (ring + 1) * segments
            for index in range(segments):
                nxt = (index + 1) % segments
                triangles.append((lower + index, lower + nxt, upper + nxt))
                triangles.append((lower + index, upper + nxt, upper + index))

        top_ring = (len(rings) - 1) * segments
        for index in range(segments):
            nxt = (index + 1) % segments
            triangles.append((top_ring + index, top_ring + nxt, apex))

        return write_glb(vertices, triangles, name="primary_accessory")
