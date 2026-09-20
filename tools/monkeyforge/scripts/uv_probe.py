"""Find which UV coordinates a rendered feature samples, by encoding UV as colour.

Reading UVs off an overlay is guesswork once a mesh has many islands: several look plausible for a
face and picking wrong wastes a rebuild. This does it the other way round -- paint the texture so
that every pixel's colour *is* its own UV coordinate, render the model, then sample the render where
the feature should be. The colour that comes back is the answer.

    make    build the encoded texture
    read    sample a render at given pixel positions and decode them back to UV

Run:
    python scripts/uv_probe.py make --size 2048 --output probe.png
    python scripts/uv_probe.py read --render front.png --points 400,300 440,300
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
from PIL import Image


def make(size: int, output: Path) -> None:
    """Red encodes u, green encodes v, blue is a constant marker.

    Blue is fixed so a sampled pixel can be recognised as coming from the probe texture rather than
    from lighting or background.
    """
    u = np.linspace(0.0, 1.0, size, dtype=np.float32)[None, :].repeat(size, axis=0)
    # Image rows run top-down while v runs bottom-up.
    v = np.linspace(1.0, 0.0, size, dtype=np.float32)[:, None].repeat(size, axis=1)
    rgb = np.dstack([
        (u * 255).astype(np.uint8),
        (v * 255).astype(np.uint8),
        np.full((size, size), 96, dtype=np.uint8),
    ])
    output.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(rgb).save(output)
    print(f"wrote {output} ({size}x{size})")


def read(render: Path, points: list[tuple[int, int]], radius: int = 3) -> None:
    image = np.asarray(Image.open(render).convert("RGB"), dtype=np.float32)
    height, width = image.shape[:2]
    print(f"render {width}x{height}")
    for x, y in points:
        x = max(0, min(width - 1, x))
        y = max(0, min(height - 1, y))
        patch = image[max(0, y - radius):y + radius + 1, max(0, x - radius):x + radius + 1]
        mean = patch.reshape(-1, 3).mean(axis=0)
        u, v = mean[0] / 255.0, mean[1] / 255.0
        # Lighting scales all channels, so recover the ratio using the known blue constant.
        scale = (96.0 / mean[2]) if mean[2] > 4 else 1.0
        print(f"  ({x:>4},{y:>4}) rgb=({mean[0]:5.1f},{mean[1]:5.1f},{mean[2]:5.1f})  "
              f"uv=({u:.4f},{v:.4f})  lit-corrected uv=("
              f"{min(u*scale,1):.4f},{min(v*scale,1):.4f})")


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)

    maker = sub.add_parser("make")
    maker.add_argument("--size", type=int, default=2048)
    maker.add_argument("--output", type=Path, required=True)

    reader = sub.add_parser("read")
    reader.add_argument("--render", type=Path, required=True)
    reader.add_argument("--points", nargs="+", required=True,
                        help="pixel positions as x,y")
    reader.add_argument("--radius", type=int, default=3)

    args = parser.parse_args()
    if args.command == "make":
        make(args.size, args.output)
    else:
        points = []
        for item in args.points:
            x, y = item.split(",")
            points.append((int(x), int(y)))
        read(args.render, points, args.radius)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
