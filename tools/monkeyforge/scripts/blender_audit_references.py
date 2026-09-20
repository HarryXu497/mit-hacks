"""Audit local prototype references without modifying or redistributing them."""

from __future__ import annotations

import argparse
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import bpy
from mathutils import Vector


def parse_args() -> argparse.Namespace:
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args(arguments)


def clear_scene() -> None:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)


def import_model(path: Path) -> None:
    if path.suffix.lower() != ".obj":
        raise ValueError(f"unsupported reference format: {path.suffix}")
    bpy.ops.wm.obj_import(filepath=str(path), forward_axis="NEGATIVE_Z", up_axis="Y")


def mesh_bounds(meshes: list[bpy.types.Object]) -> dict[str, list[float]] | None:
    corners = [obj.matrix_world @ Vector(corner) for obj in meshes for corner in obj.bound_box]
    if not corners:
        return None
    minimum = [min(point[axis] for point in corners) for axis in range(3)]
    maximum = [max(point[axis] for point in corners) for axis in range(3)]
    return {
        "minimum": [round(value, 5) for value in minimum],
        "maximum": [round(value, 5) for value in maximum],
        "extent": [round(maximum[i] - minimum[i], 5) for i in range(3)],
    }


def audit_obj(path: Path, root: Path) -> dict[str, object]:
    clear_scene()
    import_model(path)
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    materials = sorted(
        {material.name for obj in meshes for material in obj.data.materials if material is not None}
    )
    return {
        "source": path.relative_to(root).as_posix(),
        "format": path.suffix.lower().lstrip("."),
        "mesh_count": len(meshes),
        "mesh_objects": [
            {
                "name": obj.name,
                "vertices": len(obj.data.vertices),
                "polygons": len(obj.data.polygons),
                "triangles": sum(max(1, len(face.vertices) - 2) for face in obj.data.polygons),
                "parent": obj.parent.name if obj.parent else None,
            }
            for obj in meshes
        ],
        "armatures": [
            {"name": obj.name, "bones": [bone.name for bone in obj.data.bones]}
            for obj in armatures
        ],
        "materials": materials,
        "bounds": mesh_bounds(meshes),
    }


def audit_dae(path: Path, root: Path) -> dict[str, object]:
    document = ET.parse(path)
    xml_root = document.getroot()
    namespace = xml_root.tag.split("}")[0].lstrip("{")
    ns = {"c": namespace}
    mesh_objects = []
    all_positions: list[tuple[float, float, float]] = []

    for geometry in xml_root.findall(".//c:library_geometries/c:geometry", ns):
        mesh = geometry.find("c:mesh", ns)
        if mesh is None:
            continue
        position_source_id = None
        vertices = mesh.find("c:vertices", ns)
        if vertices is not None:
            for item in vertices.findall("c:input", ns):
                if item.get("semantic") == "POSITION":
                    position_source_id = (item.get("source") or "").lstrip("#")
                    break

        positions: list[tuple[float, float, float]] = []
        for source in mesh.findall("c:source", ns):
            if source.get("id") != position_source_id:
                continue
            float_array = source.find("c:float_array", ns)
            accessor = source.find("c:technique_common/c:accessor", ns)
            if float_array is None:
                continue
            values = [float(value) for value in (float_array.text or "").split()]
            stride = int(accessor.get("stride", "3")) if accessor is not None else 3
            positions = [tuple(values[i : i + 3]) for i in range(0, len(values), stride)]
            all_positions.extend(positions)

        polygon_count = 0
        triangle_count = 0
        for triangles in mesh.findall("c:triangles", ns):
            count = int(triangles.get("count", "0"))
            polygon_count += count
            triangle_count += count
        for polylist in mesh.findall("c:polylist", ns):
            counts_node = polylist.find("c:vcount", ns)
            counts = [int(value) for value in (counts_node.text or "").split()]
            polygon_count += len(counts)
            triangle_count += sum(max(1, count - 2) for count in counts)

        mesh_objects.append(
            {
                "name": geometry.get("name") or geometry.get("id") or "geometry",
                "vertices": len(positions),
                "polygons": polygon_count,
                "triangles": triangle_count,
                "parent": None,
            }
        )

    bounds = None
    if all_positions:
        minimum = [min(point[axis] for point in all_positions) for axis in range(3)]
        maximum = [max(point[axis] for point in all_positions) for axis in range(3)]
        bounds = {
            "minimum": [round(value, 5) for value in minimum],
            "maximum": [round(value, 5) for value in maximum],
            "extent": [round(maximum[i] - minimum[i], 5) for i in range(3)],
        }

    bones = sorted(
        {
            node.get("name") or node.get("sid") or node.get("id") or "joint"
            for node in xml_root.findall(".//c:node[@type='JOINT']", ns)
        }
    )
    materials = sorted(
        material.get("name") or material.get("id") or "material"
        for material in xml_root.findall(".//c:library_materials/c:material", ns)
    )
    return {
        "source": path.relative_to(root).as_posix(),
        "format": "dae",
        "mesh_count": len(mesh_objects),
        "mesh_objects": mesh_objects,
        "armatures": [{"name": "ColladaSkeleton", "bones": bones}] if bones else [],
        "materials": materials,
        "bounds": bounds,
    }


def audit_model(path: Path, root: Path) -> dict[str, object]:
    if path.suffix.lower() == ".dae":
        return audit_dae(path, root)
    return audit_obj(path, root)


def main(args: argparse.Namespace) -> None:
    root = args.root.resolve()
    model_paths = sorted([*root.rglob("*.obj"), *root.rglob("*.dae")])
    report = {
        "notice": (
            "Private topology/style audit only; source assets are excluded from distribution."
        ),
        "assets": [audit_model(path, root) for path in model_paths],
    }
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main(parse_args())
