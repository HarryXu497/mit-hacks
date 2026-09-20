"""Derive a character's palette from the drawn sketch rather than its caption.

`RuleBasedSpecProvider` used to pick colours from typed keywords (`if "red" in text`), so a drawn
red jersey produced a default blue one -- the sketch was passed in and never opened. This reads it.

The bases are flat named materials (`MF_Jersey`, `MF_Accent`, `MF_Fur`, `MF_Skin`) driven by hex
colours with toon shading applied on top, so "texturing the monkey" is a palette problem: pick four
colours well and the existing compiler does the rest.

Colours are sampled per *region*, not globally. A whole-image average of a drawing is mud; the
jersey is whatever dominates the torso band, which is a different question from what tops the head.
"""

from __future__ import annotations

import colorsys
from dataclasses import dataclass, field
from pathlib import Path

from monkeyforge.models import Palette

# A drawn figure is roughly vertical. These are fractions of the figure's own bounding box, not of
# the image, so a small drawing in the corner of a big page still lands on the right bands.
HEAD_BAND = (0.00, 0.34)
TORSO_BAND = (0.30, 0.66)
LEG_BAND = (0.62, 1.00)

# Paper and ink are not character colours.
PAPER_MIN_VALUE = 0.90
PAPER_MAX_SATURATION = 0.12
INK_MAX_VALUE = 0.22

# Below this a "colour" is a grey and cannot serve as a jersey.
JERSEY_MIN_SATURATION = 0.22

# Below this a colour is graphite, biro or paper rather than a deliberate choice.
DELIBERATE_MIN_SATURATION = 0.15

# A fill covers real area; an outline does not. Used to stop pencil strokes being read as fur.
FILL_MIN_COVERAGE = 0.12

# Distinct-enough-to-be-the-accent threshold, in RGB distance (0-441).
ACCENT_MIN_DISTANCE = 60.0

QUANTIZE_COLORS = 16


@dataclass
class SkinReading:
    """What was found in the drawing, so the result is explainable and testable."""

    palette: Palette
    confident: bool
    source: str = "sketch"
    notes: list[str] = field(default_factory=list)
    swatches: dict[str, str] = field(default_factory=dict)


def _to_hex(rgb: tuple[int, int, int]) -> str:
    return "#{:02X}{:02X}{:02X}".format(*(max(0, min(255, int(c))) for c in rgb))


def _hsv(rgb: tuple[int, int, int]) -> tuple[float, float, float]:
    r, g, b = (c / 255 for c in rgb)
    return colorsys.rgb_to_hsv(r, g, b)


def _distance(a: tuple[int, int, int], b: tuple[int, int, int]) -> float:
    return sum((x - y) ** 2 for x, y in zip(a, b, strict=True)) ** 0.5


def _distinct(
    a: tuple[int, int, int],
    b: tuple[int, int, int],
    min_hue_degrees: float = 22.0,
    min_value: float = 0.18,
    min_saturation_gap: float = 0.30,
) -> bool:
    """Would a person call these two different colours?

    Raw RGB distance is not usable here: brown fur (#84543C) and cardinal red (#9C243C) sit only 53
    apart in RGB and would be treated as the same colour, yet their hues are 17 deg and 352 deg --
    unmistakably different to a human. So compare hue first, and fall back to brightness or
    saturation for colours too grey to have a meaningful hue.
    """
    hue_a, sat_a, val_a = _hsv(a)
    hue_b, sat_b, val_b = _hsv(b)

    if sat_a >= 0.25 and sat_b >= 0.25:
        gap = abs(hue_a - hue_b) * 360.0
        if min(gap, 360.0 - gap) >= min_hue_degrees:
            return True
    return abs(val_a - val_b) >= min_value or abs(sat_a - sat_b) >= min_saturation_gap


def _is_deliberate(rgb: tuple[int, int, int]) -> bool:
    """Is this a colour someone chose, or an artefact of drawing on paper?

    Graphite (#545454), biro, and the paper itself are all near-grey. A stick figure's head is made
    entirely of those, so without this check the monkey inherits grey fur and a white face from a
    drawing that never said anything about either.
    """
    _, saturation, value = _hsv(rgb)
    return saturation >= DELIBERATE_MIN_SATURATION and value >= 0.12


def _is_paper(rgb: tuple[int, int, int]) -> bool:
    _, s, v = _hsv(rgb)
    return v >= PAPER_MIN_VALUE and s <= PAPER_MAX_SATURATION


def _is_ink(rgb: tuple[int, int, int]) -> bool:
    _, _, v = _hsv(rgb)
    return v <= INK_MAX_VALUE


def _flood_from_border(whiteish, np):
    """Which white pixels are page, not paint: those reachable from the border through white.

    Iterative dilation intersected with the white mask. Cheap, dependency-free, and converges in a
    few dozen passes at sketch resolutions.
    """
    reachable = np.zeros_like(whiteish)
    reachable[0, :] |= whiteish[0, :]
    reachable[-1, :] |= whiteish[-1, :]
    reachable[:, 0] |= whiteish[:, 0]
    reachable[:, -1] |= whiteish[:, -1]

    for _ in range(512):
        grown = reachable.copy()
        grown[1:, :] |= reachable[:-1, :]
        grown[:-1, :] |= reachable[1:, :]
        grown[:, 1:] |= reachable[:, :-1]
        grown[:, :-1] |= reachable[:, 1:]
        grown &= whiteish
        if grown.sum() == reachable.sum():
            break
        reachable = grown
    return reachable


def extract_skin(sketch_path: Path, fallback: Palette | None = None) -> SkinReading:
    """Read a palette out of a drawing.

    Falls back to `fallback` (or defaults) when the image cannot be read or contains no usable
    colour, and says so via `confident`/`source` rather than silently returning something invented.
    """
    base = fallback or Palette()
    try:
        import numpy as np
        from PIL import Image
    except ImportError:
        return SkinReading(
            palette=base,
            confident=False,
            source="fallback",
            notes=["imaging extra not installed: pip install -e .[imaging]"],
        )

    try:
        image = Image.open(sketch_path)
    except Exception as exc:
        return SkinReading(
            palette=base,
            confident=False,
            source="fallback",
            notes=[f"could not open sketch: {type(exc).__name__}"],
        )

    # Composite onto white so transparent PNGs read like paper rather than black.
    image = image.convert("RGBA")
    flat = Image.new("RGBA", image.size, (255, 255, 255, 255))
    flat.alpha_composite(image)
    rgb_image = flat.convert("RGB")

    data = np.asarray(rgb_image, dtype=np.int16)
    height, width = data.shape[:2]

    # Background is white *connected to the edge of the page*, found by flooding inward. Filtering
    # white by brightness alone would discard a white collar or white stripes, which are colour
    # choices, not paper.
    value = data.max(axis=2) / 255.0
    spread = (data.max(axis=2) - data.min(axis=2)) / np.maximum(data.max(axis=2), 1)
    whiteish = (value >= PAPER_MIN_VALUE) & (spread <= PAPER_MAX_SATURATION)
    background = _flood_from_border(whiteish, np)

    is_ink = value <= INK_MAX_VALUE
    figure = ~background & ~is_ink

    if figure.sum() < 0.002 * height * width:
        return SkinReading(
            palette=base,
            confident=False,
            source="fallback",
            notes=["sketch has almost no coloured area; using fallback palette"],
        )

    rows = np.where(figure.any(axis=1))[0]
    top, bottom = int(rows.min()), int(rows.max())
    span = max(bottom - top, 1)

    def band(fraction: tuple[float, float]) -> np.ndarray:
        lo = top + int(fraction[0] * span)
        hi = top + int(fraction[1] * span)
        mask = np.zeros_like(figure)
        mask[lo : max(hi, lo + 1)] = True
        return figure & mask

    def dominant(
        mask: np.ndarray, limit: int = 6, min_coverage: float = 0.0
    ) -> list[tuple[tuple[int, int, int], int]]:
        """Most common quantised colours inside a mask, most frequent first.

        `min_coverage` separates a fill from a line. Pencil outlines are thin and cover a few
        percent of a region; a coloured jersey covers most of it. Saturation cannot make that
        distinction -- blurred graphite picks up enough tint to look like a real colour -- but area
        can.
        """
        pixels = data[mask]
        if pixels.size == 0:
            return []
        # Quantise to a coarse grid so pencil shading and JPEG noise collapse together.
        step = 24
        keys = (pixels // step) * step + step // 2
        packed = (keys[:, 0].astype(np.int32) << 16) | (
            keys[:, 1].astype(np.int32) << 8
        ) | keys[:, 2].astype(np.int32)
        uniq, counts = np.unique(packed, return_counts=True)
        total = int(counts.sum())
        floor = min_coverage * total
        order = np.argsort(-counts)[:limit]
        out = []
        for index in order:
            if counts[index] < floor:
                continue
            key = int(uniq[index])
            out.append((((key >> 16) & 255, (key >> 8) & 255, key & 255), int(counts[index])))
        return out

    notes: list[str] = []
    swatches: dict[str, str] = {}

    # Fur and face must be *filled* areas, so the head band demands coverage. The jersey and its
    # trim are searched without that floor: lettering and a thin collar are legitimately small.
    torso = dominant(band(TORSO_BAND))
    head = dominant(band(HEAD_BAND), min_coverage=FILL_MIN_COVERAGE)
    whole = dominant(figure, limit=8, min_coverage=FILL_MIN_COVERAGE)

    # Jersey: the most saturated colour that actually covers the torso. Fur is brown/tan and
    # desaturated, so saturation is what separates "shirt" from "monkey".
    jersey_rgb = None
    for rgb, _count in torso:
        _, saturation, _ = _hsv(rgb)
        if saturation >= JERSEY_MIN_SATURATION:
            jersey_rgb = rgb
            break
    # Deliberately no fallback to "whatever is in the torso": on uncoloured line art that is the
    # paper showing through the shirt outline, and returning white as a jersey colour is worse than
    # admitting nothing was found. Line art is the diffusion model's job, not the sampler's.
    jersey_found = jersey_rgb is not None
    if not jersey_found and torso:
        notes.append("no colour in the torso band; this looks like uncoloured line art")

    # Fur and face both live in the head band, and which is which is a question of brightness, not
    # of which covers more pixels: a monkey's muzzle is the lighter inner shape, the fur around
    # darker. Picking by pixel count alone gets them backwards whenever the face is drawn large.
    # Only *deliberate* colours count. On a stick figure the head is bare pencil on paper, and
    # neither is a choice: taking them literally gave a monkey grey fur and a white face. A colour
    # has to be saturated enough to be paint rather than graphite before it can override a default.
    head_tones = [
        rgb
        for rgb, _count in head
        if _is_deliberate(rgb) and (jersey_rgb is None or _distinct(rgb, jersey_rgb))
    ]
    fur_rgb = None
    face_rgb = None
    if head_tones:
        by_brightness = sorted(head_tones, key=lambda rgb: _hsv(rgb)[2])
        fur_rgb = by_brightness[0]
        lightest = by_brightness[-1]
        # Only call it a face if it is meaningfully lighter than the fur; a flat-coloured head has
        # no separate muzzle and should keep the default rather than invent one.
        if _hsv(lightest)[2] > _hsv(fur_rgb)[2] + 0.10 and _distinct(lightest, fur_rgb):
            face_rgb = lightest

    if fur_rgb is None:
        for rgb, _count in whole:
            if _is_deliberate(rgb) and (jersey_rgb is None or _distinct(rgb, jersey_rgb)):
                fur_rgb = rgb
                break

    # Accent: trim colour, which lives on the torso (collar, badge) and is distinct from the shirt.
    # It may legitimately be white or near-white, so no saturation floor here.
    # Trim is the one place near-white must be allowed: a white collar or white lettering is a real
    # choice. Restricting the search to the torso band makes that safe -- a filled jersey has
    # no paper showing through, so bright pixels there were put there on purpose.
    accent_rgb = None
    for rgb, _count in torso:
        if not (_is_deliberate(rgb) or _hsv(rgb)[2] >= 0.85):
            continue
        if jersey_rgb is not None and _distinct(rgb, jersey_rgb):
            if fur_rgb is None or _distinct(rgb, fur_rgb):
                accent_rgb = rgb
                break

    resolved = {
        "jersey": jersey_rgb,
        "fur": fur_rgb,
        "face": face_rgb,
        "accent": accent_rgb,
    }
    for key, rgb in resolved.items():
        if rgb is not None:
            swatches[key] = _to_hex(rgb)

    palette = Palette(
        fur=_to_hex(fur_rgb) if fur_rgb else base.fur,
        face=_to_hex(face_rgb) if face_rgb else base.face,
        jersey=_to_hex(jersey_rgb) if jersey_rgb else base.jersey,
        accent=_to_hex(accent_rgb) if accent_rgb else base.accent,
    )

    # The jersey is the colour the user most obviously "chose"; without it this is not a reading.
    confident = jersey_rgb is not None
    if not confident:
        notes.append("could not identify a jersey colour")
    for key in ("fur", "face", "accent"):
        if resolved[key] is None:
            notes.append(f"{key} not found in sketch; kept default")

    return SkinReading(
        palette=palette,
        confident=confident,
        source="sketch" if confident else "partial",
        notes=notes,
        swatches=swatches,
    )

