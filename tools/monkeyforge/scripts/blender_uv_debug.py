"""Draw a mesh's UV layout over each candidate texture, to see which one it was authored for.

Guessing from a render is unreliable -- a wrong texture and a wrong material both look like "the
face is mush". Overlaying the UV islands on the image answers it directly: if the head island lands
on the drawn face, that is the right texture.

Run:
    blender --background --python scripts/blender_uv_debug.py -- \
        --input <mesh.obj> --outdir <dir> --textures a.png b.png
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import bpy


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--outdir", type=Path, required=True)
    parser.add_argument("--textures", type=Path, nargs="+", required=True)
    parser.add_argument("--mesh-name", default=None)
    return parser.parse_args(arguments)


def main() -> None:
    args = parse_args()
    args.outdir.mkdir(parents=True, exist_ok=True)

    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.wm.obj_import(filepath=str(args.input.resolve()))

    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    if args.mesh_name:
        meshes = [m for m in meshes if args.mesh_name.lower() in m.name.lower()] or meshes
    target = max(meshes, key=lambda m: len(m.data.polygons))
    print(f"UV_DEBUG mesh={target.name} faces={len(target.data.polygons)}")

    uv_layer = target.data.uv_layers.active
    if uv_layer is None:
        raise SystemExit("mesh has no UVs")

    # Collect UV polygons once; drawing is done per texture below.
    polygons = []
    for poly in target.data.polygons:
        polygons.append([tuple(uv_layer.data[i].uv) for i in poly.loop_indices])

    us = [u for poly in polygons for u, _ in poly]
    vs = [v for poly in polygons for _, v in poly]
    print(f"UV_DEBUG u=[{min(us):.4f},{max(us):.4f}] v=[{min(vs):.4f},{max(vs):.4f}]")

    try:
        from PIL import Image, ImageDraw
    except ImportError:
        raise SystemExit("needs Pillow in Blender's python; run the overlay outside Blender instead")

    for texture in args.textures:
        if not texture.exists():
            print(f"UV_DEBUG missing texture {texture}")
            continue
        image = Image.open(texture).convert("RGB")
        width, height = image.size
        draw = ImageDraw.Draw(image)
        for poly in polygons:
            # Blender UV origin is bottom-left; image origin is top-left.
            points = [(u * width, (1.0 - v) * height) for u, v in poly]
            draw.polygon(points, outline=(0, 255, 64))
        out = args.outdir / f"uv_over_{texture.stem[:24]}.png"
        image.save(out)
        print(f"UV_DEBUG wrote {out}  ({width}x{height})")


if __name__ == "__main__":
    main()
