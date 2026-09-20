"""Redraw the base monkey's facial features, keeping its palette exactly as it is.

Recolouring the whole character is the wrong tool: it changes everything that was right about the
artwork and still leaves the same face looking back. What actually carries recognition is the
*features* -- the round eyes and the plain oval pupils. Changing those changes who the character
is, while every colour, shadow and highlight stays untouched.

Features are found structurally rather than by typed coordinates, so this keeps working if the
atlas is re-exported or the character moves to a different cell:

  * Eye whites are the only large, bright, colourless areas on a warm-toned character, so they are
    found by (high value, low saturation) and filtered by size and shape.
  * A pupil is the dark blob inside an eye white.
  * A brow is the dark, wide, flat shape sitting just above an eye.

Run:
    python scripts/restyle_features.py --texture <atlas.png> --output <out.png> \
        --pupil star --eye-narrow 0.3 --report
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

#: The dart monkey's cell of the 4x4 atlas: u 0.00-0.25, v 0.25-0.50.
DEFAULT_TILE = (0.0, 0.25, 0.25, 0.50)

PUPIL_SHAPES = ("star", "slit", "square", "diamond", "cross", "round")


def components(mask: np.ndarray, min_area: int = 40) -> list[np.ndarray]:
    """Connected components of a boolean mask, largest first."""
    labels = np.zeros(mask.shape, dtype=np.int32)
    current = 0
    found: list[np.ndarray] = []
    for seed in zip(*np.nonzero(mask), strict=True):
        if labels[seed]:
            continue
        current += 1
        stack = [seed]
        labels[seed] = current
        pixels = 0
        while stack:
            y, x = stack.pop()
            pixels += 1
            for ny, nx in ((y - 1, x), (y + 1, x), (y, x - 1), (y, x + 1)):
                if 0 <= ny < mask.shape[0] and 0 <= nx < mask.shape[1]:
                    if mask[ny, nx] and not labels[ny, nx]:
                        labels[ny, nx] = current
                        stack.append((ny, nx))
        if pixels >= min_area:
            found.append(labels == current)
    found.sort(key=lambda piece: int(piece.sum()), reverse=True)
    return found


def grow(mask: np.ndarray, radius: int) -> np.ndarray:
    """Dilate a mask.

    A painted pupil has an anti-aliased rim that is lighter than the pupil threshold, so erasing
    only the pixels below that threshold leaves a visible ghost outline of the old shape.
    """
    if radius <= 0:
        return mask
    from PIL import ImageFilter

    grown = Image.fromarray((mask * 255).astype(np.uint8)).filter(
        ImageFilter.MaxFilter(radius * 2 + 1)
    )
    return np.asarray(grown) > 127


def bbox(mask: np.ndarray) -> tuple[int, int, int, int]:
    ys, xs = np.nonzero(mask)
    return int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())


def star_points(cx: float, cy: float, outer: float, inner: float, arms: int,
                rotation: float = -math.pi / 2) -> list[tuple[float, float]]:
    points = []
    for index in range(arms * 2):
        radius = outer if index % 2 == 0 else inner
        angle = rotation + index * math.pi / arms
        points.append((cx + radius * math.cos(angle), cy + radius * math.sin(angle)))
    return points


def draw_pupil(draw: ImageDraw.ImageDraw, shape: str, cx: float, cy: float,
               half_w: float, half_h: float, colour: tuple[int, int, int]) -> None:
    """Draw one pupil, sized from the one it replaces so it still fits its eye."""
    if shape == "star":
        # A fat inner radius: a thin four-point star reads as a sparkle rather than as a pupil.
        draw.polygon(star_points(cx, cy, half_h, half_h * 0.52, 4), fill=colour)
    elif shape == "diamond":
        draw.polygon([(cx, cy - half_h * 1.2), (cx + half_w * 1.2, cy),
                      (cx, cy + half_h * 1.2), (cx - half_w * 1.2, cy)], fill=colour)
    elif shape == "square":
        draw.rectangle([cx - half_w, cy - half_h, cx + half_w, cy + half_h], fill=colour)
    elif shape == "slit":
        draw.ellipse([cx - max(half_w * 0.42, 1.0), cy - half_h * 1.25,
                      cx + max(half_w * 0.42, 1.0), cy + half_h * 1.25], fill=colour)
    elif shape == "cross":
        bar = max(half_w * 0.40, 1.5)
        draw.rectangle([cx - bar, cy - half_h * 1.2, cx + bar, cy + half_h * 1.2], fill=colour)
        draw.rectangle([cx - half_w * 1.2, cy - bar, cx + half_w * 1.2, cy + bar], fill=colour)
    else:
        draw.ellipse([cx - half_w, cy - half_h, cx + half_w, cy + half_h], fill=colour)


def head_mask(layout_path: Path, size: tuple[int, int], box: tuple[int, int, int, int],
              fraction: float) -> np.ndarray | None:
    """Which texels the head samples, rasterised from the mesh's own UVs.

    Without this, "the pale patch on the face" is found by colour alone and the monkey's pale
    hands win, because they are larger and lighter than the muzzle. The mesh knows which faces are
    on the head; colour alone never can.
    """
    if layout_path is None or not layout_path.exists():
        return None
    import json

    layout = json.loads(layout_path.read_text(encoding="utf-8"))
    mesh = max(layout["meshes"], key=lambda m: len(m["faces"]))
    faces = mesh["faces"]
    zs = [face["centre"][2] for face in faces]
    lo, hi = min(zs), max(zs)
    cutoff = lo + (hi - lo) * fraction

    width, height = size
    mask = Image.new("L", (width, height), 0)
    draw = ImageDraw.Draw(mask)
    kept = 0
    for face in faces:
        if face["centre"][2] < cutoff:
            continue
        kept += 1
        draw.polygon([(u * width, (1 - v) * height) for u, v in face["uv"]], fill=255)
    print(f"  head mask from {kept} faces above z={cutoff:.2f}")
    return np.asarray(mask.crop(box)) > 127


def near(mask: np.ndarray, eyes: list[np.ndarray], distance: int) -> bool:
    """Is this shape within `distance` pixels of any eye's bounding box?"""
    x0, y0, x1, y1 = bbox(mask)
    for eye in eyes:
        ex0, ey0, ex1, ey1 = bbox(eye)
        if (x0 - distance <= ex1 and x1 + distance >= ex0
                and y0 - distance <= ey1 and y1 + distance >= ey0):
            return True
    return False


def find_brows(value: np.ndarray, eyes: list[np.ndarray]) -> list[np.ndarray]:
    """A brow is a solid, wide, flat, mid-dark bar sitting next to the eyes.

    The threshold is well above the pupils' darkness: a brow is painted in fur-brown, not in the
    near-black of a pupil, so looking only for the darkest pixels misses it entirely.
    """
    found = []
    for piece in components(value < 0.50, min_area=200):
        x0, y0, x1, y1 = bbox(piece)
        w, h = x1 - x0 + 1, y1 - y0 + 1
        fill = piece.sum() / float(w * h)
        if w / h > 1.2 and fill > 0.70 and near(piece, eyes, 40):
            found.append(piece)
    return found


def reshape_brows(editable: Image.Image, tile: Image.Image, value: np.ndarray,
                  eyes: list[np.ndarray], tilt: float, thickness: float,
                  taper: float) -> Image.Image:
    """Erase each brow and redraw it as a tilted, optionally tapered bar."""
    brows = find_brows(value, eyes)
    if not brows:
        print("  no brows found; skipping brow reshape")
        return editable

    pixels = np.asarray(editable).copy()
    source = np.asarray(tile)
    for index, brow in enumerate(brows):
        x0, y0, x1, y1 = bbox(brow)
        w, h = x1 - x0 + 1, y1 - y0 + 1
        brow_colour = tuple(int(c) for c in source[brow].mean(axis=0))

        # Erase with the skin around the brow, sampled from a ring just outside it, so the old
        # bar leaves no shadow of itself behind.
        ring = grow(brow, 4) & ~grow(brow, 1)
        skin = (source[ring].mean(axis=0) if ring.any() else source[brow].mean(axis=0))
        pixels[grow(brow, 2)] = skin.astype(np.uint8)

        layer = Image.fromarray(pixels)
        draw = ImageDraw.Draw(layer)
        cx, cy = (x0 + x1) / 2.0, (y0 + y1) / 2.0
        half_w, half_h = w / 2.0, h / 2.0 * thickness
        radians = math.radians(tilt)
        cos, sin = math.cos(radians), math.sin(radians)

        # Build the bar as a quad around its own centre, taper one end, then rotate. Rotating the
        # corners rather than the image keeps the edges crisp on a 512px tile, where resampling a
        # rotated crop would visibly soften a 20px-tall brow.
        corners = [(-half_w, -half_h * (1.0 - taper)), (half_w, -half_h),
                   (half_w, half_h), (-half_w, half_h * (1.0 - taper))]
        draw.polygon([(cx + px * cos - py * sin, cy + px * sin + py * cos)
                      for px, py in corners], fill=brow_colour)
        pixels = np.asarray(layer).copy()
        print(f"  brow {index}: ({x0},{y0})-({x1},{y1}) {w}x{h} "
              f"-> tilt {tilt:+.0f} deg, thickness x{thickness}, taper {taper}")
    return Image.fromarray(pixels)


def narrow_muzzle(editable: Image.Image, tile: Image.Image, value: np.ndarray,
                  saturation: np.ndarray, eyes: list[np.ndarray],
                  light: float, narrow: float, head: np.ndarray | None) -> Image.Image:
    """Pull the pale muzzle patch in from both sides, changing the pale fur's outline.

    The muzzle is the largest pale warm area on the head that is not an eye. Narrowing it row by
    row about its own centre keeps the patch's top and bottom where they were, so the face still
    reads as the same face -- just with a different marking on it.
    """
    pale = (value > light) & (saturation > 0.25)
    if head is not None:
        pale &= head
    for eye in eyes:
        pale &= ~grow(eye, 3)

    patches = components(pale, min_area=1500)
    if not patches:
        print(f"  no muzzle found above value {light}; try a lower --muzzle-light")
        return editable
    muzzle = patches[0]
    x0, y0, x1, y1 = bbox(muzzle)
    print(f"  muzzle: ({x0},{y0})-({x1},{y1}) {x1-x0+1}x{y1-y0+1} "
          f"area={int(muzzle.sum())} -> narrowed {narrow:.0%}")

    pixels = np.asarray(editable).copy()
    source = np.asarray(tile)
    for y in range(y0, y1 + 1):
        row = np.nonzero(muzzle[y])[0]
        if row.size < 2:
            continue
        left_x, right_x = int(row.min()), int(row.max())
        centre = (left_x + right_x) / 2.0
        cut = int((right_x - left_x + 1) * narrow / 2.0)
        if cut < 1:
            continue
        # Fill the vacated edges with the fur immediately outside the patch on that side, so the
        # new outline sits in the surrounding colour rather than an averaged one.
        outside_left = source[y, max(left_x - 2, 0)]
        outside_right = source[y, min(right_x + 2, source.shape[1] - 1)]
        pixels[y, left_x : min(left_x + cut, int(centre))] = outside_left
        pixels[y, max(right_x - cut + 1, int(centre)) : right_x + 1] = outside_right
    return Image.fromarray(pixels)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--texture", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tile", type=float, nargs=4, default=DEFAULT_TILE,
                        metavar=("U0", "U1", "V0", "V1"))
    parser.add_argument("--pupil", choices=PUPIL_SHAPES, default=None,
                        help="redraw the pupils in this shape")
    parser.add_argument("--pupil-scale", type=float, default=1.0)
    parser.add_argument("--pupil-size", type=float, default=0.20,
                        help="pupil radius as a fraction of the eye's height")
    parser.add_argument("--eye-narrow", type=float, default=0.0,
                        help="fraction of each eye's height to bring the upper lid down by, for a "
                             "narrower, less wide-eyed look")
    parser.add_argument("--eye-narrow-bottom", type=float, default=0.0,
                        help="same from below; usually smaller than the top, as a real lid is")
    parser.add_argument("--erase-grow", type=int, default=2,
                        help="how far to grow the old pupil before erasing it, in pixels")
    parser.add_argument("--layout", type=Path, default=Path("output/uv_debug/layout_pos.json"),
                        help="mesh layout, used to work out which texels are on the head. The "
                             "pale hands outrank the muzzle on colour alone")
    parser.add_argument("--head-fraction", type=float, default=0.62,
                        help="height fraction above which a face counts as head")
    parser.add_argument("--brow-tilt", type=float, default=0.0,
                        help="degrees to tilt the brows; positive lifts the outer end (surprised), "
                             "negative drops it inward (a scowl). The two brows share mirrored "
                             "UVs, so one edit changes both and they stay symmetric")
    parser.add_argument("--brow-thickness", type=float, default=1.0,
                        help="scale the brow's thickness; below 1 is a finer, sharper brow")
    parser.add_argument("--brow-taper", type=float, default=0.0,
                        help="0 is a flat bar; towards 1 the brow narrows to a point at one end")
    parser.add_argument("--muzzle-narrow", type=float, default=0.0,
                        help="pull the light muzzle patch in from the sides by this fraction, "
                             "changing the shape of the pale fur on the face")
    parser.add_argument("--muzzle-light", type=float, default=0.72,
                        help="value threshold that separates the pale muzzle from the fur")
    parser.add_argument("--report", action="store_true",
                        help="print what was found and write a marked-up preview")
    args = parser.parse_args()

    image = Image.open(args.texture).convert("RGB")
    width, height = image.size
    u0, u1, v0, v1 = args.tile
    # v runs bottom-up in UV and top-down in image rows.
    top, bottom = int((1.0 - v1) * height), int((1.0 - v0) * height)
    left, right = int(u0 * width), int(u1 * width)
    tile = image.crop((left, top, right, bottom))
    print(f"  tile rows {top}:{bottom} cols {left}:{right}")
    head = head_mask(args.layout, image.size, (left, top, right, bottom), args.head_fraction)

    data = np.asarray(tile, dtype=np.float32) / 255.0
    maximum = data.max(axis=-1)
    minimum = data.min(axis=-1)
    value = maximum
    saturation = np.where(maximum > 1e-6, (maximum - minimum) / np.maximum(maximum, 1e-6), 0.0)

    # Eye whites: bright and colourless. Everything else on this character is warm-toned.
    white = (value > 0.82) & (saturation < 0.18)
    candidates = components(white, min_area=120)

    eyes: list[np.ndarray] = []
    for piece in candidates:
        x0, y0, x1, y1 = bbox(piece)
        w, h = x1 - x0 + 1, y1 - y0 + 1
        fill = piece.sum() / float(w * h)
        if not (fill > 0.55 and 12 < w < tile.width * 0.75):
            continue
        if w / h > 1.4:
            # The two eye whites touch, so they arrive as one blob. Split at the narrowest
            # column between them -- the waist where the two circles meet -- rather than at the
            # midpoint, which would cut one eye short if they are not the same size.
            columns = piece[:, x0 : x1 + 1].sum(axis=0)
            margin = max(int(w * 0.28), 1)
            interior = columns[margin : w - margin]
            waist = x0 + margin + int(np.argmin(interior))
            left_eye, right_eye = piece.copy(), piece.copy()
            left_eye[:, waist:] = False
            right_eye[:, :waist] = False
            print(f"  split merged eye blob at column {waist}")
            eyes.extend(part for part in (left_eye, right_eye) if part.any())
        elif 0.45 < w / h:
            eyes.append(piece)
    print(f"  {len(candidates)} bright blobs -> {len(eyes)} eyes")

    editable = tile.copy()
    draw = ImageDraw.Draw(editable)
    pixels = np.asarray(editable).copy()

    for index, eye in enumerate(eyes):
        x0, y0, x1, y1 = bbox(eye)
        eye_w, eye_h = x1 - x0 + 1, y1 - y0 + 1

        # The pupil is the dark part inside the eye's own box.
        box = np.zeros_like(eye)
        box[y0 : y1 + 1, x0 : x1 + 1] = True
        dark = box & (value < 0.42)
        pupil_parts = components(dark, min_area=8)
        if not pupil_parts:
            print(f"  eye {index}: ({x0},{y0})-({x1},{y1}) no pupil found")
            continue
        pupil = pupil_parts[0]
        px0, py0, px1, py1 = bbox(pupil)
        pupil_colour = tuple(int(c) for c in np.asarray(tile)[pupil].mean(axis=0))
        white_colour = tuple(int(c) for c in np.asarray(tile)[eye & ~dark].mean(axis=0))
        print(f"  eye {index}: white ({x0},{y0})-({x1},{y1}) {eye_w}x{eye_h}  "
              f"pupil ({px0},{py0})-({px1},{py1}) rgb{pupil_colour}")

        if args.pupil:
            # Erase the old pupil with the eye's own white, growing the mask so the anti-aliased
            # rim goes too and no ghost of the old shape is left behind. Clipped to the eye so the
            # white cannot spill onto the face.
            pixels[grow(pupil, args.erase_grow) & eye] = white_colour
            editable = Image.fromarray(pixels)
            draw = ImageDraw.Draw(editable)
            # Size the new pupil from the *eye*, not from the pupil it replaces. The source pupils
            # differ wildly in shape (one is a wide dash, the other a narrow oval), so scaling off
            # them makes one star huge and the other tiny. The position still comes from the old
            # pupil, which is what preserves the character's gaze direction.
            radius = max(eye_h * args.pupil_size * args.pupil_scale, 2.5)
            draw_pupil(
                draw, args.pupil,
                (px0 + px1) / 2.0, (py0 + py1) / 2.0,
                radius, radius, pupil_colour,
            )
            pixels = np.asarray(editable).copy()

    editable = Image.fromarray(pixels)

    if (args.eye_narrow > 0 or args.eye_narrow_bottom > 0) and eyes:
        # Bring a lid down over the eye rather than slicing a flat band off it. The fill colour is
        # taken per column from the skin immediately outside the eye in that column, so the new
        # lid edge follows the face's own shading and curve instead of leaving a straight bar of
        # one averaged colour across the eye.
        shaved = np.asarray(editable).copy()
        source = np.asarray(tile)
        for eye in eyes:
            x0, y0, x1, y1 = bbox(eye)
            eye_h = y1 - y0 + 1
            for x in range(x0, x1 + 1):
                column = np.nonzero(eye[:, x])[0]
                if column.size == 0:
                    continue
                top_y, bottom_y = int(column.min()), int(column.max())
                cut_top = int(eye_h * args.eye_narrow)
                cut_bottom = int(eye_h * args.eye_narrow_bottom)
                # Copy the band of skin sitting directly above (or below) the eye down over it,
                # rather than flooding with one sampled colour. The skin's own shading comes with
                # it, so the new lid curves with the face; a per-column flat fill produced
                # vertical stripes like an awning.
                for offset in range(cut_top):
                    target = top_y + offset
                    donor = top_y - cut_top + offset
                    if 0 <= donor < source.shape[0] and target <= bottom_y:
                        shaved[target, x] = source[donor, x]
                for offset in range(cut_bottom):
                    target = bottom_y - offset
                    donor = bottom_y + cut_bottom - offset
                    if donor < source.shape[0] and target >= top_y:
                        shaved[target, x] = source[donor, x]
        editable = Image.fromarray(shaved)
        print(f"  narrowed {len(eyes)} eyes: top {args.eye_narrow:.0%}, "
              f"bottom {args.eye_narrow_bottom:.0%}")

    if args.brow_tilt or args.brow_thickness != 1.0 or args.brow_taper:
        editable = reshape_brows(editable, tile, value, eyes,
                                 args.brow_tilt, args.brow_thickness, args.brow_taper)

    if args.muzzle_narrow > 0:
        editable = narrow_muzzle(editable, tile, value, saturation, eyes,
                                 args.muzzle_light, args.muzzle_narrow, head)

    out = image.copy()
    out.paste(editable, (left, top))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    out.save(args.output)
    args.output.with_suffix(".tmp.png").unlink(missing_ok=True)
    print(f"wrote {args.output}")

    if args.report:
        preview = editable.resize((editable.width * 2, editable.height * 2), Image.NEAREST)
        marked = ImageDraw.Draw(preview)
        for eye in eyes:
            x0, y0, x1, y1 = bbox(eye)
            marked.rectangle([x0 * 2, y0 * 2, x1 * 2, y1 * 2], outline=(0, 255, 0), width=1)
        path = args.output.with_name(args.output.stem + "_report.png")
        preview.save(path)
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
