"""Rasterise per-body-part UV masks, so a garment can be painted exactly where that part lives.

A bounding box is useless here: the torso's UV faces are scattered across the tile and their box
covers most of it, overlapping head, arms and legs. Painting into that box would put a jersey on an
ear. Filling the actual face polygons gives a mask that follows the island shapes.

Run:
    python scripts/uv_masks.py --layout <json> --texture <png> --outdir <dir> [--dilate 3]
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter


def classify(centre: list[float], z_lo: float, z_hi: float, x_span: float) -> str:
    x, _y, z = centre
    t = (z - z_lo) / max(z_hi - z_lo, 1e-6)
    if abs(x) > x_span * 0.30 and 0.25 < t < 0.75:
        return "arms"
    if t >= 0.62:
        return "head"
    if t >= 0.30:
        return "torso"
    return "legs"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--layout", type=Path, required=True)
    parser.add_argument("--texture", type=Path, required=True)
    parser.add_argument("--outdir", type=Path, required=True)
    parser.add_argument("--mesh", default=None)
    parser.add_argument("--dilate", type=int, default=3,
                        help="grow masks slightly so seams between faces do not show")
    args = parser.parse_args()
    args.outdir.mkdir(parents=True, exist_ok=True)

    payload = json.loads(args.layout.read_text(encoding="utf-8"))
    meshes = payload["meshes"]
    if args.mesh:
        meshes = [m for m in meshes if args.mesh.lower() in m["name"].lower()] or meshes
    mesh = max(meshes, key=lambda m: len(m["faces"]))
    faces = mesh["faces"]

    zs = [f["centre"][2] for f in faces]
    xs = [abs(f["centre"][0]) for f in faces]
    z_lo, z_hi, x_span = min(zs), max(zs), max(xs) or 1.0

    texture = Image.open(args.texture).convert("RGB")
    width, height = texture.size

    grouped: dict[str, list] = {}
    for face in faces:
        grouped.setdefault(classify(face["centre"], z_lo, z_hi, x_span), []).append(face)

    preview = texture.copy()
    preview_draw = ImageDraw.Draw(preview, "RGBA")

    for part, part_faces in grouped.items():
        mask = Image.new("L", (width, height), 0)
        draw = ImageDraw.Draw(mask)
        for face in part_faces:
            points = [(u * width, (1.0 - v) * height) for u, v in face["uv"]]
            draw.polygon(points, fill=255)
        if args.dilate > 0:
            mask = mask.filter(ImageFilter.MaxFilter(args.dilate * 2 + 1))

        out = args.outdir / f"mask_{part}.png"
        mask.save(out)
        coverage = sum(mask.point(lambda p: 1 if p > 127 else 0).getdata()) / (width * height)
        print(f"  {part:<6} {len(part_faces):>4} faces  coverage={coverage*100:5.2f}%  -> {out}")

        if part == "torso":
            preview_draw.bitmap((0, 0), mask, fill=(80, 200, 255, 110))

    preview.save(args.outdir / "torso_mask_preview.png")
    print(f"wrote {args.outdir / 'torso_mask_preview.png'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
