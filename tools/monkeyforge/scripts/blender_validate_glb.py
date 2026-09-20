"""Validate that a MonkeyForge GLB contains its meshes, materials, and attachment nodes."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import bpy
from mathutils import Vector

REQUIRED_SOCKETS = {
    "SOCKET_HEAD_TOP",
    "SOCKET_FACE",
    "SOCKET_BACK",
    "SOCKET_WAIST",
    "SOCKET_HAND_LEFT",
    "SOCKET_HAND_RIGHT",
    "SOCKET_TAIL_TIP",
}


def is_import_helper(obj: bpy.types.Object) -> bool:
    return any(collection.name == "glTF_not_exported" for collection in obj.users_collection)


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    return parser.parse_args(arguments)


def validate(path: Path) -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(path.resolve()))

    meshes = [
        obj
        for obj in bpy.context.scene.objects
        if obj.type == "MESH" and not is_import_helper(obj)
    ]
    present = set(bpy.data.objects.keys())
    missing = sorted(REQUIRED_SOCKETS - present)
    material_count = len({material.name for obj in meshes for material in obj.data.materials})
    triangle_count = sum(
        sum(max(1, len(polygon.vertices) - 2) for polygon in obj.data.polygons) for obj in meshes
    )

    print(
        "MONKEYFORGE_VALIDATION "
        f"meshes={len(meshes)} materials={material_count} triangles={triangle_count} "
        f"missing_sockets={missing}"
    )
    for socket_name in sorted(REQUIRED_SOCKETS & present):
        location = bpy.data.objects[socket_name].matrix_world.translation
        print(
            f"MONKEYFORGE_SOCKET name={socket_name} "
            f"location=({location.x:.4f},{location.y:.4f},{location.z:.4f})"
        )
    for mesh in meshes:
        corners = [mesh.matrix_world @ Vector(corner) for corner in mesh.bound_box]
        min_z = min(corner.z for corner in corners)
        max_z = max(corner.z for corner in corners)
        print(
            f"MONKEYFORGE_MESH name={mesh.name} parent={getattr(mesh.parent, 'name', None)} "
            f"z_bounds=({min_z:.4f},{max_z:.4f})"
        )
    if not meshes or missing:
        raise SystemExit(1)


if __name__ == "__main__":
    validate(parse_args().input)
