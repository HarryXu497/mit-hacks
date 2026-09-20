"""Report the UV bounding box of each body part, so garments can be painted in the right place.

The torso box is what a jersey needs: paint inside it and the shirt lands on the chest, paint
outside it and it lands on an ear. Derived from face positions rather than eyeballed off an
overlay, so it stays correct if the mesh changes.

Run:
    python scripts/uv_regions.py --layout <json> [--texture <png>] [--output <json>]
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


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
    parser.add_argument("--texture", type=Path, default=None)
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--mesh", default=None)
    args = parser.parse_args()

    payload = json.loads(args.layout.read_text(encoding="utf-8"))
    meshes = payload["meshes"]
    if args.mesh:
        meshes = [m for m in meshes if args.mesh.lower() in m["name"].lower()] or meshes
    mesh = max(meshes, key=lambda m: len(m["faces"]))
    faces = mesh["faces"]

    zs = [f["centre"][2] for f in faces]
    xs = [abs(f["centre"][0]) for f in faces]
    z_lo, z_hi, x_span = min(zs), max(zs), max(xs) or 1.0

    regions: dict[str, dict] = {}
    for face in faces:
        part = classify(face["centre"], z_lo, z_hi, x_span)
        entry = regions.setdefault(part, {"u0": 1.0, "v0": 1.0, "u1": 0.0, "v1": 0.0, "faces": 0})
        entry["faces"] += 1
        for u, v in face["uv"]:
            entry["u0"] = min(entry["u0"], u)
            entry["u1"] = max(entry["u1"], u)
            entry["v0"] = min(entry["v0"], v)
            entry["v1"] = max(entry["v1"], v)

    size = None
    if args.texture and args.texture.exists():
        from PIL import Image

        size = Image.open(args.texture).size

    print(f"mesh={mesh['name']}  faces={len(faces)}")
    for part in ("head", "torso", "arms", "legs"):
        entry = regions.get(part)
        if not entry:
            continue
        line = (f"  {part:<6} faces={entry['faces']:<4} "
                f"u[{entry['u0']:.4f},{entry['u1']:.4f}] v[{entry['v0']:.4f},{entry['v1']:.4f}]")
        if size:
            width, height = size
            px = (int(entry["u0"] * width), int((1 - entry["v1"]) * height),
                  int(entry["u1"] * width), int((1 - entry["v0"]) * height))
            entry["pixels"] = list(px)
            line += f"  px={px}"
        print(line)

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(regions, indent=2), encoding="utf-8")
        print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
