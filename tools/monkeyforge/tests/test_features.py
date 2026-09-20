import pytest

from monkeyforge.features import (
    disconnected_penalty,
    geometric_features,
    image_features,
    socket_fit_score,
    triangle_budget_score,
)
from monkeyforge.measurements import AccessoryMeasurement, GlbMeasurement
from monkeyforge.models import AccessorySpec, Palette, Socket


def _imaging_available() -> bool:
    from importlib.util import find_spec

    return find_spec("numpy") is not None and find_spec("PIL") is not None


needs_imaging = pytest.mark.skipif(
    not _imaging_available(), reason="needs the imaging extra: pip install -e .[imaging]"
)


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


def accessory_measurement(**overrides) -> AccessoryMeasurement:
    defaults = {
        "socket": "SOCKET_HEAD_TOP",
        "mesh_names": ["helmet"],
        "triangles": 900,
        "loose_parts": 1,
        "bbox_min": [-0.5, -0.5, 0.0],
        "bbox_max": [0.5, 0.5, 1.0],
        "extent": [1.0, 1.0, 1.0],
        "socket_location": [0.0, 0.0, 0.0],
        "socket_offset": 0.0,
        "socket_gap": 0.0,
        "socket_contained": True,
    }
    return AccessoryMeasurement(**{**defaults, **overrides})


def test_triangle_budget_treats_the_target_as_a_ceiling() -> None:
    assert triangle_budget_score(2500, 2500) == 1.0
    assert triangle_budget_score(900, 2500) == 1.0
    # An efficient low-poly prop is not penalised for spending less than its ceiling.
    assert triangle_budget_score(96, 2500) == 1.0


def test_triangle_budget_punishes_overruns_proportionally() -> None:
    assert triangle_budget_score(5000, 2500) == 0.5
    assert triangle_budget_score(10000, 2500) == 0.25


def test_triangle_budget_flags_degenerate_meshes_by_absolute_floor() -> None:
    # A near-empty mesh is broken regardless of how generous the ceiling is.
    assert triangle_budget_score(3, 2500) < 0.1
    assert triangle_budget_score(25, 2500) == 0.5
    assert triangle_budget_score(0, 2500) == 0.0


def test_disconnected_penalty_allows_the_declared_part_count() -> None:
    assert disconnected_penalty(loose_parts=1, expected_parts=1) == 0.0
    assert disconnected_penalty(loose_parts=3, expected_parts=3) == 0.0
    assert disconnected_penalty(loose_parts=5, expected_parts=1) == 1.0
    assert 0.0 < disconnected_penalty(loose_parts=2, expected_parts=1) < 1.0


def test_socket_fit_scores_seated_accessories_regardless_of_direction() -> None:
    seated = socket_fit_score(accessory_measurement(socket_gap=0.0, socket_contained=True))
    drifting = socket_fit_score(accessory_measurement(socket_gap=0.9, socket_contained=False))
    floating = socket_fit_score(accessory_measurement(socket_gap=5.0, socket_contained=False))

    assert seated == 1.0
    assert 0.0 < drifting < seated
    assert floating == 0.0


def test_socket_fit_does_not_penalise_a_hat_for_extending_upward() -> None:
    # A hat sits on the head socket and extends up, so its centre is far from the socket even
    # though it is correctly seated. Only the gap should matter.
    hat = accessory_measurement(
        bbox_min=[-0.325, -0.212, 1.717],
        bbox_max=[0.325, 0.212, 2.335],
        extent=[0.65, 0.424, 0.618],
        socket_location=[0.0, 0.0, 1.717],
        socket_offset=0.309,
        socket_gap=0.0,
        socket_contained=True,
    )
    assert socket_fit_score(hat) == 1.0


def test_detached_accessory_scores_zero_across_geometry() -> None:
    measurement = GlbMeasurement(
        source="character.glb",
        total_triangles=5000,
        mesh_count=4,
        material_count=2,
        loose_parts=4,
        accessory=None,
    )
    features = geometric_features(measurement, accessory_spec())

    assert features["socket_fit_score"] == 0.0
    assert features["triangle_budget_score"] == 0.0
    assert features["disconnected_penalty"] == 1.0


def _write_render(path, background, subject_colour, box, transparent=False):
    """A flat background with a solid rectangle subject, opaque unless asked otherwise."""
    from PIL import Image

    alpha = 0 if transparent else 255
    image = Image.new("RGBA", (128, 128), (*background, alpha))
    for x in range(box[0], box[2]):
        for y in range(box[1], box[3]):
            image.putpixel((x, y), (*subject_colour, 255))
    image.save(path)
    return path


@needs_imaging
def test_image_features_separate_subject_from_an_opaque_background(tmp_path) -> None:
    # The preview renderer writes film_transparent=False, so the subject must be found by colour.
    render = _write_render(
        tmp_path / "opaque.png",
        background=(20, 24, 40),
        subject_colour=(146, 87, 47),
        box=(44, 44, 84, 84),
    )
    features = image_features(render, Palette())

    assert 0.0 < features["silhouette_score"] <= 1.0
    assert 0.0 < features["palette_score"] <= 1.0


@needs_imaging
def test_image_features_use_alpha_when_the_render_has_it(tmp_path) -> None:
    render = _write_render(
        tmp_path / "alpha.png",
        background=(0, 0, 0),
        subject_colour=(146, 87, 47),
        box=(44, 44, 84, 84),
        transparent=True,
    )
    features = image_features(render, Palette())

    assert 0.0 < features["silhouette_score"] <= 1.0
    # The subject is exactly the default fur colour, so palette adherence should be near perfect.
    assert features["palette_score"] > 0.95


@needs_imaging
def test_image_features_return_zero_when_subject_fills_the_frame(tmp_path) -> None:
    from PIL import Image

    Image.new("RGBA", (64, 64), (146, 87, 47, 255)).save(tmp_path / "flat.png")
    features = image_features(tmp_path / "flat.png", Palette())

    assert features == {"silhouette_score": 0.0, "palette_score": 0.0}


def test_geometric_features_are_bounded_for_a_clean_asset() -> None:
    measurement = GlbMeasurement(
        source="character.glb",
        total_triangles=5000,
        mesh_count=4,
        material_count=2,
        loose_parts=4,
        accessory=accessory_measurement(),
    )
    features = geometric_features(measurement, accessory_spec())

    assert features["socket_fit_score"] == 1.0
    assert features["triangle_budget_score"] == 1.0
    assert features["disconnected_penalty"] == 0.0
