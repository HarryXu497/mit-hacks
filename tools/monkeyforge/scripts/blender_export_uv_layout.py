"""Export a mesh's UV islands as JSON, and which faces they came from.

Reading UVs by eye from a render is guesswork -- a blank face could mean a wrong texture, wrong
material, or geometry mapped somewhere unexpected, and they look identical. Dumping the islands
with the 3D position of each face lets the overlay be drawn outside Blender (which has no Pillow)
and answers directly which part of the texture the face actually samples.

Run:
    blender --background --python scripts/blender_export_uv_layout.py -- \
        --input <mesh> --output <json> [--mesh-name FlatSkin]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import bpy
from mathutils import Vector


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mesh-name", default=None)
    return parser.parse_args(arguments)


def main() -> None:
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    suffix = args.input.suffix.lower()
    if suffix == ".obj":
        bpy.ops.wm.obj_import(filepath=str(args.input.resolve()))
    else:
        bpy.ops.import_scene.gltf(filepath=str(args.input.resolve()))

    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    if args.mesh_name:
        meshes = [m for m in meshes if args.mesh_name.lower() in m.name.lower()] or meshes

    payload = {"meshes": []}
    for mesh in meshes:
        uv_layer = mesh.data.uv_layers.active
        if uv_layer is None:
            continue
        verts = mesh.data.vertices
        faces = []
        for poly in mesh.data.polygons:
            uvs = [list(uv_layer.data[i].uv) for i in poly.loop_indices]
            positions = [mesh.matrix_world @ verts[vi].co for vi in poly.vertices]
            centre = Vector((0, 0, 0))
            for position in positions:
                centre += position
            centre /= max(len(positions), 1)
            faces.append({
                "uv": uvs,
                # Face centre in world space, so islands can be labelled by body part
                # (high z = head, front = -y, and so on) without opening Blender again.
                "centre": [round(centre.x, 4), round(centre.y, 4), round(centre.z, 4)],
                # Per-vertex world positions, paired with `uv` by index. Projecting a garment
                # needs these: mapping each vertex from 3D onto the garment image is what makes
                # the shirt continuous across chest, shoulder and sleeve, which are three
                # different UV islands and cannot be fitted as one rectangle.
                "pos": [[round(p.x, 4), round(p.y, 4), round(p.z, 4)] for p in positions],
                "normal": [round(c, 4) for c in (mesh.matrix_world.to_3x3() @ poly.normal)],
            })
        images = []
        for material in mesh.data.materials:
            if material and material.use_nodes:
                for node in material.node_tree.nodes:
                    if node.type == "TEX_IMAGE" and node.image:
                        images.append({"name": node.image.name, "size": list(node.image.size)})
        payload["meshes"].append({
            "name": mesh.name,
            "faces": faces,
            "images": images,
        })
        print(f"UV_EXPORT {mesh.name}: {len(faces)} faces, images={[i['name'] for i in images]}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload), encoding="utf-8")
    print(f"UV_EXPORT wrote {args.output}")


if __name__ == "__main__":
    main()
