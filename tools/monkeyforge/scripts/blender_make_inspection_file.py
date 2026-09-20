"""Import a generated GLB and save it as an easy-to-open Blender inspection scene."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import bpy


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args(arguments)


def create_inspection_file(source: Path, output: Path) -> None:
    source = source.resolve()
    output = output.resolve()
    if not source.is_file():
        raise FileNotFoundError(source)

    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(source))

    for obj in bpy.context.scene.objects:
        if any(collection.name == "glTF_not_exported" for collection in obj.users_collection):
            obj.hide_set(True)
            obj.hide_render = True
    meshes = [
        obj
        for obj in bpy.context.scene.objects
        if obj.type == "MESH"
        and not any(
            collection.name == "glTF_not_exported" for collection in obj.users_collection
        )
    ]
    if not meshes:
        raise ValueError("GLB contains no meshes")

    bpy.ops.object.select_all(action="DESELECT")
    for mesh in meshes:
        mesh.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]

    output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(output))
    print(f"MONKEYFORGE_INSPECTION_FILE path={output}")


if __name__ == "__main__":
    args = parse_args()
    create_inspection_file(args.input, args.output)
