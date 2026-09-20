from pathlib import Path

from monkeyforge.measurements import GlbMeasurement
from monkeyforge.models import AccessorySpec, CompiledAsset, ValidationMetrics

MAX_ASSET_BYTES = 100 * 1024 * 1024

# An accessory may exceed its triangle target by this factor before the asset is rejected outright.
TRIANGLE_OVERRUN_LIMIT = 2.0


def validate_compiled_asset(
    compiled: CompiledAsset,
    accessory: AccessorySpec,
    measurement: GlbMeasurement | None = None,
) -> ValidationMetrics:
    """Validate a compiled asset, using real geometry when a measurement pass has run.

    Without a measurement this can only check that a plausible file exists; the returned metrics say
    so via `measured=False` rather than implying the topology was inspected.
    """
    warnings: list[str] = []
    path = Path(compiled.path)
    if not path.exists():
        return ValidationMetrics(
            valid=False,
            file_size_bytes=0,
            target_triangles=accessory.target_triangles,
            warnings=["compiled asset does not exist"],
        )

    size = path.stat().st_size
    if size == 0:
        warnings.append("compiled asset is empty")
    if size > MAX_ASSET_BYTES:
        warnings.append("compiled asset exceeds 100 MiB")
    if compiled.compiler == "passthrough":
        warnings.append("development passthrough used; topology and socket fit were not validated")

    valid = size > 0

    if measurement is None:
        if compiled.compiler != "passthrough":
            warnings.append("no measurement pass ran; only file size was checked")
        return ValidationMetrics(
            valid=valid,
            file_size_bytes=size,
            target_triangles=accessory.target_triangles,
            warnings=warnings,
        )

    if measurement.missing_sockets:
        warnings.append(f"missing sockets: {', '.join(measurement.missing_sockets)}")
        valid = False
    if measurement.mesh_count == 0:
        warnings.append("compiled asset contains no meshes")
        valid = False

    attached = measurement.accessory
    if attached is None:
        warnings.append(f"no mesh attached to {accessory.slot.value} socket")
        valid = False
    else:
        limit = int(accessory.target_triangles * TRIANGLE_OVERRUN_LIMIT)
        if attached.triangles > limit:
            warnings.append(
                f"accessory uses {attached.triangles} triangles "
                f"against a {accessory.target_triangles} target"
            )
            valid = False
        expected_parts = max(1, len(accessory.part_schema))
        if attached.loose_parts > expected_parts:
            warnings.append(
                f"accessory has {attached.loose_parts} disconnected parts, "
                f"expected at most {expected_parts}"
            )
        if not attached.socket_contained:
            warnings.append("accessory bounding box does not contain its socket")

    return ValidationMetrics(
        valid=valid,
        file_size_bytes=size,
        target_triangles=accessory.target_triangles,
        warnings=warnings,
        measured=True,
        accessory_triangles=attached.triangles if attached else None,
        total_triangles=measurement.total_triangles,
        mesh_count=measurement.mesh_count,
        loose_parts=measurement.loose_parts,
        missing_sockets=measurement.missing_sockets,
        socket_offset=attached.socket_offset if attached else None,
    )
