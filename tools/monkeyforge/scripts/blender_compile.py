"""Headless Blender compiler invoked with: blender --background --python this_file -- <args>."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--accessory", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--socket", required=True)
    parser.add_argument("--target-triangles", type=int, required=True)
    parser.add_argument("--target-size", type=float, default=0.65)
    # The base is built with a profile's colours baked into its materials. These let a per-character
    # palette override them at compile time, so `CharacterSpec.palette` reaches the model instead of
    # only reaching the manifest.
    parser.add_argument("--jersey", default=None, help="#RRGGBB for MF_Jersey")
    parser.add_argument("--accent", default=None, help="#RRGGBB for MF_Accent")
    parser.add_argument("--fur", default=None, help="#RRGGBB for MF_Fur")
    parser.add_argument("--face", default=None, help="#RRGGBB for MF_Skin")
    return parser.parse_args(arguments)


# Material names come from `blender_build_original_base.py`. JerseyCollar and JerseyBadge both use
# MF_Accent, so trim follows the accent colour automatically.
PALETTE_MATERIALS = {
    "jersey": "MF_Jersey",
    "accent": "MF_Accent",
    "fur": "MF_Fur",
    "face": "MF_Skin",
}


def srgb_to_linear(channel: float) -> float:
    """Blender stores base colours in linear space; hex codes are sRGB."""
    if channel <= 0.04045:
        return channel / 12.92
    return ((channel + 0.055) / 1.055) ** 2.4


def apply_palette(args: argparse.Namespace) -> None:
    """Recolour the base's named materials from the character's palette."""
    for field, material_name in PALETTE_MATERIALS.items():
        value = getattr(args, field, None)
        if not value:
            continue
        material = bpy.data.materials.get(material_name)
        if material is None:
            print(f"MONKEYFORGE_PALETTE missing material {material_name}")
            continue

        hex_value = value.lstrip("#")
        rgb = [int(hex_value[i : i + 2], 16) / 255.0 for i in (0, 2, 4)]
        linear = [srgb_to_linear(c) for c in rgb]

        material.diffuse_color = (*linear, 1.0)
        if material.use_nodes:
            for node in material.node_tree.nodes:
                if "Base Color" in getattr(node, "inputs", {}):
                    node.inputs["Base Color"].default_value = (*linear, 1.0)
                elif "Color" in getattr(node, "inputs", {}):
                    node.inputs["Color"].default_value = (*linear, 1.0)
        print(f"MONKEYFORGE_PALETTE {material_name}={value}")


def clear_scene() -> None:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)


def import_asset(path: Path) -> None:
    suffix = path.suffix.lower()
    if suffix in {".glb", ".gltf"}:
        bpy.ops.import_scene.gltf(filepath=str(path))
    elif suffix == ".obj":
        if hasattr(bpy.ops.wm, "obj_import"):
            bpy.ops.wm.obj_import(filepath=str(path))
        else:
            bpy.ops.import_scene.obj(filepath=str(path))
    else:
        raise ValueError(f"unsupported asset format: {suffix}")


def world_bounds(objects: list[bpy.types.Object]) -> tuple[Vector, Vector]:
    corners = [obj.matrix_world @ Vector(corner) for obj in objects for corner in obj.bound_box]
    if not corners:
        raise ValueError("accessory contains no bounded mesh objects")
    minimum = Vector(
        (min(v.x for v in corners), min(v.y for v in corners), min(v.z for v in corners))
    )
    maximum = Vector(
        (max(v.x for v in corners), max(v.y for v in corners), max(v.z for v in corners))
    )
    return minimum, maximum


def normalize_and_place(
    objects: list[bpy.types.Object], socket: bpy.types.Object, target_size: float
) -> None:
    minimum, maximum = world_bounds(objects)
    extent = maximum - minimum
    largest = max(extent.x, extent.y, extent.z)
    if largest <= 1e-6:
        raise ValueError("accessory has zero-sized bounds")
    uniform_scale = target_size / largest
    center = (minimum + maximum) * 0.5

    # Work entirely in world space. Imported assets can have rotated roots and
    # non-zero object origins, so changing local location/scale directly can
    # send otherwise valid geometry far away from its intended socket.
    for obj in objects:
        obj.matrix_world = (
            Matrix.Scale(uniform_scale, 4)
            @ Matrix.Translation(-center)
            @ obj.matrix_world
        )

    bpy.context.view_layer.update()
    minimum, maximum = world_bounds(objects)
    bottom_center = Vector(
        ((minimum.x + maximum.x) * 0.5, (minimum.y + maximum.y) * 0.5, minimum.z)
    )
    offset = socket.matrix_world.translation - bottom_center

    for obj in objects:
        world_matrix = Matrix.Translation(offset) @ obj.matrix_world
        obj.matrix_world = world_matrix
        obj.parent = socket
        obj.matrix_parent_inverse = socket.matrix_world.inverted()
        obj.matrix_world = world_matrix
        obj["monkeyforge_socket"] = socket.name


def decimate(objects: list[bpy.types.Object], target_triangles: int) -> None:
    face_count = sum(len(obj.data.polygons) for obj in objects)
    if face_count <= target_triangles or face_count == 0:
        return
    ratio = max(0.01, min(1.0, target_triangles / face_count))
    for obj in objects:
        if len(obj.data.polygons) < 20:
            continue
        bpy.context.view_layer.objects.active = obj
        obj.select_set(True)
        modifier = obj.modifiers.new(name="MonkeyForgeDecimate", type="DECIMATE")
        modifier.ratio = ratio
        modifier.use_collapse_triangulate = True
        bpy.ops.object.modifier_apply(modifier=modifier.name)
        obj.select_set(False)


def compile_asset(args: argparse.Namespace) -> None:
    if not args.base.exists():
        raise FileNotFoundError(args.base)
    if not args.accessory.exists():
        raise FileNotFoundError(args.accessory)

    clear_scene()
    import_asset(args.base)
    socket = bpy.data.objects.get(args.socket)
    if socket is None:
        raise ValueError(f"required socket is missing from base model: {args.socket}")

    objects_before = set(bpy.data.objects)
    import_asset(args.accessory)
    accessory_objects = [
        obj for obj in bpy.data.objects if obj not in objects_before and obj.type == "MESH"
    ]
    if not accessory_objects:
        raise ValueError("generated accessory contains no mesh objects")

    for obj in accessory_objects:
        obj.name = f"PROP_{obj.name}"
    decimate(accessory_objects, args.target_triangles)
    normalize_and_place(accessory_objects, socket, args.target_size)

    # After the accessory is imported, so its own materials are never recoloured by the body.
    apply_palette(args)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(args.output),
        export_format="GLB",
        export_apply=True,
        export_animations=True,
        export_skins=True,
        export_morph=True,
    )


if __name__ == "__main__":
    compile_asset(parse_args())
