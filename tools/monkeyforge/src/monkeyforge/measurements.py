"""Structured GLB measurements shared by hard validation and ranker feature extraction.

A candidate is measured once by ``scripts/blender_measure_glb.py``; everything downstream reads the
resulting document instead of re-opening Blender.
"""

from __future__ import annotations

import asyncio
import json
from pathlib import Path

from pydantic import Field

from monkeyforge.models import (
    SOCKET_NODE_NAMES,
    AccessorySpec,
    CompiledAsset,
    StrictModel,
)

REQUIRED_SOCKET_NODES = (
    "SOCKET_HEAD_TOP",
    "SOCKET_FACE",
    "SOCKET_BACK",
    "SOCKET_WAIST",
    "SOCKET_HAND_LEFT",
    "SOCKET_HAND_RIGHT",
    "SOCKET_TAIL_TIP",
)


class MeshMeasurement(StrictModel):
    name: str
    parent: str | None = None
    triangles: int = Field(ge=0)
    loose_parts: int = Field(ge=0)
    materials: list[str] = Field(default_factory=list)
    bbox_min: list[float] | None = None
    bbox_max: list[float] | None = None


class AccessoryMeasurement(StrictModel):
    socket: str
    mesh_names: list[str] = Field(default_factory=list)
    triangles: int = Field(ge=0)
    loose_parts: int = Field(ge=0)
    bbox_min: list[float]
    bbox_max: list[float]
    extent: list[float]
    socket_location: list[float] = Field(default_factory=list)
    socket_offset: float = Field(ge=0)
    socket_gap: float = Field(default=0.0, ge=0)
    socket_contained: bool


class GlbMeasurement(StrictModel):
    schema_version: str = "1.0"
    source: str
    total_triangles: int = Field(ge=0)
    mesh_count: int = Field(ge=0)
    material_count: int = Field(ge=0)
    loose_parts: int = Field(ge=0)
    missing_sockets: list[str] = Field(default_factory=list)
    sockets: dict[str, list[float]] = Field(default_factory=dict)
    meshes: list[MeshMeasurement] = Field(default_factory=list)
    accessory: AccessoryMeasurement | None = None


def load_measurement(path: Path) -> GlbMeasurement:
    return GlbMeasurement.model_validate_json(path.read_text(encoding="utf-8"))


class MeasurementError(RuntimeError):
    """Raised when Blender could not produce a measurement document."""


async def measure_glb(
    glb_path: Path,
    output_path: Path,
    blender_path: str,
    script_path: Path,
    accessory_socket: str | None = None,
) -> GlbMeasurement:
    """Run the Blender measurement pass and return the parsed document."""
    if not script_path.exists():
        raise MeasurementError(f"measurement script not found: {script_path}")

    command = [
        blender_path,
        "--background",
        "--python",
        str(script_path),
        "--",
        "--input",
        str(glb_path),
        "--output",
        str(output_path),
    ]
    if accessory_socket:
        command += ["--accessory-socket", accessory_socket]

    process = await asyncio.create_subprocess_exec(
        *command,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.STDOUT,
    )
    stdout, _ = await process.communicate()
    message = stdout.decode("utf-8", errors="replace")[-4000:]

    # A non-zero exit means the asset failed its hard checks, but the document is still written
    # and still worth reading: it says *why* it failed.
    if not output_path.exists():
        raise MeasurementError(f"Blender produced no measurement document:\n{message}")

    try:
        return load_measurement(output_path)
    except (json.JSONDecodeError, ValueError) as exc:
        raise MeasurementError(f"unreadable measurement document: {exc}") from exc


class BlenderMeasurer:
    """Runs the Blender measurement pass over a compiled asset."""

    def __init__(self, blender_path: str, script_path: Path) -> None:
        self.blender_path = blender_path
        self.script_path = script_path

    async def measure(
        self,
        compiled: CompiledAsset,
        accessory: AccessorySpec,
        output_dir: Path,
    ) -> GlbMeasurement:
        return await measure_glb(
            glb_path=Path(compiled.path),
            output_path=output_dir / "measurement.json",
            blender_path=self.blender_path,
            script_path=self.script_path,
            accessory_socket=SOCKET_NODE_NAMES[accessory.slot],
        )
