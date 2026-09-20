"""Segment a UV layout into islands, and report where each sits on the body.

Masking by per-face height bands cuts across islands and paints disconnected fragments -- a jersey
comes out as a red notch rather than a shirt. An island is the natural unit: it is a contiguous
patch of surface that the artist unwrapped together, so recolouring a whole island produces a
coherent garment.

Islands are found by shared UV position (not shared vertex index, which splits at seams) and then
described by the 3D geometry they cover, so they can be picked by body part.

Run:
    python scripts/uv_islands.py --layout <json> [--texture <png>] [--outdir <dir>]
"""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path

QUANTISE = 5000  # UV positions rounded to this grid count when matching island membership


def island_segments(faces: list[dict]) -> list[list[int]]:
    """Group face indices into islands that share UV positions."""
    corner_to_faces: dict[tuple[int, int], list[int]] = defaultdict(list)
    for index, face in enumerate(faces):
        for u, v in face["uv"]:
            key = (round(u * QUANTISE), round(v * QUANTISE))
            corner_to_faces[key].append(index)

    parent = list(range(len(faces)))

    def find(a: int) -> int:
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    def union(a: int, b: int) -> None:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[rb] = ra

    for members in corner_to_faces.values():
        for other in members[1:]:
            union(members[0], other)

    grouped: dict[int, list[int]] = defaultdict(list)
    for index in range(len(faces)):
        grouped[find(index)].append(index)
    return sorted(grouped.values(), key=len, reverse=True)


def describe(faces: list[dict], members: list[int], z_lo: float, z_hi: float) -> dict:
    centres = [faces[i]["centre"] for i in members]
    zs = [c[2] for c in centres]
    xs = [c[0] for c in centres]
    ys = [c[1] for c in centres]
    us = [u for i in members for u, _ in faces[i]["uv"]]
    vs = [v for i in members for _, v in faces[i]["uv"]]
    height = max(z_hi - z_lo, 1e-6)
    return {
        "faces": len(members),
        "members": members,
        "z_mid": (min(zs) + max(zs)) / 2,
        # Height fraction is what identifies a body part: 0 at the feet, 1 at the top of the head.
        "height_fraction": ((min(zs) + max(zs)) / 2 - z_lo) / height,
        "z_range": [min(zs), max(zs)],
        "x_mid": (min(xs) + max(xs)) / 2,
        "x_extent": max(xs) - min(xs),
        "y_mid": (min(ys) + max(ys)) / 2,
        "uv_box": [min(us), min(vs), max(us), max(vs)],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--layout", type=Path, required=True)
    parser.add_argument("--texture", type=Path, default=None)
    parser.add_argument("--outdir", type=Path, default=None)
    parser.add_argument("--mesh", default=None)
    parser.add_argument("--top", type=int, default=14)
    args = parser.parse_args()

    payload = json.loads(args.layout.read_text(encoding="utf-8"))
    meshes = payload["meshes"]
    if args.mesh:
        meshes = [m for m in meshes if args.mesh.lower() in m["name"].lower()] or meshes
    mesh = max(meshes, key=lambda m: len(m["faces"]))
    faces = mesh["faces"]

    zs = [f["centre"][2] for f in faces]
    z_lo, z_hi = min(zs), max(zs)

    islands = island_segments(faces)
    described = [describe(faces, members, z_lo, z_hi) for members in islands]
    print(f"mesh={mesh['name']} faces={len(faces)} islands={len(islands)}")
    print(f"{'#':>3} {'faces':>6} {'height':>7} {'x_mid':>7} {'x_ext':>7} {'y_mid':>7}  uv_box")
    for index, info in enumerate(described[: args.top]):
        print(f"{index:>3} {info['faces']:>6} {info['height_fraction']:>7.3f} "
              f"{info['x_mid']:>7.2f} {info['x_extent']:>7.2f} {info['y_mid']:>7.2f}  "
              f"[{info['uv_box'][0]:.3f},{info['uv_box'][1]:.3f},"
              f"{info['uv_box'][2]:.3f},{info['uv_box'][3]:.3f}]")

    if args.outdir:
        args.outdir.mkdir(parents=True, exist_ok=True)
        (args.outdir / "islands.json").write_text(
            json.dumps({"mesh": mesh["name"], "z_lo": z_lo, "z_hi": z_hi,
                        "islands": described}, indent=2),
            encoding="utf-8",
        )
        print(f"wrote {args.outdir / 'islands.json'}")

        if args.texture and args.texture.exists():
            from PIL import Image, ImageDraw

            image = Image.open(args.texture).convert("RGB")
            width, height = image.size
            draw = ImageDraw.Draw(image, "RGBA")
            palette = [(255, 80, 80), (80, 200, 255), (255, 220, 60), (140, 255, 140),
                       (220, 120, 255), (255, 160, 60), (120, 255, 220), (255, 255, 255)]
            for index, info in enumerate(described[: args.top]):
                colour = palette[index % len(palette)]
                for member in info["members"]:
                    points = [(u * width, (1 - v) * height) for u, v in faces[member]["uv"]]
                    draw.polygon(points, fill=(*colour, 90), outline=(*colour, 255))
                box = info["uv_box"]
                draw.text((box[0] * width + 3, (1 - box[3]) * height + 3), str(index),
                          fill=(255, 255, 255, 255))
            out = args.outdir / "islands_overlay.png"
            image.save(out)
            print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
