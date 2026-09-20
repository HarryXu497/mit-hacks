"""Turn measurements and renders into style-ranker features.

Every score here is deterministic and reproducible from artifacts on disk. ``semantic_similarity``
is deliberately absent: it needs a frozen visual encoder and is supplied by the caller, defaulting
to a neutral 0.5 so a dataset can be built from the measurable features first.
"""

from __future__ import annotations

import math
from pathlib import Path

from monkeyforge.measurements import AccessoryMeasurement, GlbMeasurement
from monkeyforge.models import AccessorySpec, Palette
from monkeyforge.ranking import CandidateFeatures

NEUTRAL_SEMANTIC_SIMILARITY = 0.5

# `target_triangles` is a ceiling, not a quota: a prop that looks right with fewer triangles is
# better, not worse. Degeneracy is therefore an absolute floor rather than a fraction of the
# ceiling, so an efficient 96-triangle helmet is not punished for missing a 2500 budget it was
# never meant to spend.
MINIMUM_VIABLE_TRIANGLES = 50


def _clamp(value: float) -> float:
    return max(0.0, min(1.0, value))


def triangle_budget_score(triangles: int, target_triangles: int) -> float:
    """Full marks anywhere within budget; overruns decay, near-empty meshes are flagged.

    Treats `target_triangles` as a ceiling. Coming in under it is efficiency, not a defect, so the
    only penalties are exceeding the ceiling and falling below an absolute viability floor.
    """
    if target_triangles <= 0 or triangles <= 0:
        return 0.0
    if triangles < MINIMUM_VIABLE_TRIANGLES:
        return _clamp(triangles / MINIMUM_VIABLE_TRIANGLES)
    ratio = triangles / target_triangles
    if ratio > 1.0:
        return _clamp(1.0 / ratio)
    return 1.0


def disconnected_penalty(loose_parts: int, expected_parts: int) -> float:
    """0 when the mesh has no more islands than its part schema promises, rising as it fragments."""
    expected = max(1, expected_parts)
    if loose_parts <= expected:
        return 0.0
    return _clamp((loose_parts - expected) / max(4, expected * 2))


def socket_fit_score(accessory: AccessoryMeasurement) -> float:
    """How well an accessory is seated on its socket.

    Measured as the gap between the socket point and the accessory's bounding box, normalised by
    the accessory's own diagonal. A seated prop scores 1.0 whatever direction it extends in, so a
    hat is not penalised for sitting above its head socket; one floating its own length away is 0.
    """
    diagonal = math.sqrt(sum(axis**2 for axis in accessory.extent)) or 1e-6
    return _clamp(1.0 - accessory.socket_gap / diagonal)


def geometric_features(
    measurement: GlbMeasurement,
    accessory: AccessorySpec,
) -> dict[str, float]:
    """The three features derivable from geometry alone, with no render and no model."""
    attached = measurement.accessory
    if attached is None:
        # Nothing resolved at the socket: the accessory is absent or detached.
        return {
            "socket_fit_score": 0.0,
            "triangle_budget_score": 0.0,
            "disconnected_penalty": 1.0,
        }
    return {
        "socket_fit_score": socket_fit_score(attached),
        "triangle_budget_score": triangle_budget_score(
            attached.triangles, accessory.target_triangles
        ),
        "disconnected_penalty": disconnected_penalty(
            attached.loose_parts, len(accessory.part_schema)
        ),
    }


def _hex_to_rgb(value: str) -> tuple[int, int, int]:
    return tuple(int(value[index : index + 2], 16) for index in (1, 3, 5))  # type: ignore[return-value]


# How far a pixel must sit from the inferred background colour to count as subject, in 0-255 RGB
# distance. Only used for renders that carry no usable alpha channel.
BACKGROUND_TOLERANCE = 40.0


def _subject_mask(data, np):
    """Separate subject from background, by alpha where available and by colour where not.

    `blender_render_preview.py` sets `film_transparent = False`, so the review pack renders are
    fully opaque and an alpha mask would select the entire frame. Fall back to inferring the
    background from the border ring, which the fixed-camera setup keeps clear of the subject.
    """
    alpha = data[..., 3]
    if alpha.min() < 250:
        return alpha > 127

    border = np.concatenate(
        [data[0, :, :3], data[-1, :, :3], data[:, 0, :3], data[:, -1, :3]]
    )
    background = np.median(border, axis=0)
    distance = np.linalg.norm(data[..., :3] - background, axis=2)
    return distance > BACKGROUND_TOLERANCE


def image_features(render_path: Path, palette: Palette) -> dict[str, float]:
    """Silhouette readability and palette adherence, measured from a rendered view.

    Requires the ``imaging`` extra (numpy, pillow).
    """
    try:
        import numpy as np
        from PIL import Image
    except ImportError as exc:  # pragma: no cover - depends on optional extra
        raise ImportError(
            "image features need the imaging extra: pip install -e .[imaging]"
        ) from exc

    image = Image.open(render_path).convert("RGBA")
    data = np.asarray(image, dtype=np.float64)
    mask = _subject_mask(data, np)

    if not mask.any() or mask.all():
        # Either nothing rendered, or the subject could not be separated from its background.
        return {"silhouette_score": 0.0, "palette_score": 0.0}

    # Silhouette: a readable low-poly prop is a solid, compact shape that fills a sensible share of
    # frame. Combine coverage against an ideal share with an isoperimetric compactness ratio.
    coverage = float(mask.mean())
    ideal_coverage = 0.25
    coverage_score = _clamp(1.0 - abs(coverage - ideal_coverage) / ideal_coverage)

    interior = (
        mask[1:-1, 1:-1]
        & mask[:-2, 1:-1]
        & mask[2:, 1:-1]
        & mask[1:-1, :-2]
        & mask[1:-1, 2:]
    )
    area = float(mask.sum())
    perimeter = float(mask[1:-1, 1:-1].sum() - interior.sum())
    compactness = _clamp(4 * np.pi * area / (perimeter**2)) if perimeter > 0 else 0.0

    silhouette = _clamp(0.5 * coverage_score + 0.5 * compactness)

    # Palette: how close subject pixels sit to the nearest colour the spec actually asked for.
    subject = data[mask][..., :3]
    palette_colours = (palette.fur, palette.face, palette.jersey, palette.accent)
    palette_rgb = np.array(
        [_hex_to_rgb(colour) for colour in palette_colours],
        dtype=np.float64,
    )
    distances = np.linalg.norm(subject[:, None, :] - palette_rgb[None, :, :], axis=2)
    nearest = distances.min(axis=1)
    max_distance = np.sqrt(3 * 255**2)
    palette_score = _clamp(1.0 - float(nearest.mean()) / max_distance)

    return {"silhouette_score": float(silhouette), "palette_score": float(palette_score)}


def extract_features(
    measurement: GlbMeasurement,
    accessory: AccessorySpec,
    render_path: Path | None = None,
    palette: Palette | None = None,
    semantic_similarity: float = NEUTRAL_SEMANTIC_SIMILARITY,
) -> CandidateFeatures:
    """Build a full feature vector, filling unmeasurable dimensions with neutral values."""
    values: dict[str, float] = {
        "semantic_similarity": _clamp(semantic_similarity),
        "silhouette_score": 0.5,
        "palette_score": 0.5,
    }
    values.update(geometric_features(measurement, accessory))

    if render_path is not None and render_path.exists():
        values.update(image_features(render_path, palette or Palette()))

    return CandidateFeatures(**values)
