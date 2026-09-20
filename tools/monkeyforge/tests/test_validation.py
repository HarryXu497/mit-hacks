from pathlib import Path

from monkeyforge.measurements import AccessoryMeasurement, GlbMeasurement
from monkeyforge.models import AccessorySpec, CompiledAsset, Socket
from monkeyforge.validation import validate_compiled_asset


def accessory_spec(**overrides) -> AccessorySpec:
    defaults = {
        "id": "helmet",
        "slot": Socket.HEAD_TOP,
        "kind": "helmet",
        "description": "a rounded goalkeeper helmet",
        "target_triangles": 1000,
        "part_schema": ["shell"],
    }
    return AccessorySpec(**{**defaults, **overrides})


def compiled_asset(tmp_path: Path, compiler: str = "blender") -> CompiledAsset:
    path = tmp_path / "character.glb"
    path.write_bytes(b"glb-bytes")
    return CompiledAsset(path=path, compiler=compiler, media_type="model/gltf-binary")


def measurement(**overrides) -> GlbMeasurement:
    attached = overrides.pop("accessory", "default")
    defaults = {
        "source": "character.glb",
        "total_triangles": 5000,
        "mesh_count": 4,
        "material_count": 2,
        "loose_parts": 4,
        "missing_sockets": [],
    }
    if attached == "default":
        attached = AccessoryMeasurement(
            socket="SOCKET_HEAD_TOP",
            mesh_names=["helmet"],
            triangles=900,
            loose_parts=1,
            bbox_min=[-0.5, -0.5, 0.0],
            bbox_max=[0.5, 0.5, 1.0],
            extent=[1.0, 1.0, 1.0],
            socket_location=[0.0, 0.0, 0.0],
            socket_offset=0.0,
            socket_gap=0.0,
            socket_contained=True,
        )
    return GlbMeasurement(**{**defaults, **overrides}, accessory=attached)


def test_unmeasured_asset_reports_that_it_was_not_inspected(tmp_path: Path) -> None:
    metrics = validate_compiled_asset(compiled_asset(tmp_path), accessory_spec(), None)

    assert metrics.measured is False
    assert any("no measurement pass ran" in warning for warning in metrics.warnings)


def test_clean_measured_asset_is_valid(tmp_path: Path) -> None:
    metrics = validate_compiled_asset(compiled_asset(tmp_path), accessory_spec(), measurement())

    assert metrics.valid is True
    assert metrics.measured is True
    assert metrics.accessory_triangles == 900
    assert metrics.warnings == []


def test_missing_sockets_fail_validation(tmp_path: Path) -> None:
    metrics = validate_compiled_asset(
        compiled_asset(tmp_path),
        accessory_spec(),
        measurement(missing_sockets=["SOCKET_TAIL_TIP"]),
    )

    assert metrics.valid is False
    assert any("SOCKET_TAIL_TIP" in warning for warning in metrics.warnings)


def test_triangle_overrun_fails_validation(tmp_path: Path) -> None:
    over = measurement()
    over.accessory.triangles = 9000
    metrics = validate_compiled_asset(compiled_asset(tmp_path), accessory_spec(), over)

    assert metrics.valid is False
    assert any("9000 triangles" in warning for warning in metrics.warnings)


def test_detached_accessory_fails_validation(tmp_path: Path) -> None:
    metrics = validate_compiled_asset(
        compiled_asset(tmp_path),
        accessory_spec(),
        measurement(accessory=None),
    )

    assert metrics.valid is False
    assert any("no mesh attached" in warning for warning in metrics.warnings)


def test_fragmented_accessory_warns_but_still_ships(tmp_path: Path) -> None:
    fragmented = measurement()
    fragmented.accessory.loose_parts = 6
    metrics = validate_compiled_asset(compiled_asset(tmp_path), accessory_spec(), fragmented)

    assert metrics.valid is True
    assert any("disconnected parts" in warning for warning in metrics.warnings)
