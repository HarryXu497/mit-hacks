"""Report an imported mesh's structure, so sockets can be placed from measurements not guesses.

Run: blender --background --python scripts/blender_inspect_mesh.py -- --input <mesh>
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import bpy
from mathutils import Vector


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    return parser.parse_args(arguments)


def import_any(path: Path) -> None:
    suffix = path.suffix.lower()
    if suffix in {".glb", ".gltf"}:
        bpy.ops.import_scene.gltf(filepath=str(path.resolve()))
    elif suffix == ".obj":
        bpy.ops.wm.obj_import(filepath=str(path.resolve()))
    elif suffix == ".dae":
        bpy.ops.wm.collada_import(filepath=str(path.resolve()))
    else:
        raise SystemExit(f"unsupported: {suffix}")


def main() -> None:
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    import_any(args.input)

    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    print(f"\nMESHES: {len(meshes)}")

    all_corners: list[Vector] = []
    for mesh in meshes:
        corners = [mesh.matrix_world @ Vector(c) for c in mesh.bound_box]
        all_corners.extend(corners)
        lo = Vector((min(c.x for c in corners), min(c.y for c in corners),
                     min(c.z for c in corners)))
        hi = Vector((max(c.x for c in corners), max(c.y for c in corners),
                     max(c.z for c in corners)))
        tris = sum(max(1, len(p.vertices) - 2) for p in mesh.data.polygons)
        uv = mesh.data.uv_layers.active.name if mesh.data.uv_layers.active else "NONE"
        mats = [m.name for m in mesh.data.materials if m]
        print(f"  {mesh.name}")
        print(f"    tris={tris}  uv={uv}  materials={mats}")
        print(f"    bounds x[{lo.x:.3f},{hi.x:.3f}] y[{lo.y:.3f},{hi.y:.3f}] "
              f"z[{lo.z:.3f},{hi.z:.3f}]")

    lo = Vector((min(c.x for c in all_corners), min(c.y for c in all_corners),
                 min(c.z for c in all_corners)))
    hi = Vector((max(c.x for c in all_corners), max(c.y for c in all_corners),
                 max(c.z for c in all_corners)))
    size = hi - lo
    print(f"\nWHOLE MODEL")
    print(f"  min=({lo.x:.3f},{lo.y:.3f},{lo.z:.3f})  max=({hi.x:.3f},{hi.y:.3f},{hi.z:.3f})")
    print(f"  size=({size.x:.3f},{size.y:.3f},{size.z:.3f})")
    # Tallest axis implies which way is up, which decides whether a rotation is needed to match
    # the MonkeyForge bases (Z-up).
    tallest = max(("x", size.x), ("y", size.y), ("z", size.z), key=lambda pair: pair[1])
    print(f"  tallest axis: {tallest[0]} ({tallest[1]:.3f}) -> likely the up axis")
    print(f"  images: {[i.name for i in bpy.data.images]}")


if __name__ == "__main__":
    main()
