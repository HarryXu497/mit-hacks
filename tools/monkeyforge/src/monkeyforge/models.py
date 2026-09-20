from __future__ import annotations

import hashlib
from datetime import UTC, datetime
from enum import StrEnum
from pathlib import Path

from pydantic import BaseModel, ConfigDict, Field, field_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


# Bumped when pipeline logic changes the bytes of a build for reasons no other
# identity field captures. Changing it invalidates every cached job.
PIPELINE_VERSION = "1"

UNKNOWN_VERSION = "unknown"


class Archetype(StrEnum):
    BALANCED = "balanced"
    RUNNER = "runner"
    PLAYMAKER = "playmaker"
    DEFENDER = "defender"
    GOALKEEPER = "goalkeeper"


class Socket(StrEnum):
    HEAD_TOP = "head_top"
    FACE = "face"
    BACK = "back"
    WAIST = "waist"
    HAND_LEFT = "hand_left"
    HAND_RIGHT = "hand_right"
    TAIL_TIP = "tail_tip"


SOCKET_NODE_NAMES: dict[Socket, str] = {
    Socket.HEAD_TOP: "SOCKET_HEAD_TOP",
    Socket.FACE: "SOCKET_FACE",
    Socket.BACK: "SOCKET_BACK",
    Socket.WAIST: "SOCKET_WAIST",
    Socket.HAND_LEFT: "SOCKET_HAND_LEFT",
    Socket.HAND_RIGHT: "SOCKET_HAND_RIGHT",
    Socket.TAIL_TIP: "SOCKET_TAIL_TIP",
}


class AbilityId(StrEnum):
    NONE = "none"
    TAILWIND = "tailwind"
    CURVE_SHOT = "curve_shot"
    RALLY = "rally"
    INTERCEPT_READ = "intercept_read"
    GOALIE_FOCUS = "goalie_focus"
    SHIELD_WALL = "shield_wall"


class JobState(StrEnum):
    QUEUED = "queued"
    PARSING = "parsing"
    GENERATING = "generating"
    COMPILING = "compiling"
    VALIDATING = "validating"
    COMPLETE = "complete"
    FAILED = "failed"


class BodyMorphs(StrictModel):
    head_size: float = Field(default=0.5, ge=0, le=1)
    muzzle_size: float = Field(default=0.5, ge=0, le=1)
    ear_size: float = Field(default=0.5, ge=0, le=1)
    torso_width: float = Field(default=0.5, ge=0, le=1)
    arm_length: float = Field(default=0.5, ge=0, le=1)
    leg_length: float = Field(default=0.5, ge=0, le=1)
    hand_size: float = Field(default=0.5, ge=0, le=1)
    foot_size: float = Field(default=0.5, ge=0, le=1)


class Palette(StrictModel):
    fur: str = "#92572F"
    face: str = "#E8B978"
    jersey: str = "#36A7D9"
    accent: str = "#F5CE42"

    @field_validator("fur", "face", "jersey", "accent")
    @classmethod
    def validate_hex(cls, value: str) -> str:
        value = value.upper()
        if len(value) != 7 or value[0] != "#":
            raise ValueError("colors must be #RRGGBB")
        try:
            int(value[1:], 16)
        except ValueError as exc:
            raise ValueError("colors must be #RRGGBB") from exc
        return value


class AccessorySpec(StrictModel):
    id: str = Field(min_length=1, max_length=40, pattern=r"^[a-z0-9_-]+$")
    slot: Socket
    kind: str = Field(min_length=1, max_length=40)
    description: str = Field(min_length=3, max_length=500)
    target_triangles: int = Field(default=2500, ge=100, le=5000)
    part_schema: list[str] = Field(default_factory=list, max_length=8)


class AbilitySpec(StrictModel):
    id: AbilityId = AbilityId.NONE
    magnitude: float = Field(default=0, ge=0, le=0.35)
    duration_seconds: float = Field(default=0, ge=0, le=8)
    cooldown_seconds: float = Field(default=10, ge=5, le=60)


class CharacterSpec(StrictModel):
    schema_version: str = Field(default="1.0", pattern=r"^1\.0$")
    name: str = Field(min_length=1, max_length=40)
    archetype: Archetype = Archetype.BALANCED
    body_morphs: BodyMorphs = Field(default_factory=BodyMorphs)
    palette: Palette = Field(default_factory=Palette)
    accessories: list[AccessorySpec] = Field(default_factory=list, max_length=4)
    ability: AbilitySpec = Field(default_factory=AbilitySpec)


class CharacterRequest(StrictModel):
    description: str = Field(min_length=3, max_length=1000)
    seed: int = Field(default=0, ge=0, le=2**31 - 1)


class BuildIdentity(StrictModel):
    """Identity of the toolchain that turns a request into a GLB.

    Two requests that are identical in description, seed and sketch still produce
    different bytes if the generator, the compiler script or a base model changed.
    Those inputs are therefore part of the cache key: see `pipeline.request_digest`.
    Everything here must be knowable *before* generation runs, because the digest
    is what decides whether generation runs at all.
    """

    pipeline_version: str = PIPELINE_VERSION
    provider: str
    provider_version: str = UNKNOWN_VERSION
    compiler: str
    compiler_version: str = UNKNOWN_VERSION
    base_models_version: str = UNKNOWN_VERSION

    def digest(self) -> str:
        digest = hashlib.sha256()
        for field in (
            self.pipeline_version,
            self.provider,
            self.provider_version,
            self.compiler,
            self.compiler_version,
            self.base_models_version,
        ):
            digest.update(field.encode("utf-8"))
            digest.update(b"\0")
        return digest.hexdigest()[:12]

    def differences(self, other: BuildIdentity) -> list[str]:
        """Field-by-field diff, for explaining *why* a cached build was rejected."""
        return [
            f"{name}: {getattr(self, name)!r} -> {getattr(other, name)!r}"
            for name in type(self).model_fields
            if getattr(self, name) != getattr(other, name)
        ]


class GeneratedAsset(StrictModel):
    accessory_id: str
    path: Path
    provider: str
    media_type: str
    # Reported by the generator itself (the worker sends `X-Generator-Version`).
    # This is ground truth; `BuildIdentity.provider_version` is only what the
    # orchestrator believed at request time. Recorded so the two can be compared.
    provider_version: str = UNKNOWN_VERSION


class CompiledAsset(StrictModel):
    path: Path
    compiler: str
    media_type: str
    compiler_version: str = UNKNOWN_VERSION


class ValidationMetrics(StrictModel):
    valid: bool
    file_size_bytes: int = Field(ge=0)
    target_triangles: int = Field(ge=0)
    warnings: list[str] = Field(default_factory=list)

    # Populated only when a Blender measurement pass ran. `measured` distinguishes "checked and
    # clean" from "never inspected", which the file-size-only path cannot express.
    measured: bool = False
    accessory_triangles: int | None = Field(default=None, ge=0)
    total_triangles: int | None = Field(default=None, ge=0)
    mesh_count: int | None = Field(default=None, ge=0)
    loose_parts: int | None = Field(default=None, ge=0)
    missing_sockets: list[str] = Field(default_factory=list)
    socket_offset: float | None = Field(default=None, ge=0)


class JobRecord(StrictModel):
    job_id: str
    state: JobState = JobState.QUEUED
    created_at: datetime = Field(default_factory=lambda: datetime.now(UTC))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(UTC))
    request: CharacterRequest
    sketch_path: Path
    spec: CharacterSpec | None = None
    output_path: Path | None = None
    metrics: ValidationMetrics | None = None
    error: str | None = None

    # The toolchain this job was built with. A cache hit is only valid if this
    # still matches the current one; see `api.create_character`.
    build: BuildIdentity | None = None
    # What the generator actually reported at build time, which can differ from
    # `build.provider_version` if the orchestrator was misconfigured.
    observed_provider_version: str | None = None


class CreateCharacterResponse(StrictModel):
    job_id: str
    state: JobState
    status_url: str

