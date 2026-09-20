"""Minimal binary glTF writer.

Enough of the spec to emit a single-mesh GLB with positions and indices, so the stub generator has
no dependency on Blender, torch, or any asset library.
"""

from __future__ import annotations

import json
import struct

GLB_MAGIC = b"glTF"
JSON_CHUNK = 0x4E4F534A
BIN_CHUNK = 0x004E4942

COMPONENT_UNSIGNED_SHORT = 5123
COMPONENT_FLOAT = 5126
MODE_TRIANGLES = 4


def _pad(data: bytes, fill: bytes = b"\x00") -> bytes:
    remainder = len(data) % 4
    return data if remainder == 0 else data + fill * (4 - remainder)


def write_glb(
    vertices: list[tuple[float, float, float]],
    triangles: list[tuple[int, int, int]],
    name: str = "accessory",
) -> bytes:
    """Pack vertices and triangles into a single-primitive binary GLB."""
    if not vertices or not triangles:
        raise ValueError("cannot write a GLB with no geometry")
    if len(vertices) > 65535:
        raise ValueError("stub GLB writer uses uint16 indices; keep meshes under 65535 vertices")

    index_bytes = b"".join(struct.pack("<3H", *triangle) for triangle in triangles)
    index_bytes = _pad(index_bytes)
    position_bytes = b"".join(struct.pack("<3f", *vertex) for vertex in vertices)

    buffer = index_bytes + position_bytes

    minimum = [min(vertex[axis] for vertex in vertices) for axis in range(3)]
    maximum = [max(vertex[axis] for vertex in vertices) for axis in range(3)]

    gltf = {
        "asset": {"version": "2.0", "generator": "monkeyforge-worker-stub"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": name}],
        "meshes": [
            {
                "name": name,
                "primitives": [
                    {
                        "attributes": {"POSITION": 1},
                        "indices": 0,
                        "mode": MODE_TRIANGLES,
                        "material": 0,
                    }
                ],
            }
        ],
        "materials": [
            {
                "name": "Primary",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.21, 0.65, 0.85, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.75,
                },
            }
        ],
        "buffers": [{"byteLength": len(buffer)}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": len(index_bytes), "target": 34963},
            {
                "buffer": 0,
                "byteOffset": len(index_bytes),
                "byteLength": len(position_bytes),
                "target": 34962,
            },
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": COMPONENT_UNSIGNED_SHORT,
                "count": len(triangles) * 3,
                "type": "SCALAR",
            },
            {
                "bufferView": 1,
                "componentType": COMPONENT_FLOAT,
                "count": len(vertices),
                "type": "VEC3",
                "min": minimum,
                "max": maximum,
            },
        ],
    }

    json_bytes = _pad(json.dumps(gltf, separators=(",", ":")).encode("utf-8"), b" ")
    binary_bytes = _pad(buffer)

    total = 12 + 8 + len(json_bytes) + 8 + len(binary_bytes)
    out = bytearray()
    out += GLB_MAGIC
    out += struct.pack("<II", 2, total)
    out += struct.pack("<II", len(json_bytes), JSON_CHUNK)
    out += json_bytes
    out += struct.pack("<II", len(binary_bytes), BIN_CHUNK)
    out += binary_bytes
    return bytes(out)
