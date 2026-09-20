from __future__ import annotations

import asyncio
import hashlib
import shutil
from pathlib import Path
from typing import Protocol

from monkeyforge.models import (
    SOCKET_NODE_NAMES,
    UNKNOWN_VERSION,
    AccessorySpec,
    Archetype,
    CompiledAsset,
    GeneratedAsset,
    Palette,
)

PASSTHROUGH_COMPILER_VERSION = "passthrough-1"


def file_digest(path: Path) -> str:
    """Short content hash of a file, or `UNKNOWN_VERSION` if it is not readable.

    Used to version the compiler by its script and the bases by their bytes, so
    editing either invalidates cached builds without anyone remembering to bump
    a constant.
    """
    try:
        data = path.read_bytes()
    except OSError:
        return UNKNOWN_VERSION
    return hashlib.sha256(data).hexdigest()[:12]


def digest_files(paths: list[Path]) -> str:
    """Combined content hash of several files, order-independent."""
    digests = sorted(f"{path.name}:{file_digest(path)}" for path in paths)
    if not digests:
        return UNKNOWN_VERSION
    return hashlib.sha256("\0".join(digests).encode("utf-8")).hexdigest()[:12]


class AssetCompiler(Protocol):
    async def compile(
        self,
        generated: GeneratedAsset,
        accessory: AccessorySpec,
        output_dir: Path,
        archetype: Archetype = Archetype.BALANCED,
        palette: Palette | None = None,
    ) -> CompiledAsset: ...


class PassthroughCompiler:
    version = PASSTHROUGH_COMPILER_VERSION

    async def compile(
        self,
        generated: GeneratedAsset,
        accessory: AccessorySpec,
        output_dir: Path,
        archetype: Archetype = Archetype.BALANCED,
        palette: Palette | None = None,
    ) -> CompiledAsset:
        output_dir.mkdir(parents=True, exist_ok=True)
        output = output_dir / f"{accessory.id}{generated.path.suffix}"
        shutil.copy2(generated.path, output)
        return CompiledAsset(
            path=output,
            compiler="passthrough",
            media_type=generated.media_type,
            compiler_version=PASSTHROUGH_COMPILER_VERSION,
        )


class BlenderCompiler:
    def __init__(
        self,
        blender_path: str,
        base_models: dict[Archetype, Path],
        script_path: Path,
    ) -> None:
        self.blender_path = blender_path
        self.base_models = base_models
        self.script_path = script_path

    @property
    def version(self) -> str:
        """Identity of the compiler, derived from the script that defines it.

        The script decides scale normalisation, socket seating and decimation, so
        its contents *are* the compiler's behaviour. Hashing it means an edit to
        `blender_compile.py` invalidates cached builds on its own.
        """
        return f"blender-{file_digest(self.script_path)}"

    @property
    def base_models_version(self) -> str:
        return digest_files(sorted(set(self.base_models.values())))

    async def compile(
        self,
        generated: GeneratedAsset,
        accessory: AccessorySpec,
        output_dir: Path,
        archetype: Archetype = Archetype.BALANCED,
        palette: Palette | None = None,
    ) -> CompiledAsset:
        base_model = self.base_models.get(archetype) or self.base_models[Archetype.BALANCED]
        if not base_model.exists():
            raise FileNotFoundError(f"base monkey not found: {base_model}")
        if not self.script_path.exists():
            raise FileNotFoundError(f"Blender compiler script not found: {self.script_path}")

        output_dir.mkdir(parents=True, exist_ok=True)
        output = output_dir / "character.glb"
        command = [
            self.blender_path,
            "--background",
            "--python",
            str(self.script_path),
            "--",
            "--base",
            str(base_model),
            "--accessory",
            str(generated.path),
            "--output",
            str(output),
            "--socket",
            SOCKET_NODE_NAMES[accessory.slot],
            "--target-triangles",
            str(accessory.target_triangles),
        ]
        if palette is not None:
            command += [
                "--jersey", palette.jersey,
                "--accent", palette.accent,
                "--fur", palette.fur,
                "--face", palette.face,
            ]
        process = await asyncio.create_subprocess_exec(
            *command,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT,
        )
        stdout, _ = await process.communicate()
        if process.returncode != 0:
            message = stdout.decode("utf-8", errors="replace")[-4000:]
            raise RuntimeError(f"Blender compilation failed:\n{message}")
        if not output.exists():
            raise RuntimeError("Blender exited successfully but produced no GLB")

        return CompiledAsset(
            path=output,
            compiler="blender",
            media_type="model/gltf-binary",
            compiler_version=self.version,
        )
