"""Render a mesh with no modifications at all, to establish a baseline.

Every render so far has gone through a builder that flattens materials, rescales, drops meshes and
edits UVs. When the result looks wrong there is no way to tell whether the asset is wrong or one of
those steps is. This imports and renders, nothing else.

Run:
    blender --background --python scripts/blender_render_raw.py -- --input <mesh> --output <png>
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

VIEWS = {"front": 0.0, "side": 90.0, "back": 180.0, "three_quarter": 35.0}


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--view", default="front", choices=sorted(VIEWS))
    parser.add_argument("--resolution", type=int, default=700)
    return parser.parse_args(arguments)


def main() -> None:
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)

    suffix = args.input.suffix.lower()
    if suffix == ".obj":
        bpy.ops.wm.obj_import(filepath=str(args.input.resolve()))
    elif suffix in {".glb", ".gltf"}:
        bpy.ops.import_scene.gltf(filepath=str(args.input.resolve()))
    elif suffix == ".dae":
        bpy.ops.wm.collada_import(filepath=str(args.input.resolve()))
    else:
        raise SystemExit(f"unsupported: {suffix}")

    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    if not meshes:
        raise SystemExit("no meshes")
    print(f"RAW meshes: {[m.name for m in meshes]}")

    corners = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
    lo = Vector((min(c.x for c in corners), min(c.y for c in corners), min(c.z for c in corners)))
    hi = Vector((max(c.x for c in corners), max(c.y for c in corners), max(c.z for c in corners)))
    centre = (lo + hi) * 0.5
    radius = max((hi - lo).length * 0.5, 0.001)

    angle = math.radians(VIEWS[args.view])
    distance = radius * 2.9
    camera_data = bpy.data.cameras.new("Camera")
    camera_data.lens = 50
    camera = bpy.data.objects.new("Camera", camera_data)
    camera.location = Vector((
        centre.x + distance * math.sin(angle),
        centre.y - distance * math.cos(angle),
        centre.z + radius * 0.25,
    ))
    camera.rotation_euler = (centre - camera.location).to_track_quat("-Z", "Y").to_euler()
    bpy.context.scene.collection.objects.link(camera)
    bpy.context.scene.camera = camera

    # Flat, bright, even lighting: the point is to see the texture, not to light a scene.
    world = bpy.data.worlds.new("World")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.42, 0.44, 0.52, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 2.4
    bpy.context.scene.world = world

    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = args.resolution
    scene.render.resolution_y = args.resolution
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = str(args.output.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.render.render(write_still=True)
    print(f"RAW wrote {args.output}")


if __name__ == "__main__":
    main()
