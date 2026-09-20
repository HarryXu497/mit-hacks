"""Render a repeatable preview of a prepared MonkeyForge Blender scene."""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--blend", type=Path)
    source.add_argument("--glb", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--view", choices=("front", "back", "side"), default="front")
    parser.add_argument("--resolution", type=int, default=768)
    return parser.parse_args(arguments)


def bounds(objects: list[bpy.types.Object]) -> tuple[Vector, Vector]:
    corners = [obj.matrix_world @ Vector(corner) for obj in objects for corner in obj.bound_box]
    return (
        Vector((min(v.x for v in corners), min(v.y for v in corners), min(v.z for v in corners))),
        Vector((max(v.x for v in corners), max(v.y for v in corners), max(v.z for v in corners))),
    )


def point_at(obj: bpy.types.Object, target: Vector) -> None:
    obj.rotation_euler = (target - obj.location).to_track_quat("-Z", "Y").to_euler()


def is_import_helper(obj: bpy.types.Object) -> bool:
    return any(collection.name == "glTF_not_exported" for collection in obj.users_collection)


def apply_toon_materials() -> None:
    for material in bpy.data.materials:
        if not material.use_nodes:
            continue
        principled = material.node_tree.nodes.get("Principled BSDF")
        color = (
            principled.inputs["Base Color"].default_value[:]
            if principled is not None
            else material.diffuse_color[:]
        )
        nodes = material.node_tree.nodes
        links = material.node_tree.links
        nodes.clear()
        output = nodes.new("ShaderNodeOutputMaterial")
        toon = nodes.new("ShaderNodeBsdfToon")
        toon.inputs["Color"].default_value = color
        toon.inputs["Size"].default_value = 0.55
        toon.inputs["Smooth"].default_value = 0.035
        links.new(toon.outputs["BSDF"], output.inputs["Surface"])


def add_area_light(name: str, location: tuple[float, float, float], energy: float) -> None:
    light_data = bpy.data.lights.new(name=name, type="AREA")
    light_data.energy = energy
    light_data.shape = "DISK"
    light_data.size = 4.0
    light = bpy.data.objects.new(name=name, object_data=light_data)
    light.location = location
    bpy.context.scene.collection.objects.link(light)
    point_at(light, Vector((0, 0, 0.7)))


def render(args: argparse.Namespace) -> None:
    if args.blend:
        bpy.ops.wm.open_mainfile(filepath=str(args.blend.resolve()))
    else:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.gltf(filepath=str(args.glb.resolve()))
    apply_toon_materials()
    scene = bpy.context.scene
    for obj in scene.objects:
        if is_import_helper(obj):
            obj.hide_render = True
    meshes = [obj for obj in scene.objects if obj.type == "MESH" and not is_import_helper(obj)]
    if not meshes:
        raise ValueError("scene contains no meshes")

    minimum, maximum = bounds(meshes)
    center = (minimum + maximum) * 0.5
    extent = maximum - minimum
    distance = max(extent.x, extent.y, extent.z) * 2.15

    camera_data = bpy.data.cameras.new("PreviewCamera")
    camera_data.lens = 58
    camera = bpy.data.objects.new("PreviewCamera", camera_data)
    scene.collection.objects.link(camera)
    if args.view == "back":
        camera.location = (center.x, center.y + distance, center.z + extent.z * 0.03)
    elif args.view == "side":
        camera.location = (center.x + distance, center.y, center.z + extent.z * 0.03)
    else:
        camera.location = (center.x, center.y - distance, center.z + extent.z * 0.03)
    point_at(camera, center)
    scene.camera = camera

    add_area_light("Key", (-2.5, -3.5, 4.0), 900)
    add_area_light("Fill", (3.0, -1.5, 2.0), 500)
    add_area_light("Rim", (0.0, 3.0, 3.0), 700)

    world = scene.world or bpy.data.worlds.new("PreviewWorld")
    scene.world = world
    world.use_nodes = True
    background = world.node_tree.nodes.get("Background")
    background.inputs["Color"].default_value = (0.025, 0.04, 0.075, 1.0)
    background.inputs["Strength"].default_value = 0.35

    scene.render.engine = "BLENDER_EEVEE"
    scene.render.use_freestyle = True
    freestyle = scene.view_layers[0].freestyle_settings
    if freestyle.linesets:
        lineset = freestyle.linesets[0]
        linestyle = lineset.linestyle
        if linestyle is None:
            linestyle = bpy.data.linestyles.new("MonkeyForgeOutline")
            lineset.linestyle = linestyle
        linestyle.color = (0.012, 0.02, 0.045)
        linestyle.thickness = 1.15
    scene.render.resolution_x = args.resolution
    scene.render.resolution_y = args.resolution
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.film_transparent = False
    scene.render.filepath = str(args.output.resolve())
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.resolution_percentage = 100
    scene.render.pixel_aspect_x = 1
    scene.render.pixel_aspect_y = 1
    camera.data.dof.use_dof = False
    camera.data.sensor_width = 36
    camera.rotation_euler.rotate_axis("Z", math.radians(0))

    args.output.resolve().parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.render.render(write_still=True)


if __name__ == "__main__":
    render(parse_args())
