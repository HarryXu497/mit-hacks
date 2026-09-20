from __future__ import annotations

import json
import math
import shutil
from pathlib import Path
from typing import Protocol
from urllib.parse import urlsplit, urlunsplit

import httpx

from monkeyforge.models import UNKNOWN_VERSION, AccessorySpec, GeneratedAsset

# Bumped when the placeholder geometry itself changes, so cached mock builds
# are not mistaken for current ones.
MOCK_PROVIDER_VERSION = "mock-1"

GENERATOR_VERSION_HEADER = "X-Generator-Version"


class GeometryProvider(Protocol):
    async def generate(
        self,
        accessory: AccessorySpec,
        reference_path: Path,
        seed: int,
        output_dir: Path,
    ) -> GeneratedAsset: ...


class MockGeometryProvider:
    """Writes a tiny low-poly OBJ so the full pipeline runs without a GPU."""

    version = MOCK_PROVIDER_VERSION

    async def generate(
        self,
        accessory: AccessorySpec,
        reference_path: Path,
        seed: int,
        output_dir: Path,
    ) -> GeneratedAsset:
        output_dir.mkdir(parents=True, exist_ok=True)
        output = output_dir / f"{accessory.id}.obj"
        material_path = output.with_suffix(".mtl")
        material_path.write_text(self._placeholder_mtl(), encoding="utf-8")
        output.write_text(
            self._placeholder_obj(accessory.id, accessory.kind, material_path.name),
            encoding="utf-8",
        )
        return GeneratedAsset(
            accessory_id=accessory.id,
            path=output,
            provider="mock",
            media_type="model/obj",
            provider_version=MOCK_PROVIDER_VERSION,
        )

    @staticmethod
    def _placeholder_mtl() -> str:
        return """newmtl Primary
Kd 0.035 0.32 0.72
Ks 0.08 0.08 0.08
Ns 24

newmtl Accent
Kd 1.0 0.58 0.05
Ks 0.12 0.12 0.12
Ns 32
"""

    @classmethod
    def _placeholder_obj(cls, name: str, kind: str, material_file: str) -> str:
        if kind == "hat":
            return cls._helmet_obj(name, material_file)
        return f"""# MonkeyForge placeholder for {name}
mtllib {material_file}
o {name}
usemtl Primary
v -0.5 -0.5 0.0
v  0.5 -0.5 0.0
v  0.5  0.5 0.0
v -0.5  0.5 0.0
v  0.0  0.0 0.7
f 1 2 5
f 2 3 5
f 3 4 5
f 4 1 5
f 1 4 3
f 1 3 2
"""

    @staticmethod
    def _helmet_obj(name: str, material_file: str) -> str:
        lines = [
            f"# Procedural MonkeyForge training helmet for {name}",
            f"mtllib {material_file}",
            f"o {name}",
        ]
        segments = 10
        rings = [(0.04, 0.48), (0.20, 0.52), (0.43, 0.39), (0.60, 0.18)]
        vertices: list[tuple[float, float, float]] = []
        for z, radius in rings:
            for index in range(segments):
                angle = math.tau * index / segments
                vertices.append((radius * math.cos(angle), radius * math.sin(angle), z))
        top_index = len(vertices) + 1
        vertices.append((0, 0, 0.69))
        brim_start = len(vertices) + 1
        for index in range(segments):
            angle = math.tau * index / segments
            vertices.append((0.64 * math.cos(angle), 0.64 * math.sin(angle), 0.025))
        ornament_bottom = len(vertices) + 1
        for index in range(6):
            angle = math.tau * index / 6
            vertices.append((0.10 * math.cos(angle), 0.10 * math.sin(angle), 0.68))
        ornament_top = len(vertices) + 1
        vertices.append((0, 0, 0.86))

        lines.extend(f"v {x:.6f} {y:.6f} {z:.6f}" for x, y, z in vertices)
        lines.append("usemtl Primary")
        for ring in range(len(rings) - 1):
            first = ring * segments + 1
            second = (ring + 1) * segments + 1
            for index in range(segments):
                next_index = (index + 1) % segments
                lines.append(
                    f"f {first + index} {first + next_index} "
                    f"{second + next_index} {second + index}"
                )
        final_ring = (len(rings) - 1) * segments + 1
        for index in range(segments):
            next_index = (index + 1) % segments
            lines.append(f"f {final_ring + index} {final_ring + next_index} {top_index}")

        lines.append("usemtl Accent")
        for index in range(segments):
            next_index = (index + 1) % segments
            lines.append(
                f"f {brim_start + index} {brim_start + next_index} "
                f"{1 + next_index} {1 + index}"
            )
        for index in range(6):
            next_index = (index + 1) % 6
            lines.append(
                f"f {ornament_bottom + index} {ornament_bottom + next_index} {ornament_top}"
            )
        return "\n".join(lines) + "\n"


class HttpGeometryProvider:
    def __init__(
        self,
        endpoint: str,
        token: str | None = None,
        max_response_bytes: int = 100 * 1024 * 1024,
        timeout_seconds: float = 900,
    ) -> None:
        self.endpoint = endpoint
        self.token = token
        self.max_response_bytes = max_response_bytes
        self.timeout_seconds = timeout_seconds

    @property
    def health_url(self) -> str:
        parts = urlsplit(self.endpoint)
        return urlunsplit((parts.scheme, parts.netloc, "/health", "", ""))

    async def probe_version(self, timeout_seconds: float = 5.0) -> str:
        """Ask the worker which generator it is running.

        Used to build the cache key before generation, so pointing at a different
        model invalidates cached jobs. Returns `UNKNOWN_VERSION` if the worker is
        unreachable: the orchestrator must start even while the GPU box is still
        coming up, and a stale-build check at request time covers the gap.
        """
        headers = {}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        try:
            async with httpx.AsyncClient(timeout=timeout_seconds) as client:
                response = await client.get(self.health_url, headers=headers)
                response.raise_for_status()
                version = response.json().get("version")
        except (httpx.HTTPError, ValueError, KeyError):
            return UNKNOWN_VERSION
        return str(version) if version else UNKNOWN_VERSION

    async def generate(
        self,
        accessory: AccessorySpec,
        reference_path: Path,
        seed: int,
        output_dir: Path,
    ) -> GeneratedAsset:
        output_dir.mkdir(parents=True, exist_ok=True)
        headers = {"X-MonkeyForge-Contract": "1"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"

        data = {
            "prompt": accessory.description,
            "seed": str(seed),
            "target_triangles": str(accessory.target_triangles),
            "part_schema": json.dumps(accessory.part_schema),
        }
        async with httpx.AsyncClient(timeout=self.timeout_seconds) as client:
            with reference_path.open("rb") as reference:
                response = await client.post(
                    self.endpoint,
                    headers=headers,
                    data=data,
                    files={
                        "reference": (
                            reference_path.name,
                            reference,
                            "application/octet-stream",
                        )
                    },
                )
            response.raise_for_status()
            content = response.content
            generator_version = response.headers.get(GENERATOR_VERSION_HEADER, UNKNOWN_VERSION)

        if len(content) > self.max_response_bytes:
            raise ValueError("GPU worker response exceeds configured size limit")
        if not content.startswith(b"glTF"):
            raise ValueError("GPU worker did not return a binary GLB")

        output = output_dir / f"{accessory.id}.glb"
        output.write_bytes(content)
        return GeneratedAsset(
            accessory_id=accessory.id,
            path=output,
            provider="http",
            media_type="model/gltf-binary",
            provider_version=generator_version,
        )


def copy_asset(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
