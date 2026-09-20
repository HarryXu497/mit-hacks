"""Draw exported UV islands over a texture, colour-coded by body part.

Answers the question a render cannot: which pixels does the face actually sample? Faces are
coloured by their 3D position -- head, torso, limbs -- so the islands can be identified at a glance
and a garment can be painted into the correct region.

Run:
    python scripts/uv_overlay.py --layout <json> --texture <png> --output <png> \
        [--atlas-window u0 v0 u1 v1]
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw

# Body parts by height fraction, then coloured so islands are distinguishable.
PART_COLOURS = {
    "head": (255, 80, 80),
    "torso": (80, 200, 255),
    "arms": (255, 220, 60),
    "legs": (140, 255, 140),
}


def classify(centre: list[float], z_lo: float, z_hi: float, x_span: float) -> str:
    x, _y, z = centre
    height = max(z_hi - z_lo, 1e-6)
    t = (z - z_lo) / height
    # Arms reach wider than the torso, so width separates them from the trunk at similar heights.
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
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mesh", default=None)
    parser.add_argument("--crop-to-uvs", action="store_true",
                        help="crop the texture to the UV bounding box (useful for atlases)")
    args = parser.parse_args()

    payload = json.loads(args.layout.read_text(encoding="utf-8"))
    meshes = payload["meshes"]
    if args.mesh:
        meshes = [m for m in meshes if args.mesh.lower() in m["name"].lower()] or meshes
    mesh = max(meshes, key=lambda m: len(m["faces"]))
    faces = mesh["faces"]
    print(f"mesh={mesh['name']} faces={len(faces)}")

    zs = [f["centre"][2] for f in faces]
    xs = [abs(f["centre"][0]) for f in faces]
    z_lo, z_hi, x_span = min(zs), max(zs), max(xs) or 1.0

    image = Image.open(args.texture).convert("RGB")
    width, height = image.size

    us = [u for f in faces for u, _ in f["uv"]]
    vs = [v for f in faces for _, v in f["uv"]]
    print(f"uv u=[{min(us):.4f},{max(us):.4f}] v=[{min(vs):.4f},{max(vs):.4f}]")

    draw = ImageDraw.Draw(image, "RGBA")
    counts: dict[str, int] = {}
    for face in faces:
        part = classify(face["centre"], z_lo, z_hi, x_span)
        counts[part] = counts.get(part, 0) + 1
        colour = PART_COLOURS[part]
        points = [(u * width, (1.0 - v) * height) for u, v in face["uv"]]
        draw.polygon(points, outline=(*colour, 255), fill=(*colour, 60))
    print("faces per part:", counts)

    if args.crop_to_uvs:
        box = (
            int(max(min(us), 0) * width),
            int(max(1 - max(vs), 0) * height),
            int(min(max(us), 1) * width),
            int(min(1 - min(vs), 1) * height),
        )
        image = image.crop(box)
        print(f"cropped to UV window {box}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    image.save(args.output)
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
