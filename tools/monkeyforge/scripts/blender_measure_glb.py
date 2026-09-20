"""Measure a MonkeyForge GLB and emit structured JSON.

This supersedes the print-scraping in ``blender_validate_glb.py``. It performs the same socket and
mesh checks but writes a machine-readable measurement document that both hard validation and style
ranker feature extraction consume, so a candidate is measured exactly once.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import bmesh
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

SCHEMA_VERSION = "1.0"


def is_import_helper(obj: bpy.types.Object) -> bool:
    return any(collection.name == "glTF_not_exported" for collection in obj.users_collection)


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--accessory-socket",
        default=None,
        help="Socket node the accessory was attached to, e.g. SOCKET_HEAD_TOP.",
    )
    parser.add_argument(
        "--bare",
        action="store_true",
        help=(
            "Measure a raw accessory that has not been compiled onto a base: no socket nodes are "
            "required, every mesh counts as the accessory, and the world origin stands in for the "
            "socket. Use this to judge what a generator produced, before the compiler normalises "
            "its position and triangle count."
        ),
    )
    return parser.parse_args(arguments)


def count_loose_parts(mesh: bpy.types.Object, weld_distance: float = 1e-4) -> int:
    """Count connected components by walking linked faces.

    glTF stores attributes per-vertex, so it splits vertices at every hard edge and UV seam.
    Walking raw imported topology therefore reports a flat-shaded cube as six separate parts. Weld
    coincident vertices first so this measures real disconnection, not shading discontinuity.
    """
    bm = bmesh.new()
    try:
        bm.from_mesh(mesh.data)
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=weld_distance)
        remaining = set(bm.faces)
        components = 0
        while remaining:
            seed = remaining.pop()
            components += 1
            stack = [seed]
            while stack:
                face = stack.pop()
                for edge in face.edges:
                    for linked in edge.link_faces:
                        if linked in remaining:
                            remaining.discard(linked)
                            stack.append(linked)
        # A mesh with only loose vertices/edges and no faces still counts as one part.
        return components if components else min(1, len(bm.verts))
    finally:
        bm.free()


def triangles_of(mesh: bpy.types.Object) -> int:
    return sum(max(1, len(polygon.vertices) - 2) for polygon in mesh.data.polygons)


def world_bounds(meshes: list[bpy.types.Object]) -> tuple[list[float], list[float]] | None:
    corners: list[Vector] = []
    for mesh in meshes:
        corners.extend(mesh.matrix_world @ Vector(corner) for corner in mesh.bound_box)
    if not corners:
        return None
    minimum = [min(c[axis] for c in corners) for axis in range(3)]
    maximum = [max(c[axis] for c in corners) for axis in range(3)]
    return minimum, maximum


def descends_from(obj: bpy.types.Object, ancestor_name: str) -> bool:
    current = obj.parent
    while current is not None:
        if current.name == ancestor_name:
            return True
        current = current.parent
    return False


def import_any(path: Path) -> None:
    """Import a GLB/glTF or a raw OBJ, so bare generator output can be measured too."""
    suffix = path.suffix.lower()
    if suffix in {".glb", ".gltf"}:
        bpy.ops.import_scene.gltf(filepath=str(path.resolve()))
    elif suffix == ".obj":
        bpy.ops.wm.obj_import(filepath=str(path.resolve()))
    else:
        raise SystemExit(f"unsupported input format: {suffix}")


def measure(path: Path, accessory_socket: str | None, bare: bool = False) -> dict:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    import_any(path)

    meshes = [
        obj
        for obj in bpy.context.scene.objects
        if obj.type == "MESH" and not is_import_helper(obj)
    ]
    present = set(bpy.data.objects.keys())
    missing = [] if bare else sorted(REQUIRED_SOCKETS - present)

    mesh_records = []
    for mesh in meshes:
        bounds = world_bounds([mesh])
        mesh_records.append(
            {
                "name": mesh.name,
                "parent": getattr(mesh.parent, "name", None),
                "triangles": triangles_of(mesh),
                "loose_parts": count_loose_parts(mesh),
                "materials": [material.name for material in mesh.data.materials if material],
                "bbox_min": bounds[0] if bounds else None,
                "bbox_max": bounds[1] if bounds else None,
            }
        )

    sockets = {
        name: list(bpy.data.objects[name].matrix_world.translation)
        for name in sorted(REQUIRED_SOCKETS & present)
    }

    document = {
        "schema_version": SCHEMA_VERSION,
        "source": str(path),
        "total_triangles": sum(record["triangles"] for record in mesh_records),
        "mesh_count": len(mesh_records),
        "material_count": len({name for record in mesh_records for name in record["materials"]}),
        "loose_parts": sum(record["loose_parts"] for record in mesh_records),
        "missing_sockets": missing,
        "sockets": sockets,
        "meshes": mesh_records,
        "accessory": None,
    }

    # In bare mode every mesh is the accessory and the world origin stands in for the socket: a
    # generator that emits geometry far from its own origin has produced a worse asset, even though
    # the compiler would later seat it correctly.
    if bare:
        attached = meshes
        socket_name = "ORIGIN"
        socket_point = Vector((0.0, 0.0, 0.0))
    elif accessory_socket and accessory_socket in present:
        attached = [mesh for mesh in meshes if descends_from(mesh, accessory_socket)]
        socket_name = accessory_socket
        socket_point = Vector(sockets[accessory_socket]) if attached else Vector((0.0, 0.0, 0.0))
    else:
        attached = []
        socket_name = accessory_socket or "ORIGIN"
        socket_point = Vector((0.0, 0.0, 0.0))

    if attached:
        bounds = world_bounds(attached)
        socket_location = socket_point
        center = Vector([(bounds[0][axis] + bounds[1][axis]) / 2 for axis in range(3)])
        extent = [bounds[1][axis] - bounds[0][axis] for axis in range(3)]

        # Distance from the socket to the accessory's bounding box, zero when the socket sits
        # inside it. Unlike distance-to-centre this does not penalise correctly seated
        # directional props: a hat legitimately extends upward from its socket.
        gap = Vector(
            [
                max(
                    bounds[0][axis] - socket_location[axis],
                    0.0,
                    socket_location[axis] - bounds[1][axis],
                )
                for axis in range(3)
            ]
        ).length

        document["accessory"] = {
            "socket": socket_name,
            "mesh_names": [mesh.name for mesh in attached],
            "triangles": sum(triangles_of(mesh) for mesh in attached),
            "loose_parts": sum(count_loose_parts(mesh) for mesh in attached),
            "bbox_min": bounds[0],
            "bbox_max": bounds[1],
            "extent": extent,
            "socket_location": list(socket_location),
            "socket_offset": (center - socket_location).length,
            "socket_gap": gap,
            "socket_contained": gap <= 1e-6,
        }

    return document


def main() -> None:
    args = parse_args()
    document = measure(args.input, args.accessory_socket, bare=args.bare)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=2), encoding="utf-8")
    print(f"MONKEYFORGE_MEASUREMENT wrote={args.output}")
    if not document["meshes"] or document["missing_sockets"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
