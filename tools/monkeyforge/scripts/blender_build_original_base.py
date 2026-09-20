"""Build an original, rigged, low-poly MonkeyForge soccer monkey."""

from __future__ import annotations

import argparse
import math
import sys
from dataclasses import dataclass
from pathlib import Path

import bpy
from mathutils import Matrix, Vector


@dataclass(frozen=True)
class Profile:
    torso_width: float
    head_size: float
    hand_size: float
    arm_length: float
    jersey: str
    accent: str


PROFILES = {
    "balanced": Profile(1.0, 1.0, 1.0, 1.0, "#298FC2", "#FFD34E"),
    "runner": Profile(0.82, 0.96, 0.92, 1.06, "#20A88A", "#E9FF70"),
    "defender": Profile(1.20, 1.0, 1.12, 0.96, "#D84B48", "#FFD15C"),
    "goalkeeper": Profile(1.08, 1.06, 1.34, 1.0, "#F2A51A", "#28A9CB"),
}


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", choices=sorted(PROFILES), required=True)
    parser.add_argument("--blend-output", type=Path, required=True)
    parser.add_argument("--glb-output", type=Path, required=True)
    return parser.parse_args(arguments)


def hex_color(value: str) -> tuple[float, float, float, float]:
    value = value.lstrip("#")
    srgb = [int(value[index : index + 2], 16) / 255 for index in (0, 2, 4)]
    linear = [
        channel / 12.92
        if channel <= 0.04045
        else ((channel + 0.055) / 1.055) ** 2.4
        for channel in srgb
    ]
    return tuple(linear) + (1.0,)


def material(name: str, color: str, roughness: float = 0.78) -> bpy.types.Material:
    result = bpy.data.materials.new(name)
    result.diffuse_color = hex_color(color)
    result.use_nodes = True
    shader = result.node_tree.nodes.get("Principled BSDF")
    shader.inputs["Base Color"].default_value = hex_color(color)
    shader.inputs["Roughness"].default_value = roughness
    return result


def apply_toon_materials() -> None:
    """Use a Blender preview shader; GLB export happens before this conversion."""
    for result in bpy.data.materials:
        if not result.use_nodes:
            continue
        principled = result.node_tree.nodes.get("Principled BSDF")
        color = (
            principled.inputs["Base Color"].default_value[:]
            if principled is not None
            else result.diffuse_color[:]
        )
        nodes = result.node_tree.nodes
        links = result.node_tree.links
        nodes.clear()
        output = nodes.new("ShaderNodeOutputMaterial")
        toon = nodes.new("ShaderNodeBsdfToon")
        toon.inputs["Color"].default_value = color
        toon.inputs["Size"].default_value = 0.55
        toon.inputs["Smooth"].default_value = 0.035
        links.new(toon.outputs["BSDF"], output.inputs["Surface"])
        result["monkeyforge_shader"] = "cel_toon_v1"


def clear_scene() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)


def finish_mesh(
    obj: bpy.types.Object,
    name: str,
    scale: tuple[float, float, float],
    assigned_material: bpy.types.Material,
    bone: str,
    bindings: dict[bpy.types.Object, str],
    smooth: bool = True,
) -> bpy.types.Object:
    obj.name = name
    obj.scale = scale
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(assigned_material)
    if smooth:
        for face in obj.data.polygons:
            face.use_smooth = True
    bindings[obj] = bone
    return obj


def ico(
    name: str,
    location: tuple[float, float, float],
    scale: tuple[float, float, float],
    assigned_material: bpy.types.Material,
    bone: str,
    bindings: dict[bpy.types.Object, str],
    subdivisions: int = 2,
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=subdivisions, radius=1, location=location)
    return finish_mesh(
        bpy.context.object,
        name,
        scale,
        assigned_material,
        bone,
        bindings,
    )


def cylinder_between(
    name: str,
    start: tuple[float, float, float],
    end: tuple[float, float, float],
    radius: float,
    assigned_material: bpy.types.Material,
    bone: str,
    bindings: dict[bpy.types.Object, str],
) -> bpy.types.Object:
    start_vector = Vector(start)
    end_vector = Vector(end)
    direction = end_vector - start_vector
    midpoint = (start_vector + end_vector) * 0.5
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=8,
        radius=radius,
        depth=direction.length,
        location=midpoint,
    )
    obj = bpy.context.object
    obj.rotation_mode = "QUATERNION"
    obj.rotation_quaternion = direction.to_track_quat("Z", "Y")
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    return finish_mesh(
        obj,
        name,
        (1, 1, 1),
        assigned_material,
        bone,
        bindings,
    )


def torus(
    name: str,
    location: tuple[float, float, float],
    major_radius: float,
    minor_radius: float,
    assigned_material: bpy.types.Material,
    bone: str,
    bindings: dict[bpy.types.Object, str],
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_torus_add(
        major_segments=12,
        minor_segments=6,
        major_radius=major_radius,
        minor_radius=minor_radius,
        location=location,
    )
    return finish_mesh(
        bpy.context.object,
        name,
        (1, 1, 1),
        assigned_material,
        bone,
        bindings,
    )


def cone(
    name: str,
    location: tuple[float, float, float],
    radius: float,
    depth: float,
    rotation: tuple[float, float, float],
    assigned_material: bpy.types.Material,
    bone: str,
    bindings: dict[bpy.types.Object, str],
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_cone_add(
        vertices=6,
        radius1=radius,
        radius2=0,
        depth=depth,
        location=location,
        rotation=rotation,
    )
    return finish_mesh(
        bpy.context.object,
        name,
        (1, 1, 1),
        assigned_material,
        bone,
        bindings,
        smooth=False,
    )


def create_armature(profile: Profile) -> bpy.types.Object:
    bpy.ops.object.armature_add(enter_editmode=True, location=(0, 0, 0))
    armature = bpy.context.object
    armature.name = "MonkeyForgeRig"
    armature.data.name = "MonkeyForgeSkeleton"
    armature.show_in_front = True
    edit_bones = armature.data.edit_bones
    edit_bones.remove(edit_bones[0])

    bones = {
        "root": ((0, 0, 0.02), (0, 0, 0.20), None),
        "hips": ((0, 0, 0.42), (0, 0, 0.63), "root"),
        "spine": ((0, 0, 0.63), (0, 0, 0.91), "hips"),
        "chest": ((0, 0, 0.91), (0, 0, 1.02), "spine"),
        "neck": ((0, 0, 1.02), (0, 0, 1.10), "chest"),
        "head": ((0, 0, 1.10), (0, 0, 1.48), "neck"),
        "upper_arm.L": ((0.16, 0, 0.91), (0.51, 0, 0.91), "chest"),
        "forearm.L": ((0.51, 0, 0.91), (0.80 * profile.arm_length, 0, 0.91), "upper_arm.L"),
        "hand.L": (
            (0.80 * profile.arm_length, 0, 0.91),
            (1.0 * profile.arm_length, 0, 0.91),
            "forearm.L",
        ),
        "upper_arm.R": ((-0.16, 0, 0.91), (-0.51, 0, 0.91), "chest"),
        "forearm.R": ((-0.51, 0, 0.91), (-0.80 * profile.arm_length, 0, 0.91), "upper_arm.R"),
        "hand.R": (
            (-0.80 * profile.arm_length, 0, 0.91),
            (-1.0 * profile.arm_length, 0, 0.91),
            "forearm.R",
        ),
        "thigh.L": ((0.13, 0, 0.55), (0.13, 0, 0.31), "hips"),
        "shin.L": ((0.13, 0, 0.31), (0.13, 0, 0.10), "thigh.L"),
        "foot.L": ((0.13, 0, 0.10), (0.13, -0.22, 0.10), "shin.L"),
        "thigh.R": ((-0.13, 0, 0.55), (-0.13, 0, 0.31), "hips"),
        "shin.R": ((-0.13, 0, 0.31), (-0.13, 0, 0.10), "thigh.R"),
        "foot.R": ((-0.13, 0, 0.10), (-0.13, -0.22, 0.10), "shin.R"),
        "tail.01": ((0, 0.16, 0.58), (0.20, 0.30, 0.58), "hips"),
        "tail.02": ((0.20, 0.30, 0.58), (0.42, 0.35, 0.66), "tail.01"),
        "tail.03": ((0.42, 0.35, 0.66), (0.48, 0.28, 0.79), "tail.02"),
    }
    created = {}
    for name, (head, tail, _parent) in bones.items():
        bone = edit_bones.new(name)
        bone.head = head
        bone.tail = tail
        created[name] = bone
    for name, (_head, _tail, parent_name) in bones.items():
        if parent_name:
            created[name].parent = created[parent_name]
    bpy.ops.object.mode_set(mode="OBJECT")
    return armature


def bind_meshes(armature: bpy.types.Object, bindings: dict[bpy.types.Object, str]) -> None:
    for obj, bone_name in bindings.items():
        group = obj.vertex_groups.new(name=bone_name)
        group.add(range(len(obj.data.vertices)), 1.0, "REPLACE")
        modifier = obj.modifiers.new(name="MonkeyForgeArmature", type="ARMATURE")
        modifier.object = armature
        obj.parent = armature
        obj.matrix_parent_inverse = armature.matrix_world.inverted()


def add_socket(
    armature: bpy.types.Object,
    name: str,
    bone_name: str,
    location: tuple[float, float, float],
) -> None:
    socket = bpy.data.objects.new(name, None)
    socket.empty_display_type = "SPHERE"
    socket.empty_display_size = 0.035
    socket.parent = armature
    socket.parent_type = "BONE"
    socket.parent_bone = bone_name
    socket.matrix_world = Matrix.Translation(Vector(location))
    socket["monkeyforge_socket_version"] = 1
    bpy.context.scene.collection.objects.link(socket)


def create_monkey(profile_name: str) -> None:
    profile = PROFILES[profile_name]
    bindings: dict[bpy.types.Object, str] = {}
    fur = material("MF_Fur", "#8C512F")
    skin = material("MF_Skin", "#E7AD72")
    jersey = material("MF_Jersey", profile.jersey)
    shorts = material("MF_Shorts", "#173049")
    white = material("MF_EyeWhite", "#FFF8E8")
    dark = material("MF_Dark", "#17202A")
    accent = material("MF_Accent", profile.accent)
    cheek = material("MF_Cheek", "#F5B17D")

    armature = create_armature(profile)
    ico(
        "Torso",
        (0, 0, 0.73),
        (0.29 * profile.torso_width, 0.22, 0.36),
        jersey,
        "spine",
        bindings,
    )
    ico("Hips", (0, 0, 0.50), (0.25, 0.20, 0.18), shorts, "hips", bindings)
    head_radius = profile.head_size
    ico(
        "Head",
        (0, 0, 1.22),
        (0.43 * head_radius, 0.37 * head_radius, 0.40 * head_radius),
        fur,
        "head",
        bindings,
        subdivisions=2,
    )
    ico(
        "Muzzle",
        (0, -0.34 * head_radius, 1.12),
        (0.30 * head_radius, 0.14, 0.20 * head_radius),
        skin,
        "head",
        bindings,
    )
    for side, sign in (("L", 1), ("R", -1)):
        ico(
            f"Cheek.{side}",
            (0.18 * sign * head_radius, -0.455 * head_radius, 1.06),
            (0.11 * head_radius, 0.025, 0.075 * head_radius),
            cheek,
            "head",
            bindings,
            subdivisions=1,
        )
    for side, sign in (("L", 1), ("R", -1)):
        ico(
            f"Ear.{side}",
            (0.42 * sign * head_radius, 0.01, 1.22),
            (0.14, 0.08, 0.18),
            skin,
            "head",
            bindings,
            subdivisions=1,
        )
        ico(
            f"Eye.{side}",
            (0.15 * sign, -0.355 * head_radius, 1.27),
            (0.115, 0.035, 0.15),
            white,
            "head",
            bindings,
            subdivisions=2,
        )
        ico(
            f"Pupil.{side}",
            (0.15 * sign, -0.389 * head_radius, 1.26),
            (0.050, 0.018, 0.072),
            dark,
            "head",
            bindings,
            subdivisions=1,
        )
        ico(
            f"Brow.{side}",
            (0.15 * sign, -0.392 * head_radius, 1.41),
            (0.12, 0.016, 0.035),
            fur,
            "head",
            bindings,
            subdivisions=1,
        )

    ico("Nose", (0, -0.49, 1.13), (0.065, 0.035, 0.045), dark, "head", bindings, 1)
    cone("Tuft.Center", (0, 0, 1.64), 0.09, 0.28, (0, 0, 0), fur, "head", bindings)
    cone(
        "Tuft.Left",
        (0.09, 0, 1.60),
        0.075,
        0.24,
        (0, math.radians(24), 0),
        fur,
        "head",
        bindings,
    )
    cone(
        "Tuft.Right",
        (-0.09, 0, 1.60),
        0.075,
        0.24,
        (0, math.radians(-24), 0),
        fur,
        "head",
        bindings,
    )

    arm_extent = profile.arm_length
    for side, sign in (("L", 1), ("R", -1)):
        cylinder_between(
            f"UpperArm.{side}",
            (0.19 * sign, 0, 0.91),
            (0.51 * sign, 0, 0.91),
            0.10,
            fur,
            f"upper_arm.{side}",
            bindings,
        )
        cylinder_between(
            f"Forearm.{side}",
            (0.51 * sign, 0, 0.91),
            (0.80 * sign * arm_extent, 0, 0.91),
            0.085,
            fur,
            f"forearm.{side}",
            bindings,
        )
        ico(
            f"Hand.{side}",
            (0.91 * sign * arm_extent, -0.01, 0.91),
            (0.15 * profile.hand_size, 0.12 * profile.hand_size, 0.13 * profile.hand_size),
            skin,
            f"hand.{side}",
            bindings,
            subdivisions=1,
        )
        cylinder_between(
            f"Thigh.{side}",
            (0.13 * sign, 0, 0.53),
            (0.13 * sign, 0, 0.31),
            0.10,
            fur,
            f"thigh.{side}",
            bindings,
        )
        cylinder_between(
            f"Shin.{side}",
            (0.13 * sign, 0, 0.31),
            (0.13 * sign, 0, 0.11),
            0.075,
            fur,
            f"shin.{side}",
            bindings,
        )
        ico(
            f"Foot.{side}",
            (0.13 * sign, -0.09, 0.075),
            (0.13, 0.20, 0.075),
            skin,
            f"foot.{side}",
            bindings,
            subdivisions=1,
        )

    tail_points = [
        ((0, 0.16, 0.58), (0.20, 0.30, 0.58), "tail.01"),
        ((0.20, 0.30, 0.58), (0.42, 0.35, 0.66), "tail.02"),
        ((0.42, 0.35, 0.66), (0.48, 0.28, 0.79), "tail.03"),
    ]
    for index, (start, end, bone) in enumerate(tail_points, start=1):
        cylinder_between(
            f"Tail.{index:02d}", start, end, 0.065 - index * 0.009, fur, bone, bindings
        )

    torus("JerseyCollar", (0, 0, 1.035), 0.18, 0.025, accent, "chest", bindings)
    torus("ShortsWaistband", (0, 0, 0.57), 0.22, 0.018, accent, "hips", bindings)
    ico("JerseyBadge", (0, -0.222, 0.79), (0.075, 0.018, 0.075), accent, "spine", bindings, 1)
    bind_meshes(armature, bindings)

    head_top = 1.62 * head_radius
    hand_x = 1.02 * arm_extent
    add_socket(armature, "SOCKET_HEAD_TOP", "head", (0, 0, head_top))
    add_socket(armature, "SOCKET_FACE", "head", (0, -0.46 * head_radius, 1.22))
    add_socket(armature, "SOCKET_BACK", "spine", (0, 0.25, 0.78))
    add_socket(armature, "SOCKET_WAIST", "hips", (0, 0, 0.53))
    add_socket(armature, "SOCKET_HAND_LEFT", "hand.L", (hand_x, 0, 0.91))
    add_socket(armature, "SOCKET_HAND_RIGHT", "hand.R", (-hand_x, 0, 0.91))
    add_socket(armature, "SOCKET_TAIL_TIP", "tail.03", (0.48, 0.28, 0.79))

    armature["monkeyforge_profile"] = profile_name
    armature["monkeyforge_original_asset"] = True


def build(args: argparse.Namespace) -> None:
    clear_scene()
    create_monkey(args.profile)
    args.blend_output.resolve().parent.mkdir(parents=True, exist_ok=True)
    args.glb_output.resolve().parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(args.glb_output.resolve()),
        export_format="GLB",
        export_apply=False,
        export_animations=True,
        export_skins=True,
        export_morph=True,
    )
    apply_toon_materials()
    bpy.ops.wm.save_as_mainfile(filepath=str(args.blend_output.resolve()))
    print(
        f"MONKEYFORGE_BASE profile={args.profile} "
        f"blend={args.blend_output.resolve()} glb={args.glb_output.resolve()}"
    )


if __name__ == "__main__":
    build(parse_args())
