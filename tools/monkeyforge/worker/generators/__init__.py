"""Pluggable generators behind the worker contract.

Every generator returns raw binary GLB bytes. Low-poly conversion, socket fitting and validation
stay in the Blender compiler on the orchestrator side, per `docs/gpu-worker-contract.md`.
"""

from __future__ import annotations

from typing import Protocol


class Generator(Protocol):
    name: str
    version: str
    ready: bool

    async def warm(self) -> None: ...

    async def generate(
        self,
        image: bytes,
        prompt: str,
        seed: int,
        target_triangles: int,
        part_schema: list[str],
    ) -> bytes: ...


def load_generator(name: str) -> Generator:
    if name == "stub":
        from generators.stub import StubGenerator

        return StubGenerator()
    if name == "trellis":
        from generators.trellis import TrellisGenerator

        return TrellisGenerator()
    raise ValueError(f"unknown generator: {name!r} (expected 'stub' or 'trellis')")
