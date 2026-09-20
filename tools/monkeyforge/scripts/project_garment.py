"""Project a generated garment sprite into a body part's UV region.

`paint_garment.py` only recolours: it takes the base artwork's luminance inside the mask and tints
it. The result is the monkey's own chest and belly shading in a different hue -- which is exactly
what it looks like, a red monkey rather than a monkey in a shirt.

This instead maps the *generated garment's pixels* into the UV island, so the collar, sleeve panels,
hem and fabric colours that the model actually drew end up on the model.

The island is a rough planar unwrap of the trunk, so a fitted planar projection is enough: scale the
sprite to the island's bounding box, rotate to match how the island was unwrapped, and composite
only inside the mask so nothing bleeds onto skin.

Run:
    python scripts/project_garment.py --texture <base.png> --mask <mask.png> \
        --garment <sprite.png> --output <out.png> [--rotate 180] [--text MIT]
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

FONT_CANDIDATES = ("arialbd.ttf", "ariblk.ttf", "DejaVuSans-Bold.ttf", "arial.ttf")


def parse_hex(value: str) -> tuple[int, int, int]:
    value = value.lstrip("#")
    return tuple(int(value[i : i + 2], 16) for i in (0, 2, 4))  # type: ignore[return-value]


def sprite_alpha(sprite: Image.Image, tolerance: int = 26) -> Image.Image:
    """Alpha for the garment, from its own channel or by removing a flat background."""
    rgba = sprite.convert("RGBA")
    alpha = np.asarray(rgba)[..., 3]
    if alpha.min() < 250:
        return rgba
    rgb = np.asarray(rgba.convert("RGB"), dtype=np.int16)
    border = np.concatenate([rgb[0], rgb[-1], rgb[:, 0], rgb[:, -1]])
    background = np.median(border, axis=0)
    mask = (np.linalg.norm(rgb - background, axis=2) > tolerance).astype(np.uint8) * 255
    # A 4-channel uint8 array is inferred as RGBA; passing the mode is deprecated in Pillow 13.
    out = np.dstack([np.asarray(rgba.convert("RGB")), mask]).astype(np.uint8)
    return Image.fromarray(out)


def stamp(sprite: Image.Image, text: str, colour: str, fraction: float) -> Image.Image:
    """Put lettering on the sprite *before* projection, so it follows the garment."""
    if not text:
        return sprite
    out = sprite.convert("RGBA")
    draw = ImageDraw.Draw(out)
    letters = text.upper()[:6]
    target = max(int(out.width * fraction), 12)

    font = None
    for candidate in FONT_CANDIDATES:
        try:
            ImageFont.truetype(candidate, 12)
        except OSError:
            continue
        best = None
        for point in range(8, 600, 2):
            trial = ImageFont.truetype(candidate, point)
            if draw.textlength(letters, font=trial) > target:
                break
            best = trial
        if best is not None:
            font = best
            break
    if font is None:
        font = ImageFont.load_default()

    # Chest logos sit a little above the sprite's vertical middle.
    width = draw.textlength(letters, font=font)
    bbox = font.getbbox(letters)
    position = (out.width / 2 - width / 2,
                out.height * 0.46 - (bbox[3] + bbox[1]) / 2)

    thickness = max(2, font.size // 20)
    for dx in range(-thickness, thickness + 1):
        for dy in range(-thickness, thickness + 1):
            if dx or dy:
                draw.text((position[0] + dx, position[1] + dy), letters,
                          font=font, fill=(24, 20, 22, 255))
    draw.text(position, letters, font=font, fill=(*parse_hex(colour), 255))
    print(f"  stamped '{letters}' on the sprite at {font.size}pt")
    return out


def components(mask: np.ndarray) -> list[np.ndarray]:
    """Split the mask into connected pieces.

    The trunk unwraps as two islands, front and back. Fitting one sprite to the bounding box of
    both puts the shirt's left half on the chest and its right half on the spine, which is how the
    first attempt produced a diagonal panel and half a letter.
    """
    labels = np.zeros(mask.shape, dtype=np.int32)
    current = 0
    for seed in zip(*np.nonzero(mask), strict=True):
        if labels[seed]:
            continue
        current += 1
        stack = [seed]
        labels[seed] = current
        while stack:
            y, x = stack.pop()
            for ny, nx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
                if 0 <= ny < mask.shape[0] and 0 <= nx < mask.shape[1]:
                    if mask[ny, nx] and not labels[ny, nx]:
                        labels[ny, nx] = current
                        stack.append((ny, nx))
    pieces = [labels == index for index in range(1, current + 1)]
    pieces.sort(key=lambda piece: int(piece.sum()), reverse=True)
    return pieces


def cover_crop(sprite: Image.Image, aspect: float) -> Image.Image:
    """Centre-crop the sprite to a target aspect so scaling it does not distort the garment.

    A flat-lay shirt is wide; the monkey's trunk is a narrow column. Stretching one onto the other
    flattens the collar into a smear. Cropping to the column's shape keeps collar, chest and hem
    stacked the way they are worn and simply loses the sleeves, which belong to other islands.
    """
    width, height = sprite.size
    if width / height > aspect:
        new_width = max(int(round(height * aspect)), 1)
        left = (width - new_width) // 2
        return sprite.crop((left, 0, left + new_width, height))
    new_height = max(int(round(width / aspect)), 1)
    top = (height - new_height) // 2
    return sprite.crop((0, top, width, top + new_height))


def source_crop(sprite: Image.Image, spec: str | None) -> Image.Image:
    """Take a sub-rectangle of the sprite, in fractions: `x0,y0,x1,y1`.

    Sleeves live at the sides of a flat-lay shirt. Cropping them out lets the arm islands get the
    garment's own sleeve fabric instead of a flat fill.
    """
    if not spec:
        return sprite
    x0, y0, x1, y1 = (float(part) for part in spec.split(","))
    width, height = sprite.size
    return sprite.crop((int(x0 * width), int(y0 * height),
                        max(int(x1 * width), int(x0 * width) + 1),
                        max(int(y1 * height), int(y0 * height) + 1)))


def trim_island(island: np.ndarray, spec: str | None) -> np.ndarray:
    """Keep only part of an island, as `top:0.55` / `bottom:0.4` / `left:0.5` / `right:0.5`.

    An arm unwraps as one strip from shoulder to hand. A sleeve covers the shoulder end of it, so
    the island has to be cut before the garment is projected or the monkey gets painted gloves.
    """
    if not spec:
        return island
    side, _, amount = spec.partition(":")
    fraction = float(amount)
    ys, xs = np.nonzero(island)
    y0, y1 = int(ys.min()), int(ys.max())
    x0, x1 = int(xs.min()), int(xs.max())
    keep = np.zeros_like(island)
    if side == "top":
        keep[y0 : y0 + max(int((y1 - y0 + 1) * fraction), 1), :] = True
    elif side == "bottom":
        keep[y1 - max(int((y1 - y0 + 1) * fraction), 1) + 1 : y1 + 1, :] = True
    elif side == "left":
        keep[:, x0 : x0 + max(int((x1 - x0 + 1) * fraction), 1)] = True
    elif side == "right":
        keep[:, x1 - max(int((x1 - x0 + 1) * fraction), 1) + 1 : x1 + 1] = True
    else:
        raise SystemExit(f"unknown trim side {side!r}")
    return island & keep


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--texture", type=Path, required=True)
    parser.add_argument("--mask", type=Path, required=True)
    parser.add_argument("--garment", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rotate", type=float, default=0.0,
                        help="degrees; the island may be unwrapped rotated")
    parser.add_argument("--text", default=None)
    parser.add_argument("--text-colour", default="#D8D6D3")
    parser.add_argument("--text-fraction", type=float, default=0.52)
    parser.add_argument("--bleed", type=int, default=6,
                        help="grow the projection past the mask so seams do not show")
    parser.add_argument("--source-crop", default=None,
                        help="sub-rectangle of the sprite as x0,y0,x1,y1 fractions, e.g. a sleeve")
    parser.add_argument("--trim-to", default=None,
                        help="keep only part of each island, e.g. top:0.55, so a sleeve stops at "
                             "the elbow instead of running to the hand")
    args = parser.parse_args()

    texture = Image.open(args.texture).convert("RGB")
    mask_image = Image.open(args.mask).convert("L").resize(texture.size, Image.NEAREST)
    mask = np.asarray(mask_image) > 127
    if not mask.any():
        raise SystemExit("mask is empty")

    sprite = source_crop(sprite_alpha(Image.open(args.garment)), args.source_crop)
    base = np.asarray(texture).copy()
    painted = 0

    islands = components(mask)
    print(f"  {len(islands)} island(s): {[int(i.sum()) for i in islands]} px")

    for index, island in enumerate(islands):
        island = trim_island(island, args.trim_to)
        if not island.any():
            continue
        ys, xs = np.nonzero(island)
        x0, x1 = int(xs.min()), int(xs.max())
        y0, y1 = int(ys.min()), int(ys.max())
        # A little bleed keeps the sprite covering the island's edges after resampling.
        x0, y0 = max(x0 - args.bleed, 0), max(y0 - args.bleed, 0)
        x1 = min(x1 + args.bleed, texture.width - 1)
        y1 = min(y1 + args.bleed, texture.height - 1)
        box_w, box_h = x1 - x0 + 1, y1 - y0 + 1

        piece = cover_crop(sprite, box_w / box_h)
        # Lettering goes on the largest island only -- that is the chest. The others are the back
        # and sides, where a logo would be hidden or split across a seam.
        if index == 0:
            piece = stamp(piece, args.text, args.text_colour, args.text_fraction)
        if args.rotate:
            piece = piece.rotate(args.rotate, expand=True, resample=Image.BICUBIC)
        fitted = piece.resize((box_w, box_h), Image.LANCZOS)

        canvas = Image.new("RGBA", texture.size, (0, 0, 0, 0))
        canvas.paste(fitted, (x0, y0))
        projected = np.asarray(canvas)

        # Only inside this island, and only where the sprite itself is opaque.
        apply = island & (projected[..., 3] > 40)
        base[apply] = projected[..., :3][apply]
        painted += int(apply.sum())
        print(f"  island {index}: box=({x0},{y0})-({x1},{y1}) {box_w}x{box_h} "
              f"-> {int(apply.sum())} px")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(base).save(args.output)
    print(f"wrote {args.output}  ({painted} px from the generated garment)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
