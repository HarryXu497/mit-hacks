"""Give the DartMonkey a skeleton.

`assets/characters/base.glb` is a single rigid mesh with its T-pose baked into the vertices: no
skin, no joints, no clips. Nothing in the game can animate it, because there are no limbs to move
-- only a lump to bounce. This builds a 21-joint humanoid armature, binds the mesh to it, and
re-exports, after which `entities::rig` drives a real walk cycle over it.

Run under Blender, headless:

    blender --background --python tools/rig_dartmonkey.py

Bone positions are not guessed. They come from slicing the mesh's own vertex cloud along Y and
reading off where the geometry actually is (`tools/`-adjacent analysis): the face is +Z, the arms
show as a widening to x = +/-0.627 across y 0.42..0.63, the head is the mass from y 0.65 to 1.25,
the legs are stubby clusters near x = +/-0.09 below y 0.30, and the tail sweeps back and down to
z = -0.50.

Joint names match what `entities::rig::Joint::from_name` looks for -- Blender's own `.L`/`.R`
convention and `tail.01`..`tail.03` -- which is also what the MonkeyForge rigs use, so one
animator serves both.
"""

import math
import os
import sys

import bpy
from mathutils import Vector

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCE = os.path.join(REPO, "assets", "characters", "base.glb")
TARGET = os.path.join(REPO, "assets", "characters", "base_rigged.glb")

# The model is authored 1.4 units tall with its feet on y=0 and its face towards +Z.
FACE = 1.0  # sign of +Z


def g2b(x, y, z):
    """glTF coordinates to Blender's.

    The glTF importer converts Y-up to Blender's Z-up, so a point at glTF (x, y, z) lands at
    Blender (x, -z, y). Bone positions are quoted in glTF space below, because that is the space
    the measurements were taken in and the space the game sees.
    """
    return Vector((x, -z, y))


# (name, parent, head, tail) with head/tail in glTF space.
#
# `root` and `hips` carry the body; `spine`/`chest` give the torso something to twist against;
# arms run out along +/-X because the mesh is in a T-pose, which is exactly the rest pose a
# procedural swing wants to offset from. The character faces +Z, so its left is +X.
SKELETON = [
    ("root",         None,           (0.0, 0.00, 0.0),        (0.0, 0.10, 0.0)),
    ("hips",         "root",         (0.0, 0.30, 0.0),        (0.0, 0.40, 0.0)),
    ("spine",        "hips",         (0.0, 0.40, 0.0),        (0.0, 0.48, 0.0)),
    ("chest",        "spine",        (0.0, 0.48, 0.0),        (0.0, 0.60, 0.0)),
    ("neck",         "chest",        (0.0, 0.60, 0.0),        (0.0, 0.66, 0.0)),
    ("head",         "neck",         (0.0, 0.66, 0.0),        (0.0, 1.25, 0.0)),

    # Arms, out along the T-pose. Split at the elbow so a forearm can bend independently.
    ("upper_arm.L",  "chest",        (0.13, 0.555, 0.0),      (0.36, 0.555, 0.0)),
    ("forearm.L",    "upper_arm.L",  (0.36, 0.555, 0.0),      (0.54, 0.555, 0.0)),
    ("hand.L",       "forearm.L",    (0.54, 0.555, 0.0),      (0.627, 0.555, 0.0)),
    ("upper_arm.R",  "chest",        (-0.13, 0.555, 0.0),     (-0.36, 0.555, 0.0)),
    ("forearm.R",    "upper_arm.R",  (-0.36, 0.555, 0.0),     (-0.54, 0.555, 0.0)),
    ("hand.R",       "forearm.R",    (-0.54, 0.555, 0.0),     (-0.627, 0.555, 0.0)),

    # Legs. Short and stubby on this model: hip at 0.30, knee at 0.16, ankle at 0.05, with the
    # foot pointing forward into +Z where the toe geometry sits.
    ("thigh.L",      "hips",         (0.09, 0.30, 0.0),       (0.09, 0.16, 0.0)),
    ("shin.L",       "thigh.L",      (0.09, 0.16, 0.0),       (0.09, 0.05, 0.0)),
    ("foot.L",       "shin.L",       (0.09, 0.05, 0.0),       (0.09, 0.02, 0.18 * FACE)),
    ("thigh.R",      "hips",         (-0.09, 0.30, 0.0),      (-0.09, 0.16, 0.0)),
    ("shin.R",       "thigh.R",      (-0.09, 0.16, 0.0),      (-0.09, 0.05, 0.0)),
    ("foot.R",       "shin.R",       (-0.09, 0.05, 0.0),      (-0.09, 0.02, 0.18 * FACE)),

    # The tail, sweeping back and down behind the hips in three segments so it can whip.
    ("tail.01",      "hips",         (0.0, 0.30, -0.06),      (0.0, 0.27, -0.21)),
    ("tail.02",      "tail.01",      (0.0, 0.27, -0.21),      (0.0, 0.24, -0.36)),
    ("tail.03",      "tail.02",      (0.0, 0.24, -0.36),      (0.0, 0.21, -0.50)),
]


def log(message):
    print(f"[rig] {message}", flush=True)


def clear_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_source():
    if not os.path.exists(SOURCE):
        raise SystemExit(f"missing {SOURCE}")
    bpy.ops.import_scene.gltf(filepath=SOURCE)
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    if not meshes:
        raise SystemExit("imported no meshes")
    for mesh in meshes:
        log(f"mesh {mesh.name!r}: {len(mesh.data.vertices)} verts, "
            f"materials {[m.name for m in mesh.data.materials]}")
    return meshes


def build_armature():
    """Create the armature and its bones, in Blender space."""
    armature_data = bpy.data.armatures.new("MonkeyRig")
    armature = bpy.data.objects.new("MonkeyRig", armature_data)
    bpy.context.scene.collection.objects.link(armature)

    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.mode_set(mode="EDIT")

    created = {}
    for name, parent, head, tail in SKELETON:
        bone = armature_data.edit_bones.new(name)
        bone.head = g2b(*head)
        bone.tail = g2b(*tail)
        # A zero-length bone is silently dropped by the exporter, which would lose the joint.
        if (bone.tail - bone.head).length < 1e-4:
            raise SystemExit(f"bone {name} has no length")
        if parent is not None:
            bone.parent = created[parent]
            # Not connected: several children share a parent's head (both thighs on the hips,
            # both arms on the chest), and connecting them would drag the parent's tail around.
            bone.use_connect = False
        created[name] = bone

    bpy.ops.object.mode_set(mode="OBJECT")
    log(f"armature built: {len(armature_data.bones)} bones")
    return armature


def islands(mesh):
    """The connected vertex groups of a mesh, as lists of vertex indices.

    This model is built from separate blocky pieces -- head, torso, two arms, two hands, two
    legs, tail segments -- in the style of the artwork, where the hands genuinely float away from
    the arms. That is what makes heat-diffusion weighting the wrong tool: it wants a continuous
    surface to spread across, and given islands it smears each one across whichever bones happen
    to be near, which tore limbs off the body.
    """
    parent = list(range(len(mesh.data.vertices)))

    def find(a):
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    for edge in mesh.data.edges:
        a, b = (find(v) for v in edge.vertices)
        if a != b:
            parent[a] = b

    groups = {}
    for index in range(len(mesh.data.vertices)):
        groups.setdefault(find(index), []).append(index)
    return list(groups.values())


def nearest_bone(point, armature):
    """The bone whose segment passes closest to `point`, in the armature's space."""
    best, best_distance = None, float("inf")
    for bone in armature.data.bones:
        # `neutral_bone` is the exporter's own filler and drives nothing.
        if bone.name in ("root", "neutral_bone"):
            continue
        head, tail = bone.head_local, bone.tail_local
        span = tail - head
        length_squared = span.length_squared
        if length_squared < 1e-9:
            continue
        # Closest point on the segment, clamped to its ends.
        t = max(0.0, min(1.0, (point - head).dot(span) / length_squared))
        distance = (head + span * t - point).length
        if distance < best_distance:
            best, best_distance = bone, distance
    return best, best_distance


def bind(meshes, armature):
    """Bind each mesh to the armature, one rigid piece per island.

    Every island is assigned whole to the single bone nearest its centroid, at weight 1.0. For a
    character made of separate solid pieces this is both simpler and better-looking than smooth
    weights: an arm rotates as an arm, and nothing can be half-claimed by the bone next door.

    The eyelids are their own mesh floating in front of the face and go rigidly to the head.
    """
    for mesh in meshes:
        modifier = mesh.modifiers.new(name="Armature", type="ARMATURE")
        modifier.object = armature
        mesh.parent = armature
        mesh.matrix_parent_inverse = armature.matrix_world.inverted()

        if "eyelid" in mesh.name.lower():
            group = mesh.vertex_groups.new(name="head")
            group.add(range(len(mesh.data.vertices)), 1.0, "REPLACE")
            log(f"{mesh.name!r}: bound rigidly to the head")
            continue

        pieces = islands(mesh)
        log(f"{mesh.name!r}: {len(pieces)} islands")
        made = {}
        for piece in sorted(pieces, key=len, reverse=True):
            centroid = Vector((0.0, 0.0, 0.0))
            for index in piece:
                centroid += mesh.data.vertices[index].co
            centroid /= len(piece)
            bone, distance = nearest_bone(centroid, armature)
            if bone is None:
                continue

            # Rigid pieces bind to the *proximal* joint of the limb they belong to, not to
            # whichever bone happens to be nearest.
            #
            # A limb here is one solid island: the whole arm, hand included, is a single 43-vertex
            # piece. Bound by proximity it lands on `forearm`, and then rotating the shoulder
            # swings the piece about the elbow -- which tears its shoulder end away from the body.
            # Bound to `upper_arm` the same piece rotates about the shoulder, which is what a
            # rigid arm does.
            #
            # Blender axes here: Z is glTF Y (height), +Y is glTF -Z (behind the monkey).
            height = centroid.z
            behind = centroid.y > 0.10
            side = "L" if centroid.x > 0.0 else "R"

            if height > 0.66:
                # Anything above the neck is part of the head: the skull, the ears, the hair
                # tuft. The ears sit out at x = +/-0.30 and y = 0.77, which is outside the arm
                # band but close enough to a shoulder that nearest-bone welded them to the arm --
                # so they were being flung around by it. That was the detached plate at the
                # shoulder.
                bone = armature.data.bones["head"]
                distance = 0.0
            elif behind and height < 0.40:
                # The tail, which sweeps back and down from the hips.
                bone = armature.data.bones["tail.02"]
                distance = 0.0
            elif abs(centroid.x) > 0.20 and 0.40 <= height <= 0.72:
                # An arm: out to the side, at shoulder height.
                bone = armature.data.bones[f"upper_arm.{side}"]
                distance = 0.0
            elif height < 0.30 and abs(centroid.x) > 0.02:
                # A leg. This model has no thigh or shin geometry of its own -- just short
                # forward-pointing stubs -- so the whole leg goes to the thigh.
                bone = armature.data.bones[f"thigh.{side}"]
                distance = 0.0

            group = made.get(bone.name) or mesh.vertex_groups.new(name=bone.name)
            made[bone.name] = group
            group.add(piece, 1.0, "REPLACE")
            log(f"    island of {len(piece):4} verts -> {bone.name:<13} "
                f"(centroid {centroid.x:+.2f},{centroid.y:+.2f},{centroid.z:+.2f}, "
                f"{distance:.3f} away)")


def report_weights(meshes):
    """Say which bones actually got weight, so a silent bind failure is visible."""
    for mesh in meshes:
        if not mesh.vertex_groups:
            log(f"WARNING {mesh.name!r} has no vertex groups -- it will not deform")
            continue
        totals = {group.name: 0.0 for group in mesh.vertex_groups}
        for vertex in mesh.data.vertices:
            for element in vertex.groups:
                name = mesh.vertex_groups[element.group].name
                totals[name] = totals.get(name, 0.0) + element.weight
        driven = {k: v for k, v in totals.items() if v > 0.01}
        log(f"{mesh.name!r} weighted bones ({len(driven)}): "
            + ", ".join(f"{k}={v:.1f}" for k, v in sorted(driven.items(), key=lambda kv: -kv[1])))
        if "eyelid" in mesh.name.lower():
            continue
        # The joints the walk cycle actually drives. Any of these driving nothing means a limb
        # that will not move, which is the whole point of the exercise.
        for wanted in ("thigh.L", "thigh.R", "upper_arm.L", "upper_arm.R", "head"):
            if totals.get(wanted, 0.0) <= 0.01:
                log(f"WARNING {wanted} drives nothing on {mesh.name!r}")


def export():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=TARGET,
        export_format="GLB",
        use_selection=True,
        export_skins=True,
        export_animations=False,
        # Keep the artwork exactly as it came in: this pass is about adding a skeleton, not about
        # changing how the monkey looks.
        export_materials="EXPORT",
        export_yup=True,
        export_apply=False,
    )
    log(f"wrote {TARGET} ({os.path.getsize(TARGET)} bytes)")


def main():
    clear_scene()
    meshes = import_source()
    armature = build_armature()
    bind(meshes, armature)
    report_weights(meshes)
    export()


if __name__ == "__main__":
    main()
