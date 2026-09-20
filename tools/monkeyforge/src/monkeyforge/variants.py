"""Parameterised procedural accessories for building a preference dataset without a GPU.

The style ranker learns from pairs that differ in quality, so a useful first dataset needs
candidates spanning clearly-good to clearly-broken. These variants are generated deliberately across
that range, including the exact failure modes the geometric features exist to catch: props that
float off their socket, props that fragment into loose islands, and props that blow their triangle
ceiling.

Geometry only. Style and semantics need a real image-to-3D model; `semantic_similarity` stays
neutral until then.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from enum import StrEnum


class Defect(StrEnum):
    NONE = "none"
    FLOATING = "floating"
    FRAGMENTED = "fragmented"
    OVER_BUDGET = "over_budget"
    DEGENERATE = "degenerate"


class Shape(StrEnum):
    DOME = "dome"
    WEDGE = "wedge"
    DRUM = "drum"


@dataclass
class VariantSpec:
    """A single procedural accessory to generate."""

    variant_id: str
    shape: Shape = Shape.DOME
    defect: Defect = Defect.NONE
    segments: int = 10
    rings: int = 4
    radius: float = 0.50
    height: float = 0.66
    taper: float = 0.35
    crest: bool = True
    notes: list[str] = field(default_factory=list)


Vertex = tuple[float, float, float]
Triangle = tuple[int, int, int]


def _ring_radius(shape: Shape, base_radius: float, taper: float, t: float) -> float:
    """Radius at normalised height `t` along the prop."""
    if shape is Shape.DOME:
        return base_radius * (1.0 - taper * t**1.6)
    if shape is Shape.WEDGE:
        return base_radius * (1.0 - taper * t)
    return base_radius * (1.0 - 0.15 * taper * t)


def build_variant(spec: VariantSpec) -> tuple[list[Vertex], list[Triangle]]:
    """Build the accessory mesh for a variant, applying its defect if any."""
    segments = max(3, spec.segments)
    rings = max(2, spec.rings)

    if spec.defect is Defect.OVER_BUDGET:
        segments = max(segments, 64)
        rings = max(rings, 24)
    elif spec.defect is Defect.DEGENERATE:
        segments, rings = 3, 2

    vertices: list[Vertex] = []
    for ring in range(rings):
        t = ring / (rings - 1)
        z = spec.height * t
        radius = _ring_radius(spec.shape, spec.radius, spec.taper, t)
        for index in range(segments):
            angle = math.tau * index / segments
            vertices.append((radius * math.cos(angle), radius * math.sin(angle), z))

    triangles: list[Triangle] = []
    for ring in range(rings - 1):
        lower = ring * segments
        upper = (ring + 1) * segments
        for index in range(segments):
            nxt = (index + 1) % segments
            triangles.append((lower + index, lower + nxt, upper + nxt))
            triangles.append((lower + index, upper + nxt, upper + index))

    apex = len(vertices)
    vertices.append((0.0, 0.0, spec.height))
    top = (rings - 1) * segments
    for index in range(segments):
        nxt = (index + 1) % segments
        triangles.append((top + index, top + nxt, apex))

    if spec.crest and spec.defect is not Defect.DEGENERATE:
        vertices, triangles = _add_crest(vertices, triangles, spec)

    if spec.defect is Defect.FRAGMENTED:
        vertices, triangles = _fragment(vertices, triangles, spec)
    elif spec.defect is Defect.FLOATING:
        # Lift the whole prop clear of its socket so the bounding box no longer contains it.
        lift = spec.height * 1.8
        vertices = [(x, y, z + lift) for x, y, z in vertices]

    return vertices, triangles


def _add_crest(
    vertices: list[Vertex], triangles: list[Triangle], spec: VariantSpec
) -> tuple[list[Vertex], list[Triangle]]:
    """A small separate knob on top; a legitimate second island."""
    base_index = len(vertices)
    crest_radius = spec.radius * 0.2
    crest_base = spec.height * 0.98
    for index in range(6):
        angle = math.tau * index / 6
        vertices.append(
            (crest_radius * math.cos(angle), crest_radius * math.sin(angle), crest_base)
        )
    tip = len(vertices)
    vertices.append((0.0, 0.0, spec.height * 1.25))
    for index in range(6):
        nxt = (index + 1) % 6
        triangles.append((base_index + index, base_index + nxt, tip))
    return vertices, triangles


def _fragment(
    vertices: list[Vertex], triangles: list[Triangle], spec: VariantSpec
) -> tuple[list[Vertex], list[Triangle]]:
    """Scatter extra disconnected shards so the prop reads as broken."""
    shards = 5
    for shard in range(shards):
        angle = math.tau * shard / shards
        cx = spec.radius * 1.5 * math.cos(angle)
        cy = spec.radius * 1.5 * math.sin(angle)
        cz = spec.height * (0.3 + 0.12 * shard)
        size = spec.radius * 0.16
        start = len(vertices)
        vertices.extend(
            [
                (cx - size, cy - size, cz),
                (cx + size, cy - size, cz),
                (cx, cy + size, cz + size),
            ]
        )
        triangles.append((start, start + 1, start + 2))
    return vertices, triangles


def default_catalogue() -> list[tuple[str, str, list[VariantSpec]]]:
    """Prompts paired with a spread of variants, good through broken.

    Returned as (prompt, accessory_kind, variants). Each prompt gets several candidates so
    `review_pairs.py plan` has pairs to work with.
    """
    catalogue: list[tuple[str, str, list[VariantSpec]]] = []

    presets = [
        (
            "rounded goalkeeper helmet with a bold crest",
            "hat",
            [
                VariantSpec("clean_dome", Shape.DOME, Defect.NONE, segments=12, rings=4),
                VariantSpec("tall_wedge", Shape.WEDGE, Defect.NONE, segments=10, rings=5,
                            height=0.82, taper=0.5),
                VariantSpec("squat_drum", Shape.DRUM, Defect.NONE, segments=14, rings=3,
                            height=0.44, radius=0.56),
                VariantSpec("floating", Shape.DOME, Defect.FLOATING, segments=12, rings=4,
                            notes=["lifted clear of the socket"]),
                VariantSpec("shattered", Shape.DOME, Defect.FRAGMENTED, segments=12, rings=4,
                            notes=["loose shards"]),
                VariantSpec("dense", Shape.DOME, Defect.OVER_BUDGET, notes=["blows the ceiling"]),
            ],
        ),
        (
            "compact wind-powered backpack",
            "backpack",
            [
                VariantSpec("clean_drum", Shape.DRUM, Defect.NONE, segments=12, rings=4,
                            radius=0.42, height=0.58, crest=False),
                VariantSpec("tapered", Shape.WEDGE, Defect.NONE, segments=10, rings=4,
                            radius=0.46, height=0.62, taper=0.55, crest=False),
                VariantSpec("floating", Shape.DRUM, Defect.FLOATING, segments=12, rings=4,
                            radius=0.42, height=0.58, crest=False,
                            notes=["lifted clear of the socket"]),
                VariantSpec("shattered", Shape.DRUM, Defect.FRAGMENTED, segments=12, rings=4,
                            radius=0.42, height=0.58, crest=False, notes=["loose shards"]),
            ],
        ),
        (
            "defender shoulder shield",
            "shield",
            [
                VariantSpec("clean_wedge", Shape.WEDGE, Defect.NONE, segments=10, rings=4,
                            radius=0.48, height=0.5, crest=False),
                VariantSpec("broad", Shape.DRUM, Defect.NONE, segments=16, rings=3,
                            radius=0.58, height=0.4, crest=False),
                VariantSpec("sliver", Shape.WEDGE, Defect.DEGENERATE, crest=False,
                            notes=["near-empty mesh"]),
                VariantSpec("shattered", Shape.WEDGE, Defect.FRAGMENTED, segments=10, rings=4,
                            radius=0.48, height=0.5, crest=False, notes=["loose shards"]),
            ],
        ),
    ]

    for prompt, kind, variants in presets:
        catalogue.append((prompt, kind, variants))
    return catalogue
