"""SUPERSEDED -- use `blender_assemble_character.py --texture <png>` instead.

Inherits the GLB round trip from `blender_build_btd6_base.py`, and with it the destroyed eyelid
alpha and shifted UVs. The assembler does the same job -- swap in a texture carrying a painted
garment -- without ever re-importing the base.

---

Build a BTD6 base using a supplied texture, so a painted garment reaches the model.

`blender_build_btd6_base.py` keeps the source artwork. This variant swaps in an edited texture --
the same atlas with a garment painted into one body part's UV region -- which is how a jersey
becomes part of the character rather than a flat tint on the mesh.

Run:
    blender --background --python scripts/blender_build_btd6_skinned.py -- \
        --input <mesh.obj> --texture <edited.png> --output <base.glb>
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))

from blender_build_btd6_base import (  # noqa: E402
    SOCKET_NAMES,
    TARGET_HEIGHT,
    body_meshes,
    bounds,
    derive_sockets,
    flatten_materials,
    make_socket,
    OPEN_EYES_UV,
    remap_eyelids_to_open,
    wire_alpha,
    world_vertices,
)
from mathutils import Vector  # noqa: E402


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--texture", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--v-offset", type=float, default=0.0,
                        help="slide mirrored artwork within its tile")
    return parser.parse_args(arguments)


def swap_body_texture(texture: Path) -> int:
    """Replace the atlas image wherever it is used, keeping UVs untouched.

    Only the largest image is swapped: the eyelid/blink sheet and any prop textures are different
    resolutions and must keep their own artwork.
    """
    replacement = bpy.data.images.load(str(texture.resolve()))
    swapped = 0
    for material in bpy.data.materials:
        if not material.use_nodes:
            continue
        for node in material.node_tree.nodes:
            if node.type != "TEX_IMAGE" or node.image is None:
                continue
            width, height = node.image.size
            if width >= 1024 and width == height:
                node.image = replacement
                swapped += 1
    return swapped


def main() -> None:
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.wm.obj_import(filepath=str(args.input.resolve()))

    for obj in list(bpy.context.scene.objects):
        if obj.type != "MESH":
            continue
        name = obj.name.lower()
        if "dart" in name and "flatskin" not in name:
            bpy.data.objects.remove(obj, do_unlink=True)
        elif "eyelid" in name:
            remap_eyelids_to_open(obj, OPEN_EYES_UV)
        else:
            # The source was exported with a top-down V convention, so the artwork lands mirrored
            # inside its atlas tile -- the face samples the muzzle where the eyes should be.
            pass

    swapped = swap_body_texture(args.texture)
    print(f"MONKEYFORGE_SKIN swapped {swapped} texture node(s) -> {args.texture.name}")
    wire_alpha()
    flatten_materials()

    meshes = body_meshes()
    points = world_vertices(meshes)
    lo, hi = bounds(points)
    scale = TARGET_HEIGHT / (hi.z - lo.z)
    for obj in meshes:
        obj.scale = (scale, scale, scale)
    bpy.context.view_layer.update()

    points = world_vertices(meshes)
    lo, hi = bounds(points)
    offset = Vector((-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, -lo.z))
    for obj in meshes:
        obj.location = obj.location + offset
    bpy.context.view_layer.update()

    points = world_vertices(meshes)
    for name, location in derive_sockets(points).items():
        make_socket(name, location)

    missing = [n for n in SOCKET_NAMES if n not in bpy.data.objects]
    if missing:
        raise SystemExit(f"failed to create sockets: {missing}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(args.output), export_format="GLB", export_apply=True,
        export_image_format="AUTO",
    )
    print(f"MONKEYFORGE_SKIN wrote {args.output}")


if __name__ == "__main__":
    main()
