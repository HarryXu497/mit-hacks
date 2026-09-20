"""SUPERSEDED -- do not use for dressing characters. See `blender_assemble_character.py`.

This script exports the base to GLB. That round trip is what destroyed the model: the OBJ's
`map_d` alpha cutout does not survive it, so the eyelid quad turns into an opaque tan rectangle
across the face and the sampled UVs shift. `blender_bisect_pipeline.py` proved it -- stages 0-3
render correctly, stage 4 (export + re-import) does not.

Everything below that looks like a fix for eyes or UVs -- `flip_v_in_tile`, `remap_eyelids_to_open`,
`OPEN_EYES_UV`, `wire_alpha` -- is compensating for that damage, not correcting a fault in the
source art. Reusing any of it on an undamaged base will break the base.

`blender_assemble_character.py` never round-trips: it imports the OBJ once, dresses it, places
accessories and writes the result. It is the supported path. This file is kept only because it is
still the reference for how the seven SOCKET_* empties are derived from mesh extremes.

---

Turn the BTD6 dart monkey into a MonkeyForge base: scaled, socketed, GLB.

The procedural bases carry no UVs and no texture, so a garment can only ever be a flat tint on the
torso. This mesh has both, which is what makes layered clothing possible at all.

Two things must be added for it to work as a base:

  * scale -- the source is ~25 units tall while the compiler's sizing assumes the ~1.7 unit bases
  * sockets -- the seven SOCKET_* empties the compiler parents accessories to

Socket positions are derived from the mesh's own vertex extremes rather than typed in, so they stay
correct if the source mesh is ever swapped or re-exported.

Run:
    blender --background --python scripts/blender_build_btd6_base.py -- \
        --input "assets/reference/btd6_extract/Dart Monkey/dartmonkey.obj" \
        --output assets/base/btd6_dartmonkey.glb
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

TARGET_HEIGHT = 1.72  # matches the procedural bases, so accessory sizing carries over unchanged

SOCKET_NAMES = (
    "SOCKET_HEAD_TOP",
    "SOCKET_FACE",
    "SOCKET_BACK",
    "SOCKET_WAIST",
    "SOCKET_HAND_LEFT",
    "SOCKET_HAND_RIGHT",
    "SOCKET_TAIL_TIP",
)


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--drop-prop", action="store_true", default=True,
                        help="remove the held dart, which is a tower prop not part of the body")
    # Off by default. The body mesh's UVs occupy a single 0.25 window of the atlas (u 0.00-0.25,
    # v 0.26-0.50), so they are already atlas-aware and pointing them at a standalone 512 skin
    # samples a corner of it instead. Only useful if a source ever ships un-atlased UVs.
    parser.add_argument("--skin", default=None,
                        help="override: retarget atlas materials to this single-skin image")
    return parser.parse_args(arguments)


def body_meshes() -> list[bpy.types.Object]:
    return [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]


#: Where the open eyes sit on DartMonkeyDiffuse.png, as a UV box. The eyelid mesh ships mapped to
#: the *closed* sprite (u 0.71-0.83, v 0.64-0.78) because the game animates the offset to blink;
#: at rest the character should have its eyes open.
OPEN_EYES_UV = (0.234, 0.795, 0.420, 0.893)


def remap_eyelids_to_open(mesh: bpy.types.Object, box: tuple[float, float, float, float]) -> None:
    """Move the eyelid mesh's UVs from the blink sprite to the open-eye sprite.

    Deleting the mesh instead leaves the monkey with no eyes at all -- they are not painted on the
    body texture.

    Both eye quads share a single closed-eye sprite, so they must be remapped *per side*: the open
    sprite holds two eyes side by side, and stretching the combined bounding box across it would
    smear each quad over both eyes. Faces are split by their 3D x position and each half is fitted
    to its own eye.
    """
    uv_layer = mesh.data.uv_layers.active
    if uv_layer is None:
        return

    target_u0, target_v0, target_u1, target_v1 = box
    mid_u = (target_u0 + target_u1) / 2
    halves = {
        "left": (target_u0, mid_u),
        "right": (mid_u, target_u1),
    }

    # Group loops by which side of the head their face sits on.
    sides: dict[str, list[int]] = {"left": [], "right": []}
    for poly in mesh.data.polygons:
        centre_x = sum(mesh.data.vertices[v].co.x for v in poly.vertices) / len(poly.vertices)
        side = "left" if centre_x >= 0 else "right"
        sides[side].extend(poly.loop_indices)

    for side, loops in sides.items():
        if not loops:
            continue
        us = [uv_layer.data[i].uv[0] for i in loops]
        vs = [uv_layer.data[i].uv[1] for i in loops]
        u0, u1 = min(us), max(us)
        v0, v1 = min(vs), max(vs)
        u_span = max(u1 - u0, 1e-6)
        v_span = max(v1 - v0, 1e-6)
        eye_u0, eye_u1 = halves[side]

        for i in loops:
            u, v = uv_layer.data[i].uv
            uv_layer.data[i].uv = (
                eye_u0 + (u - u0) / u_span * (eye_u1 - eye_u0),
                target_v0 + (v - v0) / v_span * (target_v1 - target_v0),
            )
        print(f"MONKEYFORGE_BASE remapped {mesh.name} {side} eye "
              f"({len(loops)} loops) -> u[{eye_u0:.3f},{eye_u1:.3f}]")


def retarget_atlas(skin_image: str) -> None:
    """Point body materials at a single skin rather than the multi-skin atlas.

    The MTL assigns a 2048x2048 atlas (a 4x4 grid of 512x512 alternate skins) to the body, but the
    mesh's UVs span roughly the full 0-1 range -- they were authored for one 512 skin. Sampling the
    atlas with them lands every face on a different tile, which is why the face renders as
    unrecognisable blobs. Each tile is a complete skin, so swapping the image is the whole fix; the
    UVs are already correct.
    """
    target = bpy.data.images.get(skin_image)
    if target is None:
        print(f"MONKEYFORGE_BASE skin image not found: {skin_image}")
        return

    for material in bpy.data.materials:
        if not material.use_nodes:
            continue
        for node in material.node_tree.nodes:
            if node.type != "TEX_IMAGE" or node.image is None:
                continue
            if node.image.name == skin_image:
                continue
            # An atlas is square and several times the size of a single skin.
            width, height = node.image.size
            if width >= 1024 and width == height:
                print(f"MONKEYFORGE_BASE {material.name}: {node.image.name} -> {skin_image}")
                node.image = target


def wire_alpha() -> None:
    """Connect each texture's alpha to its BSDF and set a blend mode.

    The MTL declares `map_d`, giving the eyelid material a cutout so only the eye shape draws. The
    OBJ importer leaves that alpha unconnected, and glTF exports only what is wired -- so after a
    round trip the eyelid quad becomes fully opaque and renders as a tan rectangle across the face.
    Bisecting the pipeline showed this is the step that breaks the model; everything before the GLB
    export renders correctly.
    """
    for material in bpy.data.materials:
        if not material.use_nodes:
            continue
        tree = material.node_tree
        bsdf = next((n for n in tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
        image_node = next((n for n in tree.nodes if n.type == "TEX_IMAGE" and n.image), None)
        if bsdf is None or image_node is None:
            continue
        if not bsdf.inputs["Alpha"].is_linked:
            tree.links.new(image_node.outputs["Alpha"], bsdf.inputs["Alpha"])
            print(f"MONKEYFORGE_BASE wired alpha for {material.name}")
        # Blender 4.2+ renamed the EEVEE blend control; set whichever exists.
        for attribute, value in (("blend_method", "BLEND"), ("surface_render_method", "BLENDED")):
            if hasattr(material, attribute):
                try:
                    setattr(material, attribute, value)
                except TypeError:
                    pass


def flatten_materials() -> None:
    """Strip the specular the OBJ ships with.

    The MTL declares `Ns 180` and `Ks 1.0`, which Blender turns into a hard gloss -- the model reads
    as wet plastic. BTD6 art is flat and unlit, so the texture should carry all of the shading and
    the surface should contribute none.
    """
    for material in bpy.data.materials:
        if not material.use_nodes:
            material.specular_intensity = 0.0
            material.roughness = 1.0
            continue
        for node in material.node_tree.nodes:
            if node.type != "BSDF_PRINCIPLED":
                continue
            for name, value in (
                ("Specular IOR Level", 0.0),
                ("Specular", 0.0),
                ("Roughness", 1.0),
                ("Metallic", 0.0),
                ("Sheen Weight", 0.0),
                ("Coat Weight", 0.0),
            ):
                if name in node.inputs:
                    node.inputs[name].default_value = value
        print(f"MONKEYFORGE_BASE flattened material {material.name}")


def world_vertices(objects: list[bpy.types.Object]) -> list[Vector]:
    out: list[Vector] = []
    for obj in objects:
        matrix = obj.matrix_world
        out.extend(matrix @ v.co for v in obj.data.vertices)
    return out


def bounds(points: list[Vector]) -> tuple[Vector, Vector]:
    lo = Vector((min(p.x for p in points), min(p.y for p in points), min(p.z for p in points)))
    hi = Vector((max(p.x for p in points), max(p.y for p in points), max(p.z for p in points)))
    return lo, hi


def flip_v_in_tile(mesh: bpy.types.Object, offset: float = 0.0) -> None:
    """Mirror UVs vertically inside their atlas tile.

    The source OBJ was exported with a top-down V convention while Blender reads V bottom-up, so the
    artwork lands upside down *within its own tile*: a UV probe showed the face sampling v 0.26-0.37
    while the eyes sit at v 0.45-0.47. Mirroring about the tile's own centre (v' = v0 + v1 - v) puts
    them back without moving the mesh to a different tile.
    """
    uv_layer = mesh.data.uv_layers.active
    if uv_layer is None:
        return
    vs = [uv_layer.data[i].uv[1] for i in range(len(uv_layer.data))]
    v_lo, v_hi = min(vs), max(vs)

    # Snap to the enclosing quarter-tile so the mirror axis is the tile's centre, not the artwork's
    # bounding box -- the mesh does not necessarily touch every edge of its tile.
    tile_v0 = math.floor(v_lo * 4) / 4
    tile_v1 = tile_v0 + 0.25
    axis = tile_v0 + tile_v1

    # `offset` slides the mirrored artwork up or down within the tile. The mirror alone put the eyes
    # noticeably low on the face, and a small shift is cheaper to tune than re-unwrapping a mesh
    # that is not ours.
    for i in range(len(uv_layer.data)):
        u, v = uv_layer.data[i].uv
        uv_layer.data[i].uv = (u, axis - v + offset)
    print(f"MONKEYFORGE_BASE flipped V for {mesh.name} in tile "
          f"[{tile_v0:.3f},{tile_v1:.3f}] offset={offset:+.4f} "
          f"(was v[{v_lo:.3f},{v_hi:.3f}])")


def make_socket(name: str, location: Vector) -> bpy.types.Object:
    empty = bpy.data.objects.new(name, None)
    empty.empty_display_type = "PLAIN_AXES"
    empty.empty_display_size = 0.08
    empty.location = location
    bpy.context.scene.collection.objects.link(empty)
    return empty


def derive_sockets(points: list[Vector]) -> dict[str, Vector]:
    """Locate each socket from the mesh's own extremes.

    The monkey faces -Y (its eyelids sit at negative Y), so "front" is minimum Y throughout.
    """
    lo, hi = bounds(points)
    height = hi.z - lo.z
    centre_x = (lo.x + hi.x) / 2

    def slab(z_lo: float, z_hi: float) -> list[Vector]:
        band = [p for p in points if z_lo <= p.z <= z_hi]
        return band or points

    # Head occupies roughly the top third; take the real extremes inside that slab rather than
    # assuming the bounding box corners belong to the head.
    head = slab(lo.z + height * 0.60, hi.z)
    head_lo, head_hi = bounds(head)
    torso = slab(lo.z + height * 0.28, lo.z + height * 0.62)
    torso_lo, torso_hi = bounds(torso)

    # Hands are the widest points of the arms, which sit around mid-body.
    arms = slab(lo.z + height * 0.25, lo.z + height * 0.65)
    left = max(arms, key=lambda p: p.x)
    right = min(arms, key=lambda p: p.x)

    # The tail trails behind and low: furthest +Y in the bottom half.
    lower = slab(lo.z, lo.z + height * 0.55)
    tail = max(lower, key=lambda p: p.y)

    # The topmost vertex is the hair tuft, not the skull. A hat placed there floats above the head
    # with a visible gap, so take a high percentile of the head's vertices instead: that lands on
    # the crown, where a cap actually sits, and the tuft pokes through it as it should.
    # The compiler seats an accessory's *bottom* on the socket, so a hat placed at the crown floats
    # with its rim resting on the very top of the skull. Dropping the socket to roughly two thirds
    # of head height lets the brim sit around the head the way a cap is worn.
    head_zs = sorted(p.z for p in head)
    crown_z = head_zs[int(len(head_zs) * 0.62)] if head_zs else hi.z

    return {
        "SOCKET_HEAD_TOP": Vector((centre_x, (head_lo.y + head_hi.y) / 2, crown_z)),
        "SOCKET_FACE": Vector((centre_x, head_lo.y, (head_lo.z + head_hi.z) / 2)),
        "SOCKET_BACK": Vector((centre_x, torso_hi.y, (torso_lo.z + torso_hi.z) / 2)),
        "SOCKET_WAIST": Vector((centre_x, (torso_lo.y + torso_hi.y) / 2, torso_lo.z)),
        "SOCKET_HAND_LEFT": Vector((left.x, left.y, left.z)),
        "SOCKET_HAND_RIGHT": Vector((right.x, right.y, right.z)),
        "SOCKET_TAIL_TIP": Vector((tail.x, tail.y, tail.z)),
    }


def main() -> None:
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.wm.obj_import(filepath=str(args.input.resolve()))

    # Two meshes ship with the tower that do not belong on a character base:
    #
    #   DartMonkeyDart      the thrown dart -- a tower prop, not anatomy
    #   MonkeyEyelidsGeo    a blink overlay. Its UVs point at the *closed*-eye sprite
    #                       (u 0.71-0.83, v 0.64-0.78), so leaving it in renders the monkey
    #                       permanently blinking and hides the open eyes painted on the body.
    for obj in list(bpy.context.scene.objects):
        if obj.type != "MESH":
            continue
        name = obj.name.lower()
        if "dart" in name and "flatskin" not in name and args.drop_prop:
            # The thrown dart is a tower prop, not anatomy.
            print(f"MONKEYFORGE_BASE dropping prop mesh {obj.name}")
            bpy.data.objects.remove(obj, do_unlink=True)
        elif "eyelid" in name:
            remap_eyelids_to_open(obj, OPEN_EYES_UV)

    meshes = body_meshes()
    if not meshes:
        raise SystemExit("no meshes imported")

    if args.skin:
        retarget_atlas(args.skin)
    flatten_materials()

    points = world_vertices(meshes)
    lo, hi = bounds(points)
    source_height = hi.z - lo.z
    scale = TARGET_HEIGHT / source_height
    print(f"MONKEYFORGE_BASE source_height={source_height:.3f} scale={scale:.5f}")

    # Scale about the origin and drop the feet to z=0 so the character stands on the ground plane.
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
        print(f"MONKEYFORGE_SOCKET {name} "
              f"({location.x:.4f},{location.y:.4f},{location.z:.4f})")

    missing = [n for n in SOCKET_NAMES if n not in bpy.data.objects]
    if missing:
        raise SystemExit(f"failed to create sockets: {missing}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(args.output),
        export_format="GLB",
        export_apply=True,
        export_image_format="AUTO",
    )
    print(f"MONKEYFORGE_BASE wrote {args.output}")


if __name__ == "__main__":
    main()
