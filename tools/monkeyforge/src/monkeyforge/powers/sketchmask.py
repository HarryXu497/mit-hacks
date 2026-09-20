"""Turn a player's drawing into the two images the beautifier needs.

`line_art` is the ControlNet conditioning image; `subject_mask` is the alpha used to
lift the generated subject off its background.

Both deliberately use only Pillow and numpy. OpenCV would be the obvious tool, but it
is a heavy dependency for four morphological operations, and keeping it out means this
module installs and tests in the lightweight API environment rather than only on the
GPU box.

The masking idea is the important part. Salient-object segmentation alone is
unreliable here: when SDXL fills the frame with clutter, RMBG-2.0 returns nearly the
whole canvas and the icon ends up showing a square patch of background. But we already
know the silhouette -- the player drew it, and ControlNet reproduces those strokes
closely. Thickening the drawing and filling what it encloses therefore gives a mask
that cannot fail in that way, and it enforces the product promise directly: the icon
keeps *your* shape.
"""

from __future__ import annotations

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

# How far to grow the drawn stroke, as a fraction of the image width. A pencil line is
# a couple of pixels; the chunky outline SDXL renders in its place is far thicker, and
# a mask that only covers the pencil line crops the icon's own border away.
STROKE_GROWTH = 0.017

# Closing bridges the gaps a hand-drawn outline leaves behind, so the flood fill cannot
# leak out of a shape the player meant to be closed. Needs to exceed the largest gap.
GAP_BRIDGE = 2.2


def _odd(value: float, minimum: int = 3) -> int:
    """Pillow's rank filters need an odd window of at least 3."""
    size = max(minimum, int(round(value)))
    return size if size % 2 else size + 1


def otsu_threshold(gray: np.ndarray) -> int:
    """The grey level that best separates ink from paper.

    Chosen rather than a fixed threshold so a faint pencil scan and a hard marker both
    binarise correctly without anyone tuning a number per drawing.
    """
    histogram = np.bincount(gray.ravel(), minlength=256).astype(np.float64)
    levels = np.arange(256, dtype=np.float64)
    weight_background = np.cumsum(histogram)
    weight_foreground = gray.size - weight_background
    cumulative = np.cumsum(levels * histogram)
    total = cumulative[-1]

    valid = (weight_background > 0) & (weight_foreground > 0)
    if not valid.any():
        return 127
    mean_background = np.divide(cumulative, weight_background, where=valid, out=np.zeros(256))
    mean_foreground = np.divide(
        total - cumulative, weight_foreground, where=valid, out=np.zeros(256)
    )
    variance = weight_background * weight_foreground * (mean_background - mean_foreground) ** 2
    return int(np.argmax(np.where(valid, variance, -1.0)))


def _ink(sketch: Image.Image, size: int) -> Image.Image:
    """Binarise to white ink on black, at `size` square."""
    gray = np.asarray(sketch.convert("L").resize((size, size), Image.LANCZOS))
    binary = (gray <= otsu_threshold(gray)).astype(np.uint8) * 255
    return Image.fromarray(binary)


def line_art(sketch: Image.Image | str, size: int = 1024) -> Image.Image:
    """White strokes on black -- the conditioning image the canny ControlNet expects.

    Running Canny over a drawing would return both sides of every stroke as separate
    edges and double the geometry, because a drawing is already an edge map.
    """
    image = Image.open(sketch) if isinstance(sketch, str) else sketch
    ink = _ink(image, size)
    return ink.filter(ImageFilter.MaxFilter(3)).convert("RGB")


def subject_mask(
    sketch: Image.Image | str,
    size: int = 1024,
    growth: float = STROKE_GROWTH,
    feather: int = 4,
) -> Image.Image:
    """An 8-bit alpha covering the drawn strokes and everything they enclose."""
    image = Image.open(sketch) if isinstance(sketch, str) else sketch
    ink = _ink(image, size)

    grow = _odd(size * growth)
    thick = ink.filter(ImageFilter.MaxFilter(grow))
    # Closing = dilate then erode: bridges gaps without permanently fattening the shape.
    bridge = _odd(grow * GAP_BRIDGE)
    closed = thick.filter(ImageFilter.MaxFilter(bridge)).filter(ImageFilter.MinFilter(bridge))

    # Flood the outside from a corner; whatever the flood cannot reach is enclosed by
    # the drawing and therefore belongs to the subject.
    outside = closed.copy()
    ImageDraw.floodfill(outside, (0, 0), 128)
    filled = np.asarray(closed).copy()
    filled[np.asarray(outside) != 128] = 255

    mask = Image.fromarray(filled)
    return mask.filter(ImageFilter.GaussianBlur(feather)) if feather else mask


def apply_mask(image: Image.Image, mask: Image.Image) -> Image.Image:
    """Put `mask` in `image`'s alpha channel and crop to what survives."""
    out = image.convert("RGBA")
    out.putalpha(mask.convert("L").resize(out.size, Image.LANCZOS))
    return trim(out)


def trim(image: Image.Image, threshold: int = 12) -> Image.Image:
    """Crop away fully transparent margins so the badge decides the framing."""
    alpha = np.asarray(image.convert("RGBA"))[..., 3]
    rows = np.where(alpha.max(axis=1) > threshold)[0]
    columns = np.where(alpha.max(axis=0) > threshold)[0]
    if not rows.size or not columns.size:
        return image
    return image.crop((columns[0], rows[0], columns[-1] + 1, rows[-1] + 1))
