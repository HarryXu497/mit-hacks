"""Assemble a character in one Blender pass: untouched base + garment texture + socketed accessory.

The previous pipeline exported the base to GLB, re-imported it to compile an accessory, then
exported again. Bisecting showed that round trip is what broke the model -- the OBJ's `map_d` alpha
cutout does not survive it, so the eyelid quad turns into an opaque tan rectangle across the face,
and the sampled UVs shift. Every "fix" after that was compensating for damage this script simply
avoids.

So: import the source OBJ once, dress it, place accessories, and write the result. The base's mesh,
UVs, materials and textures are never re-imported and never rewritten.

Run:
    blender --background --python scripts/blender_assemble_character.py -- \
        --base "<dartmonkey.obj>" [--texture <painted.png>] \
        [--accessory <cap.glb> --socket SOCKET_HEAD_TOP --accessory-size 0.9] \
        [--glb out.glb] [--render out.png --view three_quarter]
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

VIEWS = {"front": 0.0, "side": 90.0, "back": 180.0, "three_quarter": 35.0}

SOCKET_HEIGHTS = {
    # Fractions of total height, measured from the feet. Derived from the source mesh rather than
    # typed as absolute coordinates so they survive a different base.
    "SOCKET_HEAD_TOP": 0.86,
    "SOCKET_FACE": 0.78,
    "SOCKET_BACK": 0.46,
    "SOCKET_WAIST": 0.30,
    "SOCKET_HAND_LEFT": 0.42,
    "SOCKET_HAND_RIGHT": 0.42,
    "SOCKET_TAIL_TIP": 0.16,
}


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--texture", type=Path, default=None,
                        help="replacement body texture, e.g. one with a garment painted in")
    parser.add_argument("--base-texture", type=Path, default=None,
                        help="replacement artwork for the body itself, e.g. from restyle_base.py. "
                             "Distinct from --texture, which carries the garment")
    parser.add_argument("--shell", type=Path, default=None,
                        help="coverage mask from project_planar. Given one, the garment is built "
                             "as a second layer of geometry standing off the body rather than "
                             "painted into the base texture, so it reads as worn cloth with an "
                             "edge and a shadow instead of a tattoo. The base keeps its own "
                             "artwork underneath, untouched")
    parser.add_argument("--shell-offset", type=float, default=0.014,
                        help="how far the garment stands off the body, as a fraction of height")
    parser.add_argument("--no-shell-rim", action="store_true",
                        help="leave the garment as an open surface. Without the rim it has zero "
                             "thickness and the body shows through at its border")
    parser.add_argument("--height-map", type=Path, default=None,
                        help="height map from garment_height.py; drives a bump node so cloth "
                             "details read as raised or creased rather than printed flat")
    parser.add_argument("--spike", action="append", default=[],
                        choices=sorted(SPIKE_REGIONS),
                        help="grow spikes out of this region of the body; repeatable")
    parser.add_argument("--spike-count", type=int, default=6)
    parser.add_argument("--spike-length", type=float, default=0.075,
                        help="spike length as a fraction of body height")
    parser.add_argument("--spike-radius", type=float, default=0.022,
                        help="spike base radius as a fraction of body height")
    parser.add_argument("--spike-sides", type=int, default=5)
    parser.add_argument("--height-strength", type=float, default=0.8)
    parser.add_argument("--height-distance", type=float, default=0.02,
                        help="bump depth as a fraction of the model's height. Blender's Bump node "
                             "wants scene units, and this base is ~24 units tall, so a raw 0.05 "
                             "is invisible on it and enormous on a 1.7-unit one")
    parser.add_argument("--accessory", type=Path, action="append", default=[])
    parser.add_argument("--socket", action="append", default=[])
    parser.add_argument("--accessory-size", type=float, default=0.9,
                        help="accessory width as a fraction of head width")
    parser.add_argument("--sink", type=float, default=0.35,
                        help="how far an accessory drops onto its anchor, as a fraction of its own "
                             "height; a cap must sit around the skull, not perch on top of it")
    parser.add_argument("--drop-prop", action="store_true", default=True)
    parser.add_argument("--accessory-faces", type=int, default=0,
                        help="collapse each accessory to about this many faces; 0 keeps the "
                             "generator's own density, which is ~20k for a hat")
    parser.add_argument("--pack-texture", type=Path, default=None,
                        help="crop the body atlas to the region this character samples and "
                             "rewrite its UVs to match, then write the cropped sheet here. Same "
                             "pixels, far smaller export")
    parser.add_argument("--normalize-height", type=float, default=0.0,
                        help="scale the finished character to this height and stand it on z=0, "
                             "so the game can scale by its own player size with no per-asset "
                             "constant. 0 leaves the source scale alone")
    parser.add_argument("--glb", type=Path, default=None)
    parser.add_argument("--render", type=Path, default=None)
    parser.add_argument("--view", default="three_quarter", choices=sorted(VIEWS))
    parser.add_argument("--resolution", type=int, default=800)
    return parser.parse_args(arguments)


def body_meshes() -> list[bpy.types.Object]:
    return [o for o in bpy.context.scene.objects if o.type == "MESH"
            and not o.name.startswith("ACC_")]


def world_points(objects: list[bpy.types.Object]) -> list[Vector]:
    return [o.matrix_world @ v.co for o in objects for v in o.data.vertices]


def bounds(points: list[Vector]) -> tuple[Vector, Vector]:
    lo = Vector((min(p.x for p in points), min(p.y for p in points), min(p.z for p in points)))
    hi = Vector((max(p.x for p in points), max(p.y for p in points), max(p.z for p in points)))
    return lo, hi


def swap_texture(path: Path) -> int:
    """Point body materials at a replacement image, leaving UVs and meshes alone."""
    replacement = bpy.data.images.load(str(path.resolve()))
    swapped = 0
    for material in bpy.data.materials:
        if not material.use_nodes:
            continue
        for node in material.node_tree.nodes:
            if node.type != "TEX_IMAGE" or node.image is None:
                continue
            width, height = node.image.size
            # Only the large square atlas carries the body; the eyelid sheet must keep its own art.
            if width >= 1024 and width == height:
                node.image = replacement
                swapped += 1
    return swapped


def build_garment_shell(mask_path: Path, texture_path: Path, offset: float,
                        height_path: Path | None = None,
                        height_strength: float = 0.6,
                        height_distance: float = 0.06,
                        rim: bool = True) -> int:
    """Lift the garment off the body as its own layer of geometry.

    Painting a garment into the body texture makes it look printed on the skin, because that is
    literally what it is: the same surface, the same silhouette, no edge where cloth meets fur.

    This duplicates only the faces the garment covers, pushes them out along their normals, and
    gives the copies the garment texture while the original faces keep the base artwork. The
    result has a real hem, a real collar edge, and self-shadowing -- and the base underneath is
    never modified, so the monkey survives intact if the garment is removed.

    Faces are selected by sampling the coverage mask at each face's UV centre, which is the same
    space the garment was projected into, so the two cannot disagree.
    """
    import bmesh

    mask_image = bpy.data.images.load(str(mask_path.resolve()))
    width, height = mask_image.size
    pixels = list(mask_image.pixels)  # flat RGBA floats; a local list is ~100x faster to index

    def covered(u: float, v: float) -> bool:
        x = min(max(int(u * width), 0), width - 1)
        y = min(max(int(v * height), 0), height - 1)
        return pixels[(y * width + x) * 4] > 0.5

    garment_texture = bpy.data.images.load(str(texture_path.resolve()))
    material = bpy.data.materials.new("Garment")
    material.use_nodes = True
    tree = material.node_tree
    bsdf = next(n for n in tree.nodes if n.type == "BSDF_PRINCIPLED")
    image_node = tree.nodes.new("ShaderNodeTexImage")
    image_node.image = garment_texture
    image_node.interpolation = "Closest"
    tree.links.new(image_node.outputs["Color"], bsdf.inputs["Base Color"])
    for field, value in (("Specular IOR Level", 0.0), ("Roughness", 0.85), ("Metallic", 0.0)):
        if field in bsdf.inputs:
            bsdf.inputs[field].default_value = value

    if height_path is not None:
        # Perturb the surface normal from the height map. This gives folds, a raised tie and
        # recessed seams for the cost of a texture lookup -- no extra geometry, and the low-poly
        # silhouette the game wants is untouched.
        height_image = bpy.data.images.load(str(height_path.resolve()))
        height_image.colorspace_settings.name = "Non-Color"  # heights are data, not colour
        height_node = tree.nodes.new("ShaderNodeTexImage")
        height_node.image = height_image
        bump = tree.nodes.new("ShaderNodeBump")
        bump.inputs["Strength"].default_value = height_strength
        bump.inputs["Distance"].default_value = height_distance
        tree.links.new(height_node.outputs["Color"], bump.inputs["Height"])
        tree.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
        print(f"ASSEMBLE bump from {height_path.name} "
              f"(strength {height_strength}, distance {height_distance})")

    total = 0
    for obj in body_meshes():
        mesh = obj.data
        if not mesh.uv_layers.active:
            continue

        mesh.materials.append(material)
        slot = len(mesh.materials) - 1

        bm = bmesh.new()
        bm.from_mesh(mesh)
        uv_layer = bm.loops.layers.uv.active
        if uv_layer is None:
            bm.free()
            continue

        chosen = []
        for face in bm.faces:
            us = [loop[uv_layer].uv for loop in face.loops]
            centre_u = sum(uv.x for uv in us) / len(us)
            centre_v = sum(uv.y for uv in us) / len(us)
            if covered(centre_u, centre_v):
                chosen.append(face)
        if not chosen:
            bm.free()
            continue

        result = bmesh.ops.duplicate(bm, geom=chosen)
        new_faces = [item for item in result["geom"] if isinstance(item, bmesh.types.BMFace)]
        # Move each duplicated vertex along its own normal so the shell follows the body's shape
        # instead of ballooning outwards from a centre.
        moved = set()
        for face in new_faces:
            face.material_index = slot
            moved.update(face.verts)
        # Remember where each vertex started, so the rim can be bridged back down to the body.
        seated = {vert: vert.co.copy() for vert in moved}
        for vert in moved:
            vert.co += vert.normal * offset

        # A vertex's UV, taken from the shell's own loops, so the wall built under it samples the
        # same cloth. New verts default to UV (0, 0) -- the texture's corner, which on this atlas
        # is dark -- and that paints a black line around every hem and cuff.
        vert_uv = {}
        for face in new_faces:
            for loop in face.loops:
                vert_uv.setdefault(loop.vert, loop[uv_layer].uv.copy())

        if rim:
            # Close the edge of the shell down to the body.
            #
            # Without this the garment is an open surface: correct from the front, and from any
            # other angle a sheet of zero thickness floating off the monkey, with the fur visible
            # through the gap at its border. Bridging every boundary edge back to where it started
            # turns the layer into a solid with a hem you can see -- which is most of what makes
            # it read as worn cloth rather than a decal.
            new_face_set = set(new_faces)
            boundary = [edge for edge in {e for face in new_faces for e in face.edges}
                        if sum(1 for face in edge.link_faces if face in new_face_set) == 1]
            skirt = 0
            for edge in boundary:
                a, b = edge.verts
                if a not in seated or b not in seated:
                    continue
                try:
                    wall = bm.faces.new((a, b, bm.verts.new(seated[b]), bm.verts.new(seated[a])))
                except ValueError:
                    continue  # a duplicate face; the edge was already walled
                wall.material_index = slot
                # Corners 0,3 sit under a; corners 1,2 under b.
                for corner, source_vert in zip(wall.loops, (a, b, b, a), strict=True):
                    if source_vert in vert_uv:
                        corner[uv_layer].uv = vert_uv[source_vert]
                skirt += 1
            print(f"ASSEMBLE shell rim: {skirt} edges walled on {obj.name}")

        bm.to_mesh(mesh)
        bm.free()
        mesh.update()
        total += len(new_faces)
        print(f"ASSEMBLE shell: {len(new_faces)} faces raised on {obj.name}")

    return total


#: Where a named protrusion goes, as predicates over a face's position on the body. Each value is
#: (min, max) fractions: `across` is distance from the midline as a fraction of the half-span,
#: `up` is height as a fraction of the body, `depth` is front(0) to back(1).
#:
#: Expressed as fractions of the base's own measured extents rather than in model units, so the
#: same word puts spikes in the same anatomical place on a differently-sized base.
#: Heights are taken from the base's measured island heights rather than from intuition. On this
#: character the head is enormous and starts barely above halfway, so a guessed "shoulders are at
#: 0.6 of height" put spikes on the cheeks. Measured: arms 0.40, torso 0.32, ears 0.58, head 0.69,
#: crown 0.92, legs 0.12, feet 0.02.
#: `bias` is (outward, up, backward) and `blend` how far to swing the spike from the surface
#: normal towards it. Following the raw normal alone points shoulder spikes straight out of the
#: monkey's chest, because that is the way the shoulder's front face happens to look. A drawn
#: spiked pauldron rakes up and outwards, so the direction has to be part of what the region
#: means -- not just the position.
SPIKE_REGIONS: dict[str, dict] = {
    # Outward-dominant, not upward: this character's head is enormous and overhangs its own
    # shoulders, so spikes raked up simply disappear behind it.
    "shoulders": {"across": (0.16, 0.42), "up": (0.36, 0.47), "depth": (0.0, 1.0),
                  "bias": (1.0, 0.42, 0.0), "blend": 0.8},
    "arms": {"across": (0.30, 0.95), "up": (0.34, 0.49), "depth": (0.0, 1.0),
             "bias": (0.7, 0.8, 0.0), "blend": 0.6},
    "forearms": {"across": (0.58, 1.0), "up": (0.34, 0.49), "depth": (0.0, 1.0),
                 "bias": (1.0, 0.45, 0.0), "blend": 0.6},
    "back": {"across": (0.0, 0.22), "up": (0.26, 0.46), "depth": (0.60, 1.0),
             "bias": (0.0, 0.45, 1.0), "blend": 0.65},
    "head": {"across": (0.0, 0.32), "up": (0.74, 1.0), "depth": (0.0, 1.0),
             "bias": (0.25, 1.0, 0.0), "blend": 0.7},
    "tail": {"across": (0.0, 0.40), "up": (0.10, 0.30), "depth": (0.68, 1.0),
             "bias": (0.0, 0.2, 1.0), "blend": 0.5},
}

#: Words a drawing might use for each region.
SPIKE_SYNONYMS: dict[str, str] = {
    "shoulder": "shoulders", "shoulders": "shoulders", "pauldron": "shoulders",
    "pauldrons": "shoulders", "arm": "arms", "arms": "arms",
    "forearm": "forearms", "forearms": "forearms", "wrist": "forearms",
    "back": "back", "spine": "back", "head": "head", "horn": "head", "horns": "head",
    "crown": "head", "tail": "tail",
}


def farthest_points(candidates: list, count: int) -> list:
    """Pick `count` well-separated candidates, by their world position (item[0]).

    Taking the first N faces would bunch every spike into one corner, because face order follows
    the mesh's own indexing. Greedy farthest-point sampling spreads them over the region, which is
    what makes a row of spikes read as a row.
    """
    if count >= len(candidates):
        return candidates
    chosen = [max(range(len(candidates)), key=lambda i: candidates[i][0].z)]
    while len(chosen) < count:
        best = max(
            (i for i in range(len(candidates)) if i not in chosen),
            key=lambda i: min((candidates[i][0] - candidates[j][0]).length for j in chosen),
        )
        chosen.append(best)
    return [candidates[i] for i in chosen]


def add_spikes(region_name: str, count: int, length_fraction: float, radius_fraction: float,
               sides: int) -> int:
    """Grow cone spikes out of the body surface in a named region.

    Deforming the base's own faces was the other option and is the wrong one here: the mesh is a
    few hundred faces, so a shoulder is two or three triangles and pulling their vertices gives a
    lump whose shape nobody chose. Building a cone on the surface, oriented along the face's own
    normal and scaled from the body's measured size, puts the shape under our control while still
    being anchored to the real geometry -- the same principle as the accessory sockets.
    """
    import bmesh

    rule = SPIKE_REGIONS[region_name]
    meshes = body_meshes()
    points = world_points(meshes)
    lo, hi = bounds(points)
    height = hi.z - lo.z
    centre_x = (lo.x + hi.x) / 2
    half_span = max((hi.x - lo.x) / 2, 1e-6)
    depth_span = max(hi.y - lo.y, 1e-6)

    target = max(meshes, key=lambda o: len(o.data.polygons))
    bm = bmesh.new()
    bm.from_mesh(target.data)
    bm.faces.ensure_lookup_table()

    # Record each anchor's local origin and normal now. Adding geometry invalidates bmesh's index
    # tables, so anything read back from `bm.faces[i]` mid-build raises.
    candidates: list[tuple[Vector, Vector, Vector]] = []
    for face in bm.faces:
        local = face.calc_center_median()
        centre = target.matrix_world @ local
        across = abs(centre.x - centre_x) / half_span
        up = (centre.z - lo.z) / max(height, 1e-6)
        depth = (centre.y - lo.y) / depth_span
        if (rule["across"][0] <= across <= rule["across"][1]
                and rule["up"][0] <= up <= rule["up"][1]
                and rule["depth"][0] <= depth <= rule["depth"][1]):
            outward = 1.0 if centre.x >= centre_x else -1.0
            candidates.append((centre, local, face.normal.copy(), outward))

    if not candidates:
        print(f"ASSEMBLE no faces matched region {region_name!r}")
        bm.free()
        return 0

    picked = farthest_points(candidates, count)
    spike_length = height * length_fraction
    spike_radius = height * radius_fraction
    print(f"ASSEMBLE {region_name}: {len(candidates)} candidate faces -> {len(picked)} spikes "
          f"(length {spike_length:.2f}, radius {spike_radius:.2f})")

    out_w, up_w, back_w = rule.get("bias", (0.0, 0.0, 0.0))
    blend = rule.get("blend", 0.0)

    built = 0
    for _, origin, face_normal, outward in picked:
        normal = face_normal.copy()
        if normal.length < 1e-6:
            continue
        normal.normalize()
        if blend > 0.0:
            bias = Vector((out_w * outward, back_w, up_w))
            if bias.length > 1e-6:
                normal = (normal * (1.0 - blend) + bias.normalized() * blend).normalized()

        # A frame on the surface to lay the cone's base ring in.
        helper = Vector((0, 0, 1)) if abs(normal.z) < 0.9 else Vector((1, 0, 0))
        tangent = normal.cross(helper).normalized()
        bitangent = normal.cross(tangent).normalized()

        ring = []
        for step in range(sides):
            angle = 2 * math.pi * step / sides
            offset = (tangent * math.cos(angle) + bitangent * math.sin(angle)) * spike_radius
            # Sink the base slightly so the spike grows out of the body with no gap at its foot.
            ring.append(bm.verts.new(origin + offset - normal * spike_radius * 0.35))
        apex = bm.verts.new(origin + normal * spike_length)
        for step in range(sides):
            bm.faces.new((ring[step], ring[(step + 1) % sides], apex))
        built += 1

    bm.normal_update()
    bm.to_mesh(target.data)
    bm.free()
    target.data.update()
    return built


def pack_texture(output: Path, margin: int = 4) -> bool:
    """Crop the body atlas to the part this character actually samples, and remap its UVs.

    The source is a 2048x2048 sheet holding every monkey in the game; this character uses roughly
    one sixteenth of it. Exporting the whole sheet embeds ~7 MB of other characters' artwork in
    every GLB.

    This is lossless in appearance: the surviving pixels are the same pixels, at the same
    resolution. Only the empty surround is dropped, and the UVs are rescaled to match so nothing
    moves. The bounds come from the mesh's own UVs rather than from an assumed atlas cell --
    measuring showed this character's faces spill about seven pixels past its nominal cell, and a
    tidy assumption would have sliced its edge off.
    """
    meshes = [o for o in body_meshes() if o.data.uv_layers.active]
    if not meshes:
        print("ASSEMBLE no UV'd body meshes; not packing")
        return False

    # The body atlas is the large square sheet; the eyelid sheet keeps its own small image.
    # Materials are inspected via `node_tree` rather than `use_nodes`, which is deprecated in
    # Blender 5 and was quietly skipping every material here.
    def image_nodes(material):
        tree = getattr(material, "node_tree", None)
        if tree is None:
            return []
        return [node for node in tree.nodes if node.type == "TEX_IMAGE" and node.image]

    # Chosen from the body's own materials, largest first. Scanning every material in the file
    # picks up an accessory's texture instead -- TRELLIS bakes at 1024 square, which satisfies
    # any "large and square" test just as well as the body atlas does.
    body_materials = [material for obj in meshes for material in obj.data.materials if material]
    candidates = [node.image for material in body_materials for node in image_nodes(material)
                  if node.image.size[0] >= 1024 and node.image.size[0] == node.image.size[1]]
    if not candidates:
        print("ASSEMBLE no large square body atlas found; not packing")
        return False
    target_image = max(candidates, key=lambda image: image.size[0])

    width, height = target_image.size

    # Every atlas-sized sheet on the body shares one UV space: the base skin and the garment
    # texture are both derived from the same atlas, so one crop box serves both.
    atlas_images = {
        node.image for material in body_materials for node in image_nodes(material)
        if tuple(node.image.size) == (width, height)
    }

    # Membership is decided per *face*, not per object. The garment shell appends its material to
    # every body mesh's slot list, so an object-level test counts the eyelid mesh -- whose UVs
    # live in a different, unrelated 512px sheet -- and its coordinates blow the bounds out to
    # nearly the whole atlas.
    users: list[tuple[object, list[int]]] = []
    lo_u = lo_v = 1.0
    hi_u = hi_v = 0.0
    for obj in meshes:
        slots = {
            index for index, material in enumerate(obj.data.materials)
            if material and any(node.image in atlas_images for node in image_nodes(material))
        }
        if not slots:
            continue
        loops = [loop
                 for polygon in obj.data.polygons if polygon.material_index in slots
                 for loop in polygon.loop_indices]
        if not loops:
            continue
        users.append((obj, loops))
        uv_data = obj.data.uv_layers.active.data
        for loop in loops:
            u, v = uv_data[loop].uv
            lo_u, hi_u = min(lo_u, u), max(hi_u, u)
            lo_v, hi_v = min(lo_v, v), max(hi_v, v)
    if not users or hi_u <= lo_u or hi_v <= lo_v:
        print(f"ASSEMBLE not packing: {len(users)} users, "
              f"u[{lo_u:.3f},{hi_u:.3f}] v[{lo_v:.3f},{hi_v:.3f}]")
        return False

    x0 = max(int(lo_u * width) - margin, 0)
    x1 = min(int(math.ceil(hi_u * width)) + margin, width)
    y0 = max(int((1.0 - hi_v) * height) - margin, 0)
    y1 = min(int(math.ceil((1.0 - lo_v) * height)) + margin, height)
    if (x1 - x0) >= width and (y1 - y0) >= height:
        print("ASSEMBLE texture already tight; not packing")
        return False

    output.parent.mkdir(parents=True, exist_ok=True)
    replacements = {}
    for index, source in enumerate(sorted(atlas_images, key=lambda image: image.name)):
        cropped = np_pixels(source)[y0:y1, x0:x1]
        packed = bpy.data.images.new(f"Packed_{source.name}", width=x1 - x0, height=y1 - y0,
                                     alpha=True)
        packed.pixels.foreach_set(cropped[::-1].ravel())
        destination = output if index == 0 else output.with_name(f"{output.stem}_{index}.png")
        packed.filepath_raw = str(destination.resolve())
        packed.file_format = "PNG"
        packed.save()
        replacements[source] = packed

    # Rescale every UV into the crop. Both axes flip sense between image rows and UV space, so the
    # v mapping is written against the same rows the crop was taken from.
    span_u = (x1 - x0) / width
    span_v = (y1 - y0) / height
    origin_u = x0 / width
    origin_v = 1.0 - y1 / height
    for obj, loops in users:
        uv_data = obj.data.uv_layers.active.data
        for loop in loops:
            u, v = uv_data[loop].uv
            uv_data[loop].uv = ((u - origin_u) / span_u, (v - origin_v) / span_v)

    for material in bpy.data.materials:
        for node in image_nodes(material):
            if node.image in replacements:
                node.image = replacements[node.image]

    print(f"ASSEMBLE packed {len(replacements)} sheet(s) {width}x{height} -> {x1 - x0}x{y1 - y0} "
          f"({(x1 - x0) * (y1 - y0) / (width * height):.1%} of the original area)")
    return True


def np_pixels(image):
    """Image pixels as a (height, width, 4) array, top row first.

    numpy is imported inside the function because this module also has to import cleanly in a
    Blender build that has no numpy on its path.
    """
    import numpy as np

    flat = np.empty(len(image.pixels), dtype=np.float32)
    image.pixels.foreach_get(flat)
    width, height = image.size
    return flat.reshape(height, width, 4)[::-1]


def place_accessory(path: Path, socket_name: str, anchor: Vector, head_width: float,
                    size_fraction: float, sink: float, decimate_to: int = 0) -> None:
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path.resolve()))
    imported = [o for o in bpy.data.objects
                if o not in before and o.type == "MESH"]
    if not imported:
        print(f"ASSEMBLE no mesh in {path}")
        return

    for obj in imported:
        obj.name = f"ACC_{obj.name}"

    if decimate_to and decimate_to > 0:
        # TRELLIS reconstructs at ~20k triangles, which is reasonable for inspection and absurd
        # for a hat worn by a character a few dozen pixels tall in play. Collapsing is safe here:
        # a cap is a smooth dome and a brim, with no fine detail for the solver to lose.
        for obj in imported:
            before = len(obj.data.polygons)
            if before <= decimate_to:
                continue
            modifier = obj.modifiers.new("Decimate", "DECIMATE")
            modifier.decimate_type = "COLLAPSE"
            modifier.ratio = decimate_to / before
            bpy.context.view_layer.objects.active = obj
            bpy.ops.object.modifier_apply(modifier="Decimate")
            print(f"ASSEMBLE decimated {obj.name}: {before} -> {len(obj.data.polygons)} faces")

    points = world_points(imported)
    lo, hi = bounds(points)
    extent = hi - lo
    largest = max(extent.x, extent.y, extent.z)
    if largest <= 1e-6:
        print(f"ASSEMBLE degenerate accessory {path}")
        return

    scale = (head_width * size_fraction) / max(extent.x, extent.y)
    for obj in imported:
        obj.matrix_world = Matrix.Scale(scale, 4) @ obj.matrix_world
    bpy.context.view_layer.update()

    points = world_points(imported)
    lo, hi = bounds(points)
    centre = (lo + hi) * 0.5
    # Anchor the accessory's base on the socket, then sink it so it sits *around* the anchor.
    bottom = Vector((centre.x, centre.y, lo.z + (hi.z - lo.z) * sink))
    offset = anchor - bottom
    for obj in imported:
        obj.matrix_world = Matrix.Translation(offset) @ obj.matrix_world
        obj["monkeyforge_socket"] = socket_name

    print(f"ASSEMBLE placed {path.name} on {socket_name} scale={scale:.4f}")


def render(output: Path, view: str, resolution: int) -> None:
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    corners = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
    lo = Vector((min(c.x for c in corners), min(c.y for c in corners), min(c.z for c in corners)))
    hi = Vector((max(c.x for c in corners), max(c.y for c in corners), max(c.z for c in corners)))
    centre = (lo + hi) * 0.5
    radius = max((hi - lo).length * 0.5, 0.001)

    angle = math.radians(VIEWS[view])
    distance = radius * 2.8
    camera_data = bpy.data.cameras.new("Camera")
    camera_data.lens = 50
    camera = bpy.data.objects.new("Camera", camera_data)
    camera.location = Vector((
        centre.x + distance * math.sin(angle),
        centre.y - distance * math.cos(angle),
        centre.z + radius * 0.20,
    ))
    camera.rotation_euler = (centre - camera.location).to_track_quat("-Z", "Y").to_euler()
    bpy.context.scene.collection.objects.link(camera)
    bpy.context.scene.camera = camera

    world = bpy.data.worlds.new("World")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.42, 0.44, 0.52, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.5
    bpy.context.scene.world = world

    # A key light. Purely ambient lighting shades every surface the same regardless of which way
    # it faces, which makes relief invisible: a bump map perturbs normals, and nothing looks at
    # the normals when the light comes equally from everywhere. Angling a sun down from the
    # camera's left is what lets folds, seams and a raised tie actually read.
    sun_data = bpy.data.lights.new("Key", type="SUN")
    sun_data.energy = 3.0
    sun_data.angle = math.radians(12)
    sun = bpy.data.objects.new("Key", sun_data)
    sun.rotation_euler = (math.radians(58), 0.0, math.radians(angle_degrees := VIEWS[view] - 35))
    bpy.context.scene.collection.objects.link(sun)
    print(f"ASSEMBLE key light at {angle_degrees:.0f} deg")

    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = resolution
    scene.render.resolution_y = resolution
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = str(output.resolve())
    output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.render.render(write_still=True)
    print(f"ASSEMBLE rendered {output}")


def main() -> None:
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.wm.obj_import(filepath=str(args.base.resolve()))

    if args.drop_prop:
        for obj in list(bpy.context.scene.objects):
            name = obj.name.lower()
            if obj.type == "MESH" and "dart" in name and "flatskin" not in name:
                bpy.data.objects.remove(obj, do_unlink=True)

    base = body_meshes()
    points = world_points(base)
    lo, hi = bounds(points)
    height = hi.z - lo.z
    centre_x = (lo.x + hi.x) / 2

    if args.base_texture:
        # The body's own artwork, restyled. Applied before the shell so the garment layer sits on
        # top of the new skin rather than the original.
        print(f"ASSEMBLE base skin: swapped {swap_texture(args.base_texture)} node(s)")

    if args.shell and args.texture:
        # The garment becomes geometry standing off an untouched base, rather than paint replacing
        # the base's own artwork.
        raised = build_garment_shell(args.shell, args.texture, height * args.shell_offset,
                                     args.height_map, args.height_strength,
                                     height * args.height_distance,
                                     rim=not args.no_shell_rim)
        print(f"ASSEMBLE raised {raised} garment faces by {height * args.shell_offset:.3f}")
    elif args.texture:
        print(f"ASSEMBLE swapped {swap_texture(args.texture)} texture node(s)")

    head = [p for p in points if p.z >= lo.z + height * 0.60]
    head_lo, head_hi = bounds(head) if head else (lo, hi)
    head_width = max(head_hi.x - head_lo.x, 1e-3)

    for region_name in args.spike:
        # After the garment shell, so spikes push through the clothing rather than being buried
        # under it -- which is how a drawn spiked pauldron actually reads.
        grown = add_spikes(region_name, args.spike_count, args.spike_length,
                           args.spike_radius, args.spike_sides)
        print(f"ASSEMBLE grew {grown} spikes on {region_name}")

    for index, accessory in enumerate(args.accessory):
        socket_name = args.socket[index] if index < len(args.socket) else "SOCKET_HEAD_TOP"
        fraction = SOCKET_HEIGHTS.get(socket_name, 0.5)
        anchor = Vector((centre_x, (head_lo.y + head_hi.y) / 2, lo.z + height * fraction))
        place_accessory(accessory, socket_name, anchor, head_width,
                        args.accessory_size, args.sink, args.accessory_faces)

    if args.pack_texture:
        # After the shell and accessories, so every face that will be exported is counted.
        pack_texture(args.pack_texture)

    if args.normalize_height > 0:
        # Export at a known height so the game does not need a per-asset scale constant. The
        # runtime scales by its own CUBE_SIZE; a character that arrives one unit tall drops in
        # whatever the base mesh happened to measure.
        meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
        span = bounds(world_points(meshes))
        current = span[1].z - span[0].z
        if current > 1e-6:
            factor = args.normalize_height / current
            for obj in meshes:
                obj.matrix_world = Matrix.Scale(factor, 4) @ obj.matrix_world
            bpy.context.view_layer.update()
            # Sit the character's feet on the origin: the runtime positions players by their
            # centre, and a model whose pivot floats makes every spawn look sunk or hovering.
            span = bounds(world_points(meshes))
            drop = Matrix.Translation(Vector((0.0, 0.0, -span[0].z)))
            for obj in meshes:
                obj.matrix_world = drop @ obj.matrix_world
            bpy.context.view_layer.update()
            print(f"ASSEMBLE normalised height {current:.2f} -> {args.normalize_height} "
                  f"(x{factor:.4f}), feet at z=0")

    if args.glb:
        args.glb.parent.mkdir(parents=True, exist_ok=True)
        bpy.ops.export_scene.gltf(filepath=str(args.glb), export_format="GLB",
                                  export_apply=True, export_image_format="AUTO")
        print(f"ASSEMBLE wrote {args.glb}")

    if args.render:
        render(args.render, args.view, args.resolution)


if __name__ == "__main__":
    main()
