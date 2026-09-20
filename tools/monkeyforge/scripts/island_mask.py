"""Build a texture mask from chosen UV islands.

Masking by per-face height bands cuts across islands and paints disconnected fragments: the first
attempt at a jersey came out as a red notch rather than a shirt. An island is a contiguous patch the
artist unwrapped together, so selecting whole islands gives a garment-shaped region.

Islands can be chosen explicitly by index, or by body part using the same geometry the segmenter
recorded (height fraction, width, front/back).

Run:
    python scripts/island_mask.py --islands <islands.json> --layout <layout.json> \
        --texture <png> --output <mask.png> --select 5 9
    python scripts/island_mask.py ... --part torso
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

#: Height fraction bands, plus a width ceiling that separates the trunk from the arms -- the arm
#: island spans the full wingspan at a similar height to the torso, so height alone cannot split
#: them.
PART_RULES = {
    "torso": {"height": (0.24, 0.50), "max_x_extent": 8.0},
    "head": {"height": (0.55, 0.85), "max_x_extent": 16.0},
    "legs": {"height": (0.00, 0.22), "max_x_extent": 8.0},
    "arms": {"height": (0.25, 0.55), "min_x_extent": 12.0},
}


def select_by_part(islands: list[dict], part: str) -> list[int]:
    rule = PART_RULES[part]
    lo, hi = rule["height"]
    chosen = []
    for index, island in enumerate(islands):
        if not lo <= island["height_fraction"] <= hi:
            continue
        extent = island["x_extent"]
        if "max_x_extent" in rule and extent > rule["max_x_extent"]:
            continue
        if "min_x_extent" in rule and extent < rule["min_x_extent"]:
            continue
        chosen.append(index)
    return chosen


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--islands", type=Path, required=True)
    parser.add_argument("--layout", type=Path, required=True)
    parser.add_argument("--texture", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--select", type=int, nargs="*", default=None)
    parser.add_argument("--part", choices=sorted(PART_RULES), default=None)
    parser.add_argument("--dilate", type=int, default=2)
    parser.add_argument("--preview", type=Path, default=None)
    # Geometry filters. An island is unwrapped as one patch, but a garment often covers only part
    # of it -- a sleeve is the inner end of the arm island, and guessing which end that is from the
    # UV bounding box gets it wrong half the time. The faces carry their 3D centres, so cut there.
    parser.add_argument("--reach", type=float, default=None,
                        help="sleeve length as a fraction of the arm's reach from the midline: "
                             "0.0 sleeveless, ~0.35 a t-shirt, 1.0 a suit or goalkeeper jersey. A "
                             "fraction rather than a distance so the same number means the same "
                             "garment on a different base")
    parser.add_argument("--abs-x-max", type=float, default=None,
                        help="raw distance cutoff; --reach is the one to use")
    parser.add_argument("--abs-x-min", type=float, default=None)
    parser.add_argument("--z-min", type=float, default=None)
    parser.add_argument("--z-max", type=float, default=None)
    args = parser.parse_args()

    island_data = json.loads(args.islands.read_text(encoding="utf-8"))
    islands = island_data["islands"]
    layout = json.loads(args.layout.read_text(encoding="utf-8"))
    mesh = max(layout["meshes"], key=lambda m: len(m["faces"]))
    faces = mesh["faces"]

    if args.select:
        chosen = args.select
    elif args.part:
        chosen = select_by_part(islands, args.part)
    else:
        raise SystemExit("pass --select or --part")

    if not chosen:
        raise SystemExit("no islands selected")

    texture = Image.open(args.texture).convert("RGB")
    width, height = texture.size
    mask = Image.new("L", (width, height), 0)
    draw = ImageDraw.Draw(mask)

    # The midline to measure |x| against, taken from the whole mesh rather than assuming zero.
    midline_x = sum(face["centre"][0] for face in faces) / len(faces)

    cutoff = args.abs_x_max
    if args.reach is not None:
        # Measure how far the selected islands actually extend, so the fraction is relative to this
        # base's own proportions rather than to model units that mean nothing on another mesh.
        offsets = [abs(faces[m]["centre"][0] - midline_x)
                   for index in chosen for m in islands[index]["members"]]
        span = max(offsets) if offsets else 0.0
        cutoff = span * args.reach
        print(f"  reach={args.reach:.2f} of {span:.2f} -> sleeve cutoff {cutoff:.2f}")

    def keep(face: dict) -> bool:
        x, _, z = face["centre"]
        offset = abs(x - midline_x)
        if cutoff is not None and offset > cutoff:
            return False
        if args.abs_x_min is not None and offset < args.abs_x_min:
            return False
        if args.z_min is not None and z < args.z_min:
            return False
        if args.z_max is not None and z > args.z_max:
            return False
        return True

    total_faces = 0
    for index in chosen:
        island = islands[index]
        members = [m for m in island["members"] if keep(faces[m])]
        total_faces += len(members)
        print(f"  island {index}: {len(members)}/{island['faces']} faces  "
              f"height={island['height_fraction']:.3f} x_ext={island['x_extent']:.2f} "
              f"y_mid={island['y_mid']:.2f}")
        for member in members:
            points = [(u * width, (1 - v) * height) for u, v in faces[member]["uv"]]
            draw.polygon(points, fill=255)

    if args.dilate > 0:
        # Grow slightly so the seams between adjacent faces do not show as unpainted hairlines.
        mask = mask.filter(ImageFilter.MaxFilter(args.dilate * 2 + 1))

    args.output.parent.mkdir(parents=True, exist_ok=True)
    mask.save(args.output)
    print(f"wrote {args.output}  ({total_faces} faces from {len(chosen)} island(s))")

    if args.preview:
        overlay = texture.copy()
        ImageDraw.Draw(overlay, "RGBA").bitmap((0, 0), mask, fill=(80, 200, 255, 120))
        args.preview.parent.mkdir(parents=True, exist_ok=True)
        overlay.save(args.preview)
        print(f"wrote {args.preview}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
