"""Composite a subject into a fixed, procedurally drawn badge frame.

The BTD6 icon style is unusually templated: one circular badge, one metallic rim, one
lighting angle, subject centred, glossy highlight top-left. Because the frame is
near-identical every time, generating the whole icon is wasted effort and the main
source of ugly failures -- a warped rim reads as broken in a way a slightly-off subject
does not. So the model only ever produces the subject, and this module supplies the
frame.

The frame is **drawn, not copied**. Compositing a cropped BTD6 rim into every output
would put Ninja Kiwi's art in the shipped product; `docs/HANDOFF.md` is explicit that
the reference material stays local and private. A ring built from gradients is
team-owned, free, and tunable, and it gives byte-identical framing across every icon.

Antialiasing comes from signed-distance masks rather than supersampling the whole
canvas, so a 512px badge renders in one pass at full quality.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

import numpy as np
from PIL import Image, ImageDraw

from monkeyforge.powers.registry import Power, PowerId, by_id

# Where the light comes from, in degrees clockwise from straight up. Every gradient and
# highlight in the badge uses this one value; that shared angle is most of what makes a
# set of icons look like a set.
LIGHT_ANGLE_DEG = 315.0


@dataclass(frozen=True)
class BadgeStyle:
    """Proportions and palette of the frame. All lengths are fractions of the canvas."""

    size: int = 512
    outer_radius: float = 0.484
    outline_width: float = 0.020
    ring_width: float = 0.108
    subject_inset: float = 0.78
    gloss_opacity: float = 0.30
    outline_rgb: tuple[int, int, int] = (26, 32, 43)
    steel_light: tuple[int, int, int] = (222, 236, 247)
    steel_dark: tuple[int, int, int] = (86, 112, 134)
    backdrop_lift: float = 0.42
    backdrop_shade: float = 0.45
    glyph_rgb: tuple[int, int, int] = (255, 255, 255)


@dataclass(frozen=True)
class BadgeGeometry:
    """Pixel geometry of one badge, so callers can place a subject without guessing."""

    size: int
    centre: float
    outer_radius: float
    ring_inner_radius: float
    subject_radius: float

    @property
    def subject_box(self) -> tuple[int, int, int, int]:
        low = int(round(self.centre - self.subject_radius))
        high = int(round(self.centre + self.subject_radius))
        return low, low, high, high


def geometry(style: BadgeStyle = BadgeStyle()) -> BadgeGeometry:
    size = style.size
    outer = style.outer_radius * size
    inner = outer - style.outline_width * size - style.ring_width * size
    return BadgeGeometry(
        size=size,
        centre=(size - 1) / 2.0,
        outer_radius=outer,
        ring_inner_radius=inner,
        subject_radius=inner * style.subject_inset,
    )


def _hex_to_rgb(value: str) -> tuple[int, int, int]:
    """Parse #RRGGBB. Deliberately as strict as `models.Palette.validate_hex`."""
    if len(value) != 7 or not value.startswith("#"):
        raise ValueError(f"expected #RRGGBB, got {value!r}")
    try:
        channels = tuple(int(value[i : i + 2], 16) for i in (1, 3, 5))
    except ValueError as exc:
        raise ValueError(f"expected #RRGGBB, got {value!r}") from exc
    return channels  # type: ignore[return-value]


def _shift(rgb: tuple[int, int, int], amount: float) -> np.ndarray:
    """Lighten (amount > 0) or darken (amount < 0) towards white/black."""
    base = np.asarray(rgb, dtype=np.float64)
    target = 255.0 if amount > 0 else 0.0
    return base + (target - base) * abs(amount)


def _radius_field(size: int) -> np.ndarray:
    axis = np.arange(size, dtype=np.float64) - (size - 1) / 2.0
    return np.hypot(*np.meshgrid(axis, axis, indexing="xy"))


def _disc(radius_field: np.ndarray, radius: float) -> np.ndarray:
    """Soft-edged filled circle: 1 inside, 0 outside, one pixel of antialiasing."""
    return np.clip(radius + 0.5 - radius_field, 0.0, 1.0)


def _light_ramp(size: int, angle_deg: float = LIGHT_ANGLE_DEG) -> np.ndarray:
    """0 on the lit side, 1 on the shaded side, linear across the canvas."""
    theta = math.radians(angle_deg)
    axis = np.linspace(-1.0, 1.0, size)
    x, y = np.meshgrid(axis, axis, indexing="xy")
    projected = x * math.sin(theta) - y * math.cos(theta)
    return np.clip((1.0 - projected) / 2.0, 0.0, 1.0)


def _blend(base: np.ndarray, colour: np.ndarray, mask: np.ndarray) -> np.ndarray:
    """Alpha-composite a flat or per-pixel colour onto an RGB float canvas."""
    return base * (1.0 - mask[..., None]) + colour * mask[..., None]


def render_frame(
    accent: str, style: BadgeStyle = BadgeStyle()
) -> tuple[Image.Image, BadgeGeometry]:
    """The empty badge: dark outline, metallic ring, tinted backdrop, gloss.

    Returned separately from `render_badge` so the frame can be rendered once and reused
    across many subjects, and so a caller can inspect it without a subject at all.
    """
    geo = geometry(style)
    size = style.size
    radius = _radius_field(size)
    ramp = _light_ramp(size)
    accent_rgb = _hex_to_rgb(accent)

    outer = _disc(radius, geo.outer_radius)
    ring_outer = _disc(radius, geo.outer_radius - style.outline_width * size)
    inner_outline = _disc(radius, geo.ring_inner_radius + style.outline_width * size)
    inner = _disc(radius, geo.ring_inner_radius)

    steel = _blend(
        np.broadcast_to(
            np.asarray(style.steel_light, dtype=np.float64), (size, size, 3)
        ).copy(),
        np.asarray(style.steel_dark, dtype=np.float64),
        ramp,
    )
    backdrop = _blend(
        np.broadcast_to(_shift(accent_rgb, style.backdrop_lift), (size, size, 3)).copy(),
        _shift(accent_rgb, -style.backdrop_shade),
        # Squaring pulls the bright centre inwards, which reads as a spotlit subject
        # rather than a flat wash.
        np.clip(radius / max(geo.ring_inner_radius, 1e-6), 0.0, 1.0) ** 2,
    )

    canvas = np.zeros((size, size, 3), dtype=np.float64)
    outline = np.asarray(style.outline_rgb, dtype=np.float64)
    canvas = _blend(canvas, outline, outer)
    canvas = _blend(canvas, steel, ring_outer)
    canvas = _blend(canvas, outline, inner_outline)
    canvas = _blend(canvas, backdrop, inner)

    # A specular arc along the lit edge of the ring, and a broad gloss over the
    # backdrop. Both are keyed to LIGHT_ANGLE_DEG so they agree with the steel gradient.
    rim = np.clip(ring_outer - inner_outline, 0.0, 1.0) * np.clip(1.0 - ramp * 2.0, 0.0, 1.0)
    canvas = _blend(canvas, np.full(3, 255.0), rim * 0.55)

    gloss_centre = np.asarray([-0.34, -0.40]) * geo.ring_inner_radius
    axis = np.arange(size, dtype=np.float64) - (size - 1) / 2.0
    gx, gy = np.meshgrid(axis - gloss_centre[0], axis - gloss_centre[1], indexing="xy")
    gloss_dist = np.hypot(gx / (geo.ring_inner_radius * 0.92), gy / (geo.ring_inner_radius * 0.62))
    gloss = np.clip(1.0 - gloss_dist, 0.0, 1.0) ** 1.6 * inner
    canvas = _blend(canvas, np.full(3, 255.0), gloss * style.gloss_opacity)

    rgba = np.dstack([np.clip(canvas, 0, 255), np.clip(outer * 255.0, 0, 255)]).astype(np.uint8)
    return Image.fromarray(rgba), geo


def render_badge(
    subject: Image.Image | None,
    accent: str,
    style: BadgeStyle = BadgeStyle(),
) -> Image.Image:
    """Frame plus subject, with the subject clipped to the inner circle.

    The subject is scaled to fit `subject_inset` of the inner disc and centred. Clipping
    is what makes a bad generation degrade gracefully: a subject that overruns its box
    is cropped to the badge rather than breaking the silhouette.
    """
    frame, geo = render_frame(accent, style)
    if subject is None:
        return frame

    box = int(round(geo.subject_radius * 2))
    fitted = _fit(subject.convert("RGBA"), box)

    layer = Image.new("RGBA", frame.size, (0, 0, 0, 0))
    left = int(round(geo.centre - fitted.width / 2))
    top = int(round(geo.centre - fitted.height / 2))
    layer.paste(fitted, (left, top), fitted)

    radius = _radius_field(style.size)
    clip = _disc(radius, geo.ring_inner_radius)
    data = np.asarray(layer, dtype=np.float64)
    data[..., 3] *= clip
    layer = Image.fromarray(data.astype(np.uint8))

    return Image.alpha_composite(frame, layer)


def _fit(image: Image.Image, box: int) -> Image.Image:
    """Scale to fit inside a square of `box` pixels, preserving aspect ratio."""
    scale = min(box / image.width, box / image.height)
    width = max(1, int(round(image.width * scale)))
    height = max(1, int(round(image.height * scale)))
    return image.resize((width, height), Image.LANCZOS)


# --- placeholder subjects -------------------------------------------------------
#
# Flat, deterministic, drawn from polygons. Their job is to prove the compositing path
# end to end before any model exists, and to remain as the fallback when generation
# fails -- a plain white bolt in a correct badge is a worse icon but never a broken one.

_SUPERSAMPLE = 4


@dataclass(frozen=True)
class _Glyph:
    polygons: tuple[tuple[tuple[float, float], ...], ...] = ()
    strokes: tuple[tuple[tuple[float, float], ...], ...] = ()
    stroke_width: float = 0.16


def _star(points: int, inner: float, rotation: float = -90.0) -> tuple[tuple[float, float], ...]:
    step = 180.0 / points
    return tuple(
        (
            math.cos(math.radians(rotation + i * step)) * (1.0 if i % 2 == 0 else inner),
            math.sin(math.radians(rotation + i * step)) * (1.0 if i % 2 == 0 else inner),
        )
        for i in range(points * 2)
    )


def _snowflake() -> tuple[tuple[tuple[float, float], ...], ...]:
    strokes: list[tuple[tuple[float, float], ...]] = []
    for spoke in range(6):
        angle = math.radians(spoke * 60.0)
        tip = (math.cos(angle), math.sin(angle))
        strokes.append(((-tip[0] * 0.04, -tip[1] * 0.04), tip))
        for side in (+1, -1):
            barb = math.radians(spoke * 60.0 + side * 40.0)
            root = (math.cos(angle) * 0.52, math.sin(angle) * 0.52)
            strokes.append(
                (root, (root[0] + math.cos(barb) * 0.36, root[1] + math.sin(barb) * 0.36))
            )
    return tuple(strokes)


_GLYPHS: dict[PowerId, _Glyph] = {
    PowerId.BEAM_BLAST: _Glyph(polygons=(_star(8, 0.40),)),
    PowerId.FREEZE_RAY: _Glyph(strokes=_snowflake(), stroke_width=0.13),
    PowerId.BOOST: _Glyph(
        polygons=(
            (
                (0.26, -1.0),
                (-0.62, 0.14),
                (-0.06, 0.14),
                (-0.30, 1.0),
                (0.62, -0.18),
                (0.06, -0.18),
            ),
        )
    ),
    PowerId.SLOW: _Glyph(
        polygons=(
            (
                (-0.64, -0.88),
                (0.64, -0.88),
                (0.64, -0.70),
                (0.13, 0.0),
                (0.64, 0.70),
                (0.64, 0.88),
                (-0.64, 0.88),
                (-0.64, 0.70),
                (-0.13, 0.0),
                (-0.64, -0.70),
            ),
        )
    ),
}


def placeholder_subject(
    power_id: PowerId | str,
    size: int = 512,
    style: BadgeStyle = BadgeStyle(),
) -> Image.Image:
    """A flat white glyph with a dark outline, on transparency."""
    power = by_id(power_id)
    glyph = _GLYPHS[power.id]
    work = size * _SUPERSAMPLE
    image = Image.new("RGBA", (work, work), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    half = work / 2.0
    outline_px = max(1, int(round(work * 0.022)))

    def place(point: tuple[float, float]) -> tuple[float, float]:
        return half + point[0] * half * 0.92, half + point[1] * half * 0.92

    for polygon in glyph.polygons:
        draw.polygon(
            [place(point) for point in polygon],
            fill=(*style.glyph_rgb, 255),
            outline=(*style.outline_rgb, 255),
            width=outline_px,
        )
    for stroke in glyph.strokes:
        points = [place(point) for point in stroke]
        draw.line(points, fill=(*style.outline_rgb, 255), width=int(work * glyph.stroke_width))
        draw.line(
            points,
            fill=(*style.glyph_rgb, 255),
            width=max(1, int(work * glyph.stroke_width) - outline_px * 2),
        )

    return image.resize((size, size), Image.LANCZOS)


def placeholder_badge(power_id: PowerId | str, style: BadgeStyle = BadgeStyle()) -> Image.Image:
    """The complete deterministic icon for a power, with no model involved."""
    power: Power = by_id(power_id)
    subject = placeholder_subject(power.id, size=style.size, style=style)
    return render_badge(subject, power.accent, style)
