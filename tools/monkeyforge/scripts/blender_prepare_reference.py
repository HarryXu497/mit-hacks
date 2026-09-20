"""Prepare an unrigged OBJ reference as a normalized static MonkeyForge base."""

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
    parser.add_argument("--blend-output", type=Path, required=True)
    parser.add_argument("--glb-output", type=Path, required=True)
    parser.add_argument("--height-metres", type=float, default=1.4)
    return parser.parse_args(arguments)


def clear_scene() -> None:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)


def import_obj(path: Path) -> list[bpy.types.Object]:
    before = set(bpy.data.objects)
    if hasattr(bpy.ops.wm, "obj_import"):
        bpy.ops.wm.obj_import(filepath=str(path), forward_axis="NEGATIVE_Z", up_axis="Y")
    else:
        bpy.ops.import_scene.obj(filepath=str(path), axis_forward="-Z", axis_up="Y")
    return [obj for obj in bpy.data.objects if obj not in before]


def bounds(objects: list[bpy.types.Object]) -> tuple[Vector, Vector]:
    corners = [obj.matrix_world @ Vector(corner) for obj in objects for corner in obj.bound_box]
    if not corners:
        raise ValueError("no mesh bounds found")
    minimum = Vector(
        (min(v.x for v in corners), min(v.y for v in corners), min(v.z for v in corners))
    )
    maximum = Vector(
        (max(v.x for v in corners), max(v.y for v in corners), max(v.z for v in corners))
    )
    return minimum, maximum


def apply_object_transforms(objects: list[bpy.types.Object]) -> None:
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    if objects:
        bpy.context.view_layer.objects.active = objects[0]
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.object.select_all(action="DESELECT")


def normalize(objects: list[bpy.types.Object], target_height: float) -> tuple[Vector, Vector]:
    minimum, maximum = bounds(objects)
    height = maximum.z - minimum.z
    if height <= 1e-6:
        raise ValueError("model has zero height")
    scale = target_height / height
    for obj in objects:
        obj.scale *= scale
    apply_object_transforms(objects)

    minimum, maximum = bounds(objects)
    center_x = (minimum.x + maximum.x) * 0.5
    center_y = (minimum.y + maximum.y) * 0.5
    offset = Vector((-center_x, -center_y, -minimum.z))
    for obj in objects:
        obj.location += offset
    apply_object_transforms(objects)
    return bounds(objects)


def add_socket(name: str, location: tuple[float, float, float], display_size: float) -> None:
    socket = bpy.data.objects.new(name, None)
    socket.empty_display_type = "SPHERE"
    socket.empty_display_size = display_size
    socket.location = location
    socket["manual_review_required"] = True
    socket["monkeyforge_socket_version"] = 1
    bpy.context.scene.collection.objects.link(socket)


def create_approximate_sockets(minimum: Vector, maximum: Vector) -> None:
    depth = maximum.y - minimum.y
    height = maximum.z - minimum.z
    center_y = (minimum.y + maximum.y) * 0.5
    radius = height * 0.018

    add_socket("SOCKET_HEAD_TOP", (0, center_y, maximum.z + height * 0.015), radius)
    add_socket("SOCKET_FACE", (0, minimum.y - depth * 0.03, height * 0.78), radius)
    add_socket("SOCKET_BACK", (0, maximum.y + depth * 0.03, height * 0.52), radius)
    add_socket("SOCKET_WAIST", (0, center_y, height * 0.40), radius)
    add_socket("SOCKET_HAND_LEFT", (maximum.x, center_y, height * 0.44), radius)
    add_socket("SOCKET_HAND_RIGHT", (minimum.x, center_y, height * 0.44), radius)
    add_socket("SOCKET_TAIL_TIP", (0, maximum.y, height * 0.30), radius)


def prepare(args: argparse.Namespace) -> None:
    args.input = args.input.resolve()
    args.blend_output = args.blend_output.resolve()
    args.glb_output = args.glb_output.resolve()
    if not args.input.exists():
        raise FileNotFoundError(args.input)
    clear_scene()
    imported = import_obj(args.input)

    for obj in list(imported):
        if "DartMonkeyDart" in obj.name:
            bpy.data.objects.remove(obj, do_unlink=True)
            imported.remove(obj)

    meshes = [obj for obj in imported if obj.type == "MESH"]
    if not meshes:
        raise ValueError("reference contains no mesh objects")

    for obj in meshes:
        if "Eyelid" in obj.name:
            obj.name = "MonkeyEyelids"
        elif "FlatSkin" in obj.name:
            obj.name = "MonkeyBody"

    minimum, maximum = normalize(meshes, args.height_metres)
    create_approximate_sockets(minimum, maximum)

    args.blend_output.parent.mkdir(parents=True, exist_ok=True)
    args.glb_output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(args.blend_output))
    bpy.ops.export_scene.gltf(
        filepath=str(args.glb_output),
        export_format="GLB",
        export_apply=True,
        export_animations=False,
    )


if __name__ == "__main__":
    prepare(parse_args())
