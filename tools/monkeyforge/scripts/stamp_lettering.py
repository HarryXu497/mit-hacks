"""Composite lettering onto a garment sprite as real typography.

Measured, not assumed: SDXL was asked for a maroon shirt with grey "MIT" across five seeds and
produced a Greek capital pi, "nlit", "MIT-MIT", and two fields of tiled letter fragments. SD 1.5
was worse, giving "3I", "XJZ" and "VU". Diffusion models do not spell, and prompting harder does
not fix it.

Rendering the letters with a font instead is exact every time, costs milliseconds, and keeps the
letterforms crisp at any texture resolution.

Placement is derived from the garment's own silhouette rather than hardcoded, so the same code
works for a shirt, a cap or a shield.

Run:
    python scripts/stamp_lettering.py --garment <png> --text MIT --output <png>
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

FONT_CANDIDATES = (
    "arialbd.ttf", "Arial_Bold.ttf", "ariblk.ttf",
    "DejaVuSans-Bold.ttf", "seguisb.ttf", "arial.ttf",
)


def parse_hex(value: str) -> tuple[int, int, int]:
    value = value.lstrip("#")
    return tuple(int(value[i : i + 2], 16) for i in (0, 2, 4))  # type: ignore[return-value]


def subject_mask(image: Image.Image, tolerance: int = 24) -> np.ndarray:
    """Where the garment is: its alpha if it has one, else everything unlike the border colour."""
    rgba = image.convert("RGBA")
    alpha = np.asarray(rgba)[..., 3]
    if alpha.min() < 250:
        return alpha > 40
    rgb = np.asarray(rgba.convert("RGB"), dtype=np.int16)
    border = np.concatenate([rgb[0], rgb[-1], rgb[:, 0], rgb[:, -1]])
    background = np.median(border, axis=0)
    return np.linalg.norm(rgb - background, axis=2) > tolerance


def widest_band(mask: np.ndarray, lo: float, hi: float) -> tuple[int, int, int]:
    """Centre and width of the garment across a horizontal band of its height.

    A chest logo sits on the broadest part of the front panel; a cap's letters sit on its crown.
    Measuring the silhouette finds both without special-casing either.
    """
    rows = np.where(mask.any(axis=1))[0]
    top, bottom = int(rows.min()), int(rows.max())
    span = max(bottom - top, 1)
    band = mask[top + int(lo * span) : top + int(hi * span)]
    if not band.any():
        band = mask
    cols = np.where(band.any(axis=0))[0]
    centre_x = int((cols.min() + cols.max()) / 2)
    width = int(cols.max() - cols.min())
    centre_y = top + int((lo + hi) / 2 * span)
    return centre_x, centre_y, width


def fit_font(draw: ImageDraw.ImageDraw, text: str, target_width: int) -> ImageFont.FreeTypeFont:
    """Largest font whose rendered width fits the target."""
    for candidate in FONT_CANDIDATES:
        try:
            ImageFont.truetype(candidate, 12)
        except OSError:
            continue
        best = None
        for point in range(8, 600, 2):
            font = ImageFont.truetype(candidate, point)
            if draw.textlength(text, font=font) > target_width:
                break
            best = font
        if best is not None:
            return best
    return ImageFont.load_default()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--garment", type=Path, required=True)
    parser.add_argument("--text", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--colour", default="#C2C0BF")
    parser.add_argument("--outline", default="#2A2024")
    parser.add_argument("--band", type=float, nargs=2, default=[0.34, 0.62],
                        help="height fractions of the garment to centre the text within")
    parser.add_argument("--width-fraction", type=float, default=0.56)
    args = parser.parse_args()

    garment = Image.open(args.garment).convert("RGBA")
    mask = subject_mask(garment)
    if not mask.any():
        raise SystemExit("garment sprite appears empty")

    centre_x, centre_y, width = widest_band(mask, *args.band)
    target = max(int(width * args.width_fraction), 16)

    out = garment.copy()
    draw = ImageDraw.Draw(out)
    letters = args.text.upper()
    font = fit_font(draw, letters, target)

    text_width = draw.textlength(letters, font=font)
    bbox = font.getbbox(letters)
    position = (centre_x - text_width / 2, centre_y - (bbox[3] + bbox[1]) / 2)

    # Outline first, so the letters stay legible on any garment colour.
    thickness = max(2, font.size // 22)
    for dx in range(-thickness, thickness + 1):
        for dy in range(-thickness, thickness + 1):
            if dx or dy:
                draw.text((position[0] + dx, position[1] + dy), letters,
                          font=font, fill=(*parse_hex(args.outline), 255))
    draw.text(position, letters, font=font, fill=(*parse_hex(args.colour), 255))

    args.output.parent.mkdir(parents=True, exist_ok=True)
    out.save(args.output)
    print(f"  '{letters}' at ({centre_x},{centre_y}) font={font.size}pt "
          f"band_width={width}px -> {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
