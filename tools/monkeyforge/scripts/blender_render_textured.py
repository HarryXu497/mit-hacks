"""Render a GLB with its own materials intact.

`blender_render_preview.py` replaces every material with a toon BSDF, which is right for the
procedural bases but destroys a textured model -- you cannot tell whether a texture survived import
if the renderer throws it away before drawing. This one leaves materials alone.

Run:
    blender --background --python scripts/blender_render_textured.py -- \
        --glb <file> --output <png> [--view front|side|back] [--resolution 640] [--show-sockets]
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

VIEW_ANGLES = {"front": 0.0, "side": 90.0, "back": 180.0, "three_quarter": 35.0}


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--glb", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--view", default="front", choices=sorted(VIEW_ANGLES))
    parser.add_argument("--resolution", type=int, default=640)
    parser.add_argument("--show-sockets", action="store_true")
    return parser.parse_args(arguments)


def scene_bounds(meshes: list[bpy.types.Object]) -> tuple[Vector, Vector]:
    corners = [obj.matrix_world @ Vector(c) for obj in meshes for c in obj.bound_box]
    lo = Vector((min(c.x for c in corners), min(c.y for c in corners), min(c.z for c in corners)))
    hi = Vector((max(c.x for c in corners), max(c.y for c in corners), max(c.z for c in corners)))
    return lo, hi


def main() -> None:
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(args.glb.resolve()))

    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if not meshes:
        raise SystemExit("no meshes in GLB")
    lo, hi = scene_bounds(meshes)
    centre = (lo + hi) * 0.5
    radius = max((hi - lo).length * 0.5, 0.2)

    if args.show_sockets:
        # Mark sockets with small emissive spheres so their placement can be checked by eye.
        marker_material = bpy.data.materials.new("SocketMarker")
        marker_material.use_nodes = True
        emission = marker_material.node_tree.nodes.new("ShaderNodeEmission")
        emission.inputs["Color"].default_value = (1.0, 0.1, 0.4, 1.0)
        emission.inputs["Strength"].default_value = 4.0
        output = next(n for n in marker_material.node_tree.nodes if n.type == "OUTPUT_MATERIAL")
        marker_material.node_tree.links.new(emission.outputs["Emission"], output.inputs["Surface"])
        for obj in list(bpy.context.scene.objects):
            if obj.name.startswith("SOCKET_"):
                bpy.ops.mesh.primitive_uv_sphere_add(
                    radius=radius * 0.035, location=obj.matrix_world.translation
                )
                bpy.context.active_object.data.materials.append(marker_material)

    angle = math.radians(VIEW_ANGLES[args.view])
    distance = radius * 3.1
    camera_data = bpy.data.cameras.new("Camera")
    camera_data.sensor_width = 36
    camera_data.lens = 50
    camera = bpy.data.objects.new("Camera", camera_data)
    camera.location = Vector((
        centre.x + distance * math.sin(angle),
        centre.y - distance * math.cos(angle),
        centre.z + radius * 0.35,
    ))
    direction = centre - camera.location
    camera.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    bpy.context.scene.collection.objects.link(camera)
    bpy.context.scene.camera = camera

    key = bpy.data.lights.new("Key", type="AREA")
    key.energy = 900
    key.size = radius * 5
    key_object = bpy.data.objects.new("Key", key)
    key_object.location = centre + Vector((radius * 2, -radius * 2.5, radius * 2.5))
    key_object.rotation_euler = (centre - key_object.location).to_track_quat("-Z", "Y").to_euler()
    bpy.context.scene.collection.objects.link(key_object)

    world = bpy.data.worlds.new("World")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.09, 0.10, 0.13, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.1
    bpy.context.scene.world = world

    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = args.resolution
    scene.render.resolution_y = args.resolution
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = str(args.output.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.render.render(write_still=True)
    print(f"MONKEYFORGE_RENDER wrote {args.output}")


if __name__ == "__main__":
    main()
