"""Paint a garment into a body part's UV region of an existing texture.

Only pixels inside the part's mask change; everything else is copied verbatim, so the monkey's
face, fur, hands and tail survive untouched. That is the whole point -- the base is never
regenerated, only dressed.

Lettering is drawn with a font rather than generated. Diffusion models cannot spell: earlier runs
asked for "MIT" and produced "3I", "XJZ" and "VU".

Run:
    python scripts/paint_garment.py --texture <png> --mask <png> --output <png> \
        --colour "#A31F34" [--trim "#C2C0BF"] [--text MIT]
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont


def parse_hex(value: str) -> tuple[int, int, int]:
    value = value.lstrip("#")
    return tuple(int(value[i : i + 2], 16) for i in (0, 2, 4))  # type: ignore[return-value]


def shade_like(texture: Image.Image, mask: np.ndarray, colour: tuple[int, int, int]) -> Image.Image:
    """Recolour the masked area while keeping the original artwork's light and shade.

    A flat fill would erase the hand-painted shading that makes the sprite look like BTD6 art and
    leave a dead sticker. Taking the source luminance and multiplying it into the new colour keeps
    the folds and ambient occlusion that were already there.
    """
    data = np.asarray(texture.convert("RGB"), dtype=np.float32)
    luminance = data @ np.array([0.299, 0.587, 0.114], dtype=np.float32)

    region = luminance[mask]
    if region.size == 0:
        return texture.convert("RGB")
    # Normalise against the region's own range so a dark patch does not come out black.
    lo, hi = np.percentile(region, 8), np.percentile(region, 96)
    scale = np.clip((luminance - lo) / max(hi - lo, 1e-3), 0.45, 1.25)

    tinted = data.copy()
    for channel in range(3):
        tinted[..., channel] = np.clip(colour[channel] * scale, 0, 255)

    out = np.where(mask[..., None], tinted, data)
    return Image.fromarray(out.astype(np.uint8))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--texture", type=Path, required=True)
    parser.add_argument("--mask", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--colour", required=True)
    parser.add_argument("--trim", default=None)
    parser.add_argument("--text", default=None)
    parser.add_argument("--text-colour", default="#C2C0BF")
    parser.add_argument("--rotate-text", type=float, default=0.0,
                        help="degrees to rotate lettering, for islands unwrapped rotated")
    parser.add_argument("--text-fraction", type=float, default=0.46)
    args = parser.parse_args()

    texture = Image.open(args.texture).convert("RGB")
    mask_image = Image.open(args.mask).convert("L").resize(texture.size, Image.NEAREST)
    mask = np.asarray(mask_image) > 127
    if not mask.any():
        raise SystemExit("mask is empty")

    painted = shade_like(texture, mask, parse_hex(args.colour))

    if args.text:
        # Put lettering on the chest, which is the widest run of masked pixels -- the other islands
        # are the back and sides, where a logo would be hidden or wrapped around a seam.
        ys, xs = np.nonzero(mask)
        column_counts = np.bincount(xs, minlength=texture.width)
        centre_x = int(np.argmax(np.convolve(column_counts, np.ones(48), mode="same")))
        column = ys[np.abs(xs - centre_x) < 40]
        centre_y = int(np.median(column)) if column.size else int(np.median(ys))

        # Size to the *island's own width*, not to a pixel-count statistic. Using column counts
        # gave a 292pt face on a chest a fraction of that wide, so the letters merged into a solid
        # block. A garment logo occupies roughly two thirds of the chest.
        chest_rows = np.abs(ys - centre_y) < max((ys.max() - ys.min()) * 0.25, 8)
        chest_xs = xs[chest_rows]
        chest_width = int(chest_xs.max() - chest_xs.min()) if chest_xs.size else 80
        target = max(int(chest_width * args.text_fraction), 24)
        print(f"  chest width={chest_width}px -> text target={target}px")

        draw = ImageDraw.Draw(painted)
        letters = args.text.upper()[:6]
        font = None
        for candidate in ("arialbd.ttf", "DejaVuSans-Bold.ttf", "arial.ttf"):
            for point in range(8, 400, 2):
                try:
                    trial = ImageFont.truetype(candidate, point)
                except OSError:
                    break
                if draw.textlength(letters, font=trial) > target:
                    font = trial
                    break
            if font:
                break
        if font is None:
            font = ImageFont.load_default()

        width = draw.textlength(letters, font=font)
        bbox = font.getbbox(letters)
        # When the layer will be mirrored, draw at the mirrored x so the flip lands the text back
        # on the chest rather than reflecting it to the far side of the texture.
        anchor_x = centre_x
        position = (anchor_x - width / 2, centre_y - (bbox[3] - bbox[1]) / 2 - bbox[1])

        # Render the lettering on its own layer, then clip it to the garment so it cannot spill
        # onto skin or background in UV space.
        layer = Image.new("RGB", texture.size, (0, 0, 0))
        layer_mask = Image.new("L", texture.size, 0)
        layer_draw = ImageDraw.Draw(layer)
        mask_draw = ImageDraw.Draw(layer_mask)
        for dx in range(-3, 4):
            for dy in range(-3, 4):
                if dx or dy:
                    layer_draw.text((position[0] + dx, position[1] + dy), letters,
                                    font=font, fill=(22, 22, 26))
                    mask_draw.text((position[0] + dx, position[1] + dy), letters,
                                   font=font, fill=255)
        layer_draw.text(position, letters, font=font, fill=parse_hex(args.text_colour))
        mask_draw.text(position, letters, font=font, fill=255)

        if args.rotate_text:
            # The torso island is unwrapped upside-down, so type stamped straight into the texture
            # renders inverted on the model. Rotating the text layer about the stamp position -- not
            # the whole image -- cancels it without moving the letters off the chest.
            box = (centre_x - texture.width // 2, centre_y - texture.height // 2,
                   centre_x + texture.width // 2, centre_y + texture.height // 2)
            layer = layer.crop(box).rotate(args.rotate_text, expand=False)
            layer_mask = layer_mask.crop(box).rotate(args.rotate_text, expand=False)
            canvas = Image.new("RGB", texture.size, (0, 0, 0))
            canvas_mask = Image.new("L", texture.size, 0)
            canvas.paste(layer, (box[0], box[1]))
            canvas_mask.paste(layer_mask, (box[0], box[1]))
            layer, layer_mask = canvas, canvas_mask

        clipped = np.asarray(layer_mask) > 127
        clipped &= mask
        painted = Image.composite(layer, painted, Image.fromarray((clipped * 255).astype(np.uint8)))
        print(f"  lettering '{letters}' at ({centre_x},{centre_y}) size={font.size} "
              f"rotate={args.rotate_text}")

    if args.trim:
        # A thin band of trim at the garment's edge reads as a collar/hem and stops the recolour
        # looking like a paint spill.
        edge = mask_image.filter(ImageFilter.MaxFilter(5))
        inner = mask_image.filter(ImageFilter.MinFilter(7))
        band = (np.asarray(edge) > 127) & ~(np.asarray(inner) > 127) & mask
        if band.any():
            trim_layer = Image.new("RGB", texture.size, parse_hex(args.trim))
            painted = Image.composite(
                trim_layer, painted, Image.fromarray((band * 255).astype(np.uint8))
            )
            print(f"  trim {args.trim} on {band.sum()} px")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    painted.save(args.output)
    print(f"wrote {args.output}  ({mask.sum()} px painted)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
