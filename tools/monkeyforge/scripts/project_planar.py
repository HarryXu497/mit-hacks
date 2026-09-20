"""Project a garment image onto a body by 3D position, not by UV bounding box.

`project_garment.py` fits the sprite to each UV island's rectangle. That works for a single patch
and breaks for a real garment, because a shirt spans three islands that were unwrapped separately:
the trunk, and the shoulders, which belong to the *arm* island. Fitting each island its own copy of
the shirt makes the chest and the shoulder disagree, which is why the result reads as a bib rather
than something worn.

This does the thing a garment actually needs. Every vertex is mapped from its position on the body
onto a position on the garment image -- across the wingspan for x, up the body for z -- and each
triangle is then warped from garment space into texture space. Chest, shoulder and sleeve are one
continuous piece of cloth because they sample one continuous image, whatever islands they live in.

Two consequences worth having:

* Sleeve length stops being a special case. Covering more of the arm is just selecting more faces
  (`--reach`); the fabric that lands on them is already correct.
* Back faces sample the garment mirrored, so a logo does not come out reversed on the spine.

Run:
    python scripts/project_planar.py --texture <base.png> --garment <sprite.png> \
        --layout <layout_pos.json> --islands <islands.json> --select 5 9 1 \
        --reach 0.38 --output <out.png>
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

FONT_CANDIDATES = ("arialbd.ttf", "ariblk.ttf", "DejaVuSans-Bold.ttf", "arial.ttf")


def parse_hex(value: str) -> tuple[int, int, int]:
    value = value.lstrip("#")
    return tuple(int(value[i : i + 2], 16) for i in (0, 2, 4))  # type: ignore[return-value]


def sprite_alpha(sprite: Image.Image, tolerance: int = 26) -> Image.Image:
    """Alpha for the garment, from its own channel or by removing a flat background."""
    rgba = sprite.convert("RGBA")
    if np.asarray(rgba)[..., 3].min() < 250:
        return rgba
    rgb = np.asarray(rgba.convert("RGB"), dtype=np.int16)
    border = np.concatenate([rgb[0], rgb[-1], rgb[:, 0], rgb[:, -1]])
    background = np.median(border, axis=0)
    mask = (np.linalg.norm(rgb - background, axis=2) > tolerance).astype(np.uint8) * 255
    # A 4-channel uint8 array is inferred as RGBA; passing the mode is deprecated in Pillow 13.
    return Image.fromarray(np.dstack([np.asarray(rgba.convert("RGB")), mask]).astype(np.uint8))


def fit_font(draw: ImageDraw.ImageDraw, letters: str, target: int) -> ImageFont.FreeTypeFont:
    for candidate in FONT_CANDIDATES:
        try:
            ImageFont.truetype(candidate, 12)
        except OSError:
            continue
        best = None
        for point in range(8, 800, 2):
            trial = ImageFont.truetype(candidate, point)
            if draw.textlength(letters, font=trial) > target:
                break
            best = trial
        if best is not None:
            return best
    return ImageFont.load_default()


def stamp(sprite: Image.Image, text: str, colour: str, fraction: float,
          height: float) -> Image.Image:
    """Put lettering on the garment image, before any projection.

    Stamping here rather than into the texture means the letters are carried through the same warp
    as the cloth, so they sit on the chest and follow the body instead of needing their own
    rotation fudge.
    """
    if not text:
        return sprite
    out = sprite.convert("RGBA")
    draw = ImageDraw.Draw(out)
    letters = text.upper()[:6]
    font = fit_font(draw, letters, max(int(out.width * fraction), 12))

    width = draw.textlength(letters, font=font)
    bbox = font.getbbox(letters)
    position = (out.width / 2 - width / 2, out.height * height - (bbox[3] + bbox[1]) / 2)

    thickness = max(2, font.size // 22)
    for dx in range(-thickness, thickness + 1):
        for dy in range(-thickness, thickness + 1):
            if dx or dy:
                draw.text((position[0] + dx, position[1] + dy), letters,
                          font=font, fill=(24, 20, 22, 255))
    draw.text(position, letters, font=font, fill=(*parse_hex(colour), 255))
    print(f"  stamped '{letters}' at {font.size}pt")
    return out


def triangles(face: dict) -> list[tuple[list[int], ...]]:
    """Fan-triangulate a polygon into index triples, so quads and n-gons work."""
    count = min(len(face["uv"]), len(face["pos"]))
    return [(0, i, i + 1) for i in range(1, count - 1)]


def warp(destination: np.ndarray, source: np.ndarray, sprite: np.ndarray,
         out_rgb: np.ndarray, out_hit: np.ndarray,
         out_region: np.ndarray | None = None) -> None:
    """Affine-warp one triangle from garment space into texture space.

    Solving destination -> source and sampling backwards leaves no holes, which forward-mapping
    does whenever the triangle is magnified.
    """
    x0 = int(np.floor(destination[:, 0].min()))
    x1 = int(np.ceil(destination[:, 0].max()))
    y0 = int(np.floor(destination[:, 1].min()))
    y1 = int(np.ceil(destination[:, 1].max()))
    height, width = out_hit.shape
    x0, y0 = max(x0, 0), max(y0, 0)
    x1, y1 = min(x1, width - 1), min(y1, height - 1)
    if x1 < x0 or y1 < y0:
        return

    matrix = np.column_stack([destination, np.ones(3)])
    if abs(np.linalg.det(matrix)) < 1e-9:
        return  # degenerate triangle: no area to paint
    affine = np.linalg.solve(matrix, source)  # 3x2, maps [x, y, 1] -> sprite

    ys, xs = np.mgrid[y0 : y1 + 1, x0 : x1 + 1]
    points = np.stack([xs.ravel() + 0.5, ys.ravel() + 0.5, np.ones(xs.size)], axis=1)

    # Barycentric inside-test in destination space.
    bary = np.linalg.solve(matrix.T, points.T).T
    inside = (bary >= -1e-4).all(axis=1)
    if not inside.any():
        return

    if out_region is not None:
        # Every texel belonging to a face that was selected to be clothed, whether or not the
        # garment image happened to be opaque there. The difference between this and out_hit is
        # exactly the holes a garment's own silhouette punches in the body -- a jacket's bottom
        # vent, say -- which should be cloth, not bare fur.
        out_region[ys.ravel()[inside], xs.ravel()[inside]] = True

    sampled = points[inside] @ affine
    sprite_h, sprite_w = sprite.shape[:2]
    su = np.clip(sampled[:, 0].astype(np.int32), 0, sprite_w - 1)
    sv = np.clip(sampled[:, 1].astype(np.int32), 0, sprite_h - 1)
    texel = sprite[sv, su]

    opaque = texel[:, 3] > 40
    target_y = ys.ravel()[inside][opaque]
    target_x = xs.ravel()[inside][opaque]
    out_rgb[target_y, target_x] = texel[opaque, :3]
    out_hit[target_y, target_x] = True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--texture", type=Path, required=True)
    parser.add_argument("--garment", type=Path, required=True)
    parser.add_argument("--back-garment", type=Path, default=None,
                        help="different image for the back; defaults to the front, mirrored")
    parser.add_argument("--layout", type=Path, required=True)
    parser.add_argument("--islands", type=Path, required=True)
    parser.add_argument("--select", type=int, nargs="+", required=True)
    parser.add_argument("--reach", type=float, default=None,
                        help="how far along the arm the garment goes, as a fraction of arm span: "
                             "0 sleeveless, ~0.38 a t-shirt, 1.0 a suit")
    parser.add_argument("--z-min", type=float, default=None)
    parser.add_argument("--z-max", type=float, default=None)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--text", default=None)
    parser.add_argument("--text-colour", default="#D8D6D3")
    parser.add_argument("--text-fraction", type=float, default=0.40)
    parser.add_argument("--text-height", type=float, default=0.46)
    parser.add_argument("--no-autocrop", action="store_true",
                        help="keep the garment image's full frame. By default it is trimmed to "
                             "the garment's own opaque bounds, because the body is mapped across "
                             "the whole image: a pair of trousers occupying the middle third of "
                             "its frame would otherwise put the monkey's legs on the empty "
                             "background either side and the cloth in the gap between them")
    parser.add_argument("--clear-box", default=None,
                        help="x0,y0,x1,y1 fractions of the garment to repaint with the "
                             "surrounding fabric before stamping. Diffusion puts invented "
                             "lettering on any sports kit; this removes it so real type can go "
                             "in the same place instead of on top of it")
    parser.add_argument("--source-crop", default=None,
                        help="sub-rectangle of the garment image as x0,y0,x1,y1 fractions. "
                             "Diffusion often returns a product shot with several views of the "
                             "item; this picks the one to actually wear")
    parser.add_argument("--coverage", type=Path, default=None,
                        help="write a mask of the texels this garment covered. If the file "
                             "already exists its contents are kept and added to, so several "
                             "passes (body, sleeves, trousers) accumulate into one mask. The "
                             "assembler uses it to build the garment as a raised shell rather "
                             "than paint flattened into the skin")
    parser.add_argument("--fill-gaps", type=int, default=0, metavar="RADIUS",
                        help="close holes the garment image's own silhouette leaves inside faces "
                             "that were selected to be clothed -- a jacket's bottom vent shows as "
                             "a wedge of bare fur otherwise. Not on by default, because a t-shirt "
                             "leaving the forearm bare is the same situation and is wanted; use "
                             "it on the body pass, where every selected face should be covered")
    parser.add_argument("--close", type=int, default=1,
                        help="grow the painted area by this many pixels to close UV seams")
    args = parser.parse_args()

    texture = Image.open(args.texture).convert("RGB")
    width, height = texture.size

    layout = json.loads(args.layout.read_text(encoding="utf-8"))
    mesh = max(layout["meshes"], key=lambda m: len(m["faces"]))
    faces = mesh["faces"]
    if "pos" not in faces[0]:
        raise SystemExit("layout has no vertex positions; re-run blender_export_uv_layout.py")

    islands = json.loads(args.islands.read_text(encoding="utf-8"))["islands"]
    members: list[int] = []
    for index in args.select:
        members.extend(islands[index]["members"])

    midline_x = sum(face["centre"][0] for face in faces) / len(faces)
    if args.reach is not None:
        span = max(abs(faces[m]["centre"][0] - midline_x) for m in members)
        cutoff = span * args.reach
        kept = [m for m in members if abs(faces[m]["centre"][0] - midline_x) <= cutoff]
        print(f"  reach={args.reach:.2f} of {span:.2f} -> cutoff {cutoff:.2f}: "
              f"{len(kept)}/{len(members)} faces")
        members = kept
    if args.z_min is not None:
        members = [m for m in members if faces[m]["centre"][2] >= args.z_min]
    if args.z_max is not None:
        members = [m for m in members if faces[m]["centre"][2] <= args.z_max]
    if not members:
        raise SystemExit("no faces selected")

    # The garment's extent on the body. Everything maps into this box, which is what makes the
    # pieces line up across islands.
    covered = np.array([p for m in members for p in faces[m]["pos"]], dtype=np.float64)
    # Centre the box on the body's midline rather than on the covered faces' own extent. The mesh
    # is not perfectly symmetric, so raw min/max puts the garment image's centre off to one side --
    # which slides a chest logo off the chest and crops it at the edge of the cloth.
    half = max(abs(covered[:, 0].min() - midline_x), abs(covered[:, 0].max() - midline_x))
    x_lo, x_hi = midline_x - half, midline_x + half
    z_lo, z_hi = covered[:, 2].min(), covered[:, 2].max()
    y_mid = float(np.median(covered[:, 1]))
    print(f"  garment covers x[{x_lo:.2f},{x_hi:.2f}] z[{z_lo:.2f},{z_hi:.2f}] "
          f"over {len(members)} faces")

    def crop(image: Image.Image) -> Image.Image:
        if not args.source_crop:
            return image
        x0, y0, x1, y1 = (float(part) for part in args.source_crop.split(","))
        w, h = image.size
        return image.crop((int(x0 * w), int(y0 * h), int(x1 * w), int(y1 * h)))

    def clear(image: Image.Image) -> Image.Image:
        """Repaint a rectangle by stretching the fabric just above and below it.

        Sampling the neighbouring rows keeps stripes, shading and panel colours continuous, which
        a flat fill of the median colour does not -- on a striped kit a flat patch is as obvious
        as the lettering it replaced.
        """
        if not args.clear_box:
            return image
        x0f, y0f, x1f, y1f = (float(part) for part in args.clear_box.split(","))
        w, h = image.size
        x0, y0, x1, y1 = int(x0f * w), int(y0f * h), int(x1f * w), int(y1f * h)
        if x1 <= x0 or y1 <= y0:
            return image
        band = max((y1 - y0) // 4, 2)
        above = image.crop((x0, max(y0 - band, 0), x1, y0))
        below = image.crop((x0, y1, x1, min(y1 + band, h)))
        patch = Image.new("RGBA", (x1 - x0, y1 - y0))
        half = (y1 - y0) // 2
        patch.paste(above.resize((x1 - x0, half), Image.LANCZOS), (0, 0))
        patch.paste(below.resize((x1 - x0, (y1 - y0) - half), Image.LANCZOS), (0, half))
        out = image.copy()
        out.paste(patch, (x0, y0))
        print(f"  cleared garment box ({x0},{y0})-({x1},{y1})")
        return out

    def autocrop(image: Image.Image) -> Image.Image:
        if args.no_autocrop:
            return image
        alpha = np.asarray(image)[..., 3] > 40
        if not alpha.any():
            return image
        ys, xs = np.nonzero(alpha)
        box = (int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1)
        if box == (0, 0, image.width, image.height):
            return image
        print(f"  autocropped garment to {box}")
        return image.crop(box)

    front = clear(autocrop(crop(sprite_alpha(Image.open(args.garment)))))
    front = stamp(front, args.text, args.text_colour, args.text_fraction, args.text_height)
    back = (autocrop(crop(sprite_alpha(Image.open(args.back_garment)))) if args.back_garment
            else front.transpose(Image.FLIP_LEFT_RIGHT))
    front_pixels = np.asarray(front, dtype=np.uint8)
    back_pixels = np.asarray(back, dtype=np.uint8)

    out_rgb = np.asarray(texture).copy()
    out_hit = np.zeros((height, width), dtype=bool)
    out_region = np.zeros((height, width), dtype=bool)

    x_span = max(x_hi - x_lo, 1e-6)
    z_span = max(z_hi - z_lo, 1e-6)

    for member in members:
        face = faces[member]
        uvs = face["uv"]
        positions = face["pos"]
        # Which side of the body the face is on decides which image it samples, so a chest logo
        # does not reappear mirrored down the monkey's back.
        facing_back = float(np.mean([p[1] for p in positions])) > y_mid
        sprite = back_pixels if facing_back else front_pixels
        sprite_h, sprite_w = sprite.shape[:2]

        garment_uv = []
        for x, _, z in positions:
            u = (x - x_lo) / x_span
            if facing_back:
                u = 1.0 - u
            garment_uv.append((u * sprite_w, (z_hi - z) / z_span * sprite_h))

        texture_uv = [(u * width, (1.0 - v) * height) for u, v in uvs]

        for a, b, c in triangles(face):
            warp(
                np.array([texture_uv[a], texture_uv[b], texture_uv[c]], dtype=np.float64),
                np.array([garment_uv[a], garment_uv[b], garment_uv[c]], dtype=np.float64),
                sprite, out_rgb, out_hit, out_region,
            )

    gaps = out_region & ~out_hit
    if args.fill_gaps and gaps.any():
        # Grow the painted cloth into them, a ring at a time, until they are closed. Each ring
        # takes the colour of whatever cloth it touches, so a gap at the hem fills with hem and a
        # gap at a lapel fills with lapel.
        filled, remaining = 0, gaps
        for _ in range(args.fill_gaps):
            if not remaining.any():
                break
            spread = np.asarray(
                Image.fromarray(out_rgb).filter(ImageFilter.MaxFilter(3))
            )
            reachable = np.asarray(
                Image.fromarray((out_hit * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(3))
            ) > 127
            ring = remaining & reachable
            if not ring.any():
                break
            out_rgb[ring] = spread[ring]
            out_hit |= ring
            filled += int(ring.sum())
            remaining = remaining & ~ring
        print(f"  filled {filled} px of garment gaps "
              f"({int(remaining.sum())} left beyond the fill radius)")

    if args.close > 0 and out_hit.any():
        # Triangles that meet along a UV seam leave a one-pixel unpainted hairline, which renders
        # as a bright crack across the garment. Grow the painted area to swallow it.
        grown = np.asarray(
            Image.fromarray((out_hit * 255).astype(np.uint8))
            .filter(ImageFilter.MaxFilter(args.close * 2 + 1))
        ) > 127
        edge = grown & ~out_hit
        if edge.any():
            painted = Image.fromarray(out_rgb)
            spread = np.asarray(painted.filter(ImageFilter.MaxFilter(args.close * 2 + 1)))
            out_rgb[edge] = spread[edge]
            print(f"  closed {int(edge.sum())} seam px")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(out_rgb).save(args.output)
    print(f"wrote {args.output}  ({int(out_hit.sum())} px of garment)")

    if args.coverage:
        covered = out_hit
        if args.coverage.exists():
            # Accumulate: the body, sleeve and trouser passes each contribute part of one outfit.
            previous = Image.open(args.coverage).convert("L").resize(texture.size, Image.NEAREST)
            covered = covered | (np.asarray(previous) > 127)
        args.coverage.parent.mkdir(parents=True, exist_ok=True)
        Image.fromarray((covered * 255).astype(np.uint8)).save(args.coverage)
        print(f"  coverage -> {args.coverage} ({int(covered.sum())} px total)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
