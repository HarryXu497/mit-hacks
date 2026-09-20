"""Find which processing step breaks the imported asset, by adding one step at a time.

The raw OBJ renders correctly. Something in the build pipeline does not. Guessing which step is how
the last few hours were spent, so this applies them cumulatively and renders after each, making the
culprit obvious rather than inferred.

Run:
    blender --background --python scripts/blender_bisect_pipeline.py -- \
        --input <mesh.obj> --outdir <dir> --stage <n>
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

STAGES = {
    0: "import only",
    1: "+ drop dart prop",
    2: "+ flatten materials",
    3: "+ scale and recentre",
    4: "+ export to GLB and re-import",
    5: "+ drop blink overlay before export",
    6: "+ wire alpha, keep overlay, then export",
    7: "+ wire alpha, export WITHOUT export_apply",
}


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--outdir", type=Path, required=True)
    parser.add_argument("--stage", type=int, required=True, choices=sorted(STAGES))
    parser.add_argument("--resolution", type=int, default=600)
    return parser.parse_args(arguments)


def render(output: Path, resolution: int) -> None:
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    corners = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
    lo = Vector((min(c.x for c in corners), min(c.y for c in corners), min(c.z for c in corners)))
    hi = Vector((max(c.x for c in corners), max(c.y for c in corners), max(c.z for c in corners)))
    centre = (lo + hi) * 0.5
    radius = max((hi - lo).length * 0.5, 0.001)

    camera_data = bpy.data.cameras.new("Camera")
    camera_data.lens = 50
    camera = bpy.data.objects.new("Camera", camera_data)
    camera.location = Vector((centre.x, centre.y - radius * 2.9, centre.z + radius * 0.25))
    camera.rotation_euler = (centre - camera.location).to_track_quat("-Z", "Y").to_euler()
    bpy.context.scene.collection.objects.link(camera)
    bpy.context.scene.camera = camera

    world = bpy.data.worlds.new("World")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.42, 0.44, 0.52, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 2.4
    bpy.context.scene.world = world

    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = resolution
    scene.render.resolution_y = resolution
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = str(output.resolve())
    output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.render.render(write_still=True)


def main() -> None:
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.wm.obj_import(filepath=str(args.input.resolve()))
    print(f"BISECT stage {args.stage}: {STAGES[args.stage]}")

    if args.stage >= 1:
        for obj in list(bpy.context.scene.objects):
            name = obj.name.lower()
            if obj.type == "MESH" and "dart" in name and "flatskin" not in name:
                bpy.data.objects.remove(obj, do_unlink=True)

    if args.stage >= 2:
        for material in bpy.data.materials:
            if not material.use_nodes:
                continue
            for node in material.node_tree.nodes:
                if node.type != "BSDF_PRINCIPLED":
                    continue
                for field, value in (("Specular IOR Level", 0.0), ("Specular", 0.0),
                                     ("Roughness", 1.0), ("Metallic", 0.0)):
                    if field in node.inputs:
                        node.inputs[field].default_value = value

    if args.stage >= 3:
        meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
        points = [o.matrix_world @ v.co for o in meshes for v in o.data.vertices]
        lo = Vector((min(p.x for p in points), min(p.y for p in points), min(p.z for p in points)))
        hi = Vector((max(p.x for p in points), max(p.y for p in points), max(p.z for p in points)))
        scale = 1.72 / (hi.z - lo.z)
        for obj in meshes:
            obj.scale = (scale, scale, scale)
        bpy.context.view_layer.update()

    if args.stage in (6, 7):
        # The OBJ's `map_d` gives the eyelid material alpha from its own image, so only the eye
        # shape shows and the rest of the quad is cut away. Blender's OBJ importer does not always
        # wire that into the BSDF's Alpha socket, and glTF only exports alpha that is wired -- so
        # after a round trip the quad becomes opaque and renders as a tan rectangle over the face.
        # Wiring it explicitly and setting a blend mode preserves the cutout.
        for material in bpy.data.materials:
            if not material.use_nodes:
                continue
            tree = material.node_tree
            bsdf = next((n for n in tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
            image_node = next((n for n in tree.nodes if n.type == "TEX_IMAGE" and n.image), None)
            if bsdf is None or image_node is None:
                continue
            if not bsdf.inputs["Alpha"].is_linked:
                tree.links.new(image_node.outputs["Alpha"], bsdf.inputs["Alpha"])
                print(f"BISECT wired alpha for {material.name}")
            for attribute, value in (("blend_method", "BLEND"), ("surface_render_method", "BLENDED")):
                if hasattr(material, attribute):
                    try:
                        setattr(material, attribute, value)
                    except TypeError:
                        pass

    if args.stage >= 4:
        if args.stage == 5:
            # The OBJ gives the eyelid material an alpha map (`map_d`) so it is see-through and the
            # eyes painted on the body show underneath. glTF does not carry that, so after a round
            # trip the eyelids become opaque tan discs covering the eyes. The mesh is a blink
            # overlay, so removing it is the fix, not repairing its transparency.
            for obj in list(bpy.context.scene.objects):
                if obj.type == "MESH" and "eyelid" in obj.name.lower():
                    print(f"BISECT dropping blink overlay {obj.name}")
                    bpy.data.objects.remove(obj, do_unlink=True)

        temp = args.outdir / "roundtrip.glb"
        temp.parent.mkdir(parents=True, exist_ok=True)
        bpy.ops.export_scene.gltf(filepath=str(temp), export_format="GLB",
                                  export_apply=(args.stage != 7), export_image_format="AUTO")
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.gltf(filepath=str(temp))

    render(args.outdir / f"stage{args.stage}.png", args.resolution)
    print(f"BISECT wrote stage{args.stage}.png")


if __name__ == "__main__":
    main()
