"""Render the rigged DartMonkey in a few walk-cycle poses.

Proves the binding actually deforms the mesh, and shows what the in-game cycle looks like at a
size you can see. The poses here are the same ones `entities::rig::animate_rig` applies: thighs
counter-swinging, arms opposing the legs, spine twisting, tail trailing.

    blender --background --python tools/preview_rig.py
"""

import math
import os

import bpy

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RIGGED = os.path.join(REPO, "assets", "characters", "base_rigged.glb")
OUT = os.environ.get("RIG_PREVIEW_DIR", os.path.join(REPO, "target", "rig-preview"))

# Amplitudes copied from `entities::rig::swing`, so the preview is the game's cycle and not a
# prettier stand-in.
THIGH, SHIN, ARM, FOREARM, SPINE, TAIL = 0.42, 0.30, 0.40, 0.18, 0.16, 0.22
ARMS_DOWN = float(os.environ.get("ARMS_DOWN", "-1.00"))
# Reaction amplitudes, mirroring `entities::rig::swing`.
KICK_LEG, KICK_PLANT, KICK_ARMS, KICK_TWIST = 1.35, 0.35, 0.55, 0.30
HIT_ARMS, HIT_TORSO, HIT_LEGS = 0.95, 0.45, 0.50


def log(m):
    print(f"[preview] {m}", flush=True)


def pose(armature, gait, kick=0.0, hit=0.0):
    """Apply one frame of the cycle at phase `gait`, with optional reactions layered on."""
    step = math.sin(gait)
    counter = -step
    # Blender XYZ euler = Rz*Ry*Rx in bone-local space, i.e. lower on X first then swing on Z,
    # which is the same composition the game applies.
    plan = {
        "thigh.L": ("X", -step * THIGH + kick * KICK_PLANT + hit * HIT_LEGS),
        "thigh.R": ("X", -counter * THIGH - kick * KICK_LEG + hit * HIT_LEGS),
        "shin.L": ("X", max(-step, 0.0) * SHIN),
        "shin.R": ("X", max(-counter, 0.0) * SHIN),
        "upper_arm.L": ("XZ", (ARMS_DOWN + hit * HIT_ARMS, step * ARM + kick * KICK_ARMS)),
        "upper_arm.R": ("XZ", (ARMS_DOWN + hit * HIT_ARMS, step * ARM + kick * KICK_ARMS)),
        "spine": ("Z", step * SPINE - kick * KICK_TWIST),
        "tail.01": ("Z", math.sin(gait - 0.6) * TAIL),
        "tail.02": ("Z", math.sin(gait - 1.2) * TAIL),
        "tail.03": ("Z", math.sin(gait - 1.8) * TAIL),
    }
    for bone in armature.pose.bones:
        bone.rotation_mode = "XYZ"
        bone.rotation_euler = (0.0, 0.0, 0.0)
    posed = 0
    for name, (axis, radians) in plan.items():
        bone = armature.pose.bones.get(name)
        if bone is None:
            log(f"WARNING no bone {name}")
            continue
        euler = [0.0, 0.0, 0.0]
        if axis == "XZ":
            euler[0], euler[2] = radians
        else:
            euler["XYZ".index(axis)] = radians
        bone.rotation_euler = euler
        posed += 1
    return posed


def main():
    os.makedirs(OUT, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=RIGGED)

    # Blender's factory startup can leave an object behind even with `use_empty`; anything that
    # is not part of the imported character would otherwise photobomb the render.
    imported = {o.name for o in bpy.context.scene.objects}
    for obj in list(bpy.context.scene.objects):
        if obj.type == "MESH" and not obj.vertex_groups:
            log(f"removing stray {obj.name!r} from the preview scene")
            bpy.data.objects.remove(obj, do_unlink=True)
    _ = imported

    armatures = [o for o in bpy.context.scene.objects if o.type == "ARMATURE"]
    if not armatures:
        raise SystemExit("the imported file has no armature -- the rig did not survive export")
    armature = armatures[0]
    log(f"armature {armature.name!r} with {len(armature.pose.bones)} pose bones")

    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    for mesh in meshes:
        skinned = any(m.type == "ARMATURE" for m in mesh.modifiers)
        log(f"mesh {mesh.name!r}: armature-modified={skinned} groups={len(mesh.vertex_groups)}")

    # A plain three-quarter view, lit flatly: this is about the silhouette, not the lighting.
    camera_data = bpy.data.cameras.new("Cam")
    camera = bpy.data.objects.new("Cam", camera_data)
    bpy.context.scene.collection.objects.link(camera)
    camera.location = (0.0, -3.6, 0.95)
    camera.rotation_euler = (math.radians(82), 0.0, 0.0)
    bpy.context.scene.camera = camera

    sun_data = bpy.data.lights.new("Sun", type="SUN")
    sun_data.energy = 4.0
    sun = bpy.data.objects.new("Sun", sun_data)
    sun.rotation_euler = (math.radians(50), 0.0, math.radians(30))
    bpy.context.scene.collection.objects.link(sun)
    bpy.context.scene.world = bpy.data.worlds.new("W")
    bpy.context.scene.world.use_nodes = True
    bpy.context.scene.world.node_tree.nodes["Background"].inputs[0].default_value = (
        0.55, 0.62, 0.70, 1.0
    )

    scene = bpy.context.scene
    try:
        scene.render.engine = "BLENDER_EEVEE_NEXT"
    except TypeError:
        scene.render.engine = "BLENDER_WORKBENCH"
    scene.render.resolution_x = 520
    scene.render.resolution_y = 640
    scene.render.film_transparent = False

    # First a true bind pose, as a control: if the model looks wrong with every joint at rest,
    # the fault is in the binding or the render, not in the walk cycle.
    for bone in armature.pose.bones:
        bone.rotation_mode = "XYZ"
        bone.rotation_euler = (0.0, 0.0, 0.0)
    bpy.context.view_layer.update()
    scene.render.filepath = os.path.join(OUT, "bind.png")
    bpy.ops.render.render(write_still=True)
    log("bind pose (control) -> bind.png")

    # Four phases across one stride: contact, passing, contact, passing.
    for index, gait in enumerate([0.0, math.pi / 2, math.pi, 3 * math.pi / 2]):
        posed = pose(armature, gait)
        bpy.context.view_layer.update()
        path = os.path.join(OUT, f"pose-{index}.png")
        scene.render.filepath = path
        bpy.ops.render.render(write_still=True)
        log(f"gait={gait:.2f} posed {posed} bones -> {path}")

    # And the two one-shot reactions, at full strength.
    for label, kwargs in [("kick", {"kick": 1.0}), ("hit", {"hit": 1.0})]:
        pose(armature, math.pi / 2, **kwargs)
        bpy.context.view_layer.update()
        scene.render.filepath = os.path.join(OUT, f"{label}.png")
        bpy.ops.render.render(write_still=True)
        log(f"{label} at full strength -> {label}.png")


if __name__ == "__main__":
    main()
