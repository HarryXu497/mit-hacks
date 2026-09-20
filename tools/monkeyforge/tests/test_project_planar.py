"""Tests for the garment warp, which is the geometric heart of dressing a character.

The visual check ("does the monkey look right?") cannot distinguish a correct projection from one
that is subtly mirrored, transposed or half a triangle off, and those are exactly the mistakes that
cost hours earlier in this project. These pin the arithmetic down.

The module is a script rather than a package member, so it is loaded by path.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "project_planar.py"


def load_module():
    spec = importlib.util.spec_from_file_location("project_planar", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


planar = load_module()


def test_a_quad_fans_into_two_triangles():
    quad = {"uv": [(0, 0)] * 4, "pos": [(0, 0, 0)] * 4}
    assert planar.triangles(quad) == [(0, 1, 2), (0, 2, 3)]


def test_a_triangle_stays_one_triangle():
    tri = {"uv": [(0, 0)] * 3, "pos": [(0, 0, 0)] * 3}
    assert planar.triangles(tri) == [(0, 1, 2)]


def test_a_pentagon_fans_into_three():
    poly = {"uv": [(0, 0)] * 5, "pos": [(0, 0, 0)] * 5}
    assert planar.triangles(poly) == [(0, 1, 2), (0, 2, 3), (0, 3, 4)]


def test_triangulation_uses_the_shorter_of_uv_and_pos():
    """A malformed face must not index past the end of either list."""
    face = {"uv": [(0, 0)] * 5, "pos": [(0, 0, 0)] * 3}
    assert planar.triangles(face) == [(0, 1, 2)]


def make_sprite() -> np.ndarray:
    """A 4x4 sprite where every texel is a distinct, checkable colour."""
    sprite = np.zeros((4, 4, 4), dtype=np.uint8)
    for y in range(4):
        for x in range(4):
            sprite[y, x] = (x * 60, y * 60, 200, 255)
    return sprite


def test_warp_copies_the_garment_into_the_destination_triangle():
    sprite = make_sprite()
    out_rgb = np.zeros((8, 8, 3), dtype=np.uint8)
    out_hit = np.zeros((8, 8), dtype=bool)

    # Identity-ish: destination covers the top-left half of an 8x8 area, source the whole sprite.
    destination = np.array([[0.0, 0.0], [8.0, 0.0], [0.0, 8.0]])
    source = np.array([[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]])
    planar.warp(destination, source, sprite, out_rgb, out_hit)

    assert out_hit.any(), "the triangle should have painted something"
    # The lower-right corner is outside the triangle and must be untouched.
    assert not out_hit[7, 7]
    # The upper-left corner samples the sprite's origin texel.
    assert out_hit[0, 0]
    assert tuple(out_rgb[0, 0]) == (0, 0, 200)


def test_warp_respects_sprite_transparency():
    """Transparent garment texels must leave the base texture showing through."""
    sprite = make_sprite()
    sprite[..., 3] = 0  # fully transparent
    out_rgb = np.full((8, 8, 3), 77, dtype=np.uint8)
    out_hit = np.zeros((8, 8), dtype=bool)

    planar.warp(
        np.array([[0.0, 0.0], [8.0, 0.0], [0.0, 8.0]]),
        np.array([[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]]),
        sprite, out_rgb, out_hit,
    )
    assert not out_hit.any()
    assert (out_rgb == 77).all()


def test_warp_ignores_a_degenerate_triangle():
    """Collinear vertices have no area; solving for them would raise."""
    sprite = make_sprite()
    out_rgb = np.zeros((8, 8, 3), dtype=np.uint8)
    out_hit = np.zeros((8, 8), dtype=bool)

    planar.warp(
        np.array([[0.0, 0.0], [4.0, 0.0], [8.0, 0.0]]),  # a line
        np.array([[0.0, 0.0], [2.0, 0.0], [4.0, 0.0]]),
        sprite, out_rgb, out_hit,
    )
    assert not out_hit.any()


def test_warp_clips_to_the_destination_bounds():
    """A triangle hanging off the texture must not raise or wrap around."""
    sprite = make_sprite()
    out_rgb = np.zeros((8, 8, 3), dtype=np.uint8)
    out_hit = np.zeros((8, 8), dtype=bool)

    planar.warp(
        np.array([[-20.0, -20.0], [40.0, -20.0], [-20.0, 40.0]]),
        np.array([[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]]),
        sprite, out_rgb, out_hit,
    )
    assert out_hit.shape == (8, 8)


def test_warp_is_not_mirrored():
    """The failure that cost hours before: cloth arriving flipped without anyone noticing.

    Mapping the sprite's left edge to the destination's left edge must keep it on the left.
    """
    sprite = make_sprite()
    out_rgb = np.zeros((8, 8, 3), dtype=np.uint8)
    out_hit = np.zeros((8, 8), dtype=bool)

    # Two triangles covering the full 8x8 square, mapped to the full 4x4 sprite.
    for dst, src in (
        (np.array([[0.0, 0.0], [8.0, 0.0], [0.0, 8.0]]),
         np.array([[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]])),
        (np.array([[8.0, 0.0], [8.0, 8.0], [0.0, 8.0]]),
         np.array([[4.0, 0.0], [4.0, 4.0], [0.0, 4.0]])),
    ):
        planar.warp(dst, src, sprite, out_rgb, out_hit)

    # Red channel encodes the sprite's x, so it must increase left to right, not right to left.
    left = int(out_rgb[4, 0, 0])
    right = int(out_rgb[4, 7, 0])
    assert right > left, f"garment is mirrored: left={left} right={right}"


def test_sprite_alpha_cuts_out_a_flat_background(tmp_path):
    """A generated sprite arrives opaque on plain grey; the background must become transparent."""
    image = Image.new("RGB", (16, 16), (180, 180, 180))
    for y in range(4, 12):
        for x in range(4, 12):
            image.putpixel((x, y), (200, 30, 40))
    path = tmp_path / "sprite.png"
    image.save(path)

    cut = np.asarray(planar.sprite_alpha(Image.open(path)))
    assert cut[0, 0, 3] == 0, "corner background should be transparent"
    assert cut[8, 8, 3] == 255, "the garment itself should stay opaque"


def test_sprite_alpha_keeps_an_existing_alpha_channel(tmp_path):
    image = Image.new("RGBA", (8, 8), (10, 20, 30, 0))
    image.putpixel((4, 4), (200, 30, 40, 255))
    path = tmp_path / "rgba.png"
    image.save(path)

    kept = np.asarray(planar.sprite_alpha(Image.open(path)))
    assert kept[0, 0, 3] == 0
    assert kept[4, 4, 3] == 255


@pytest.mark.parametrize("value,expected", [
    ("#A31F34", (163, 31, 52)),
    ("FFFFFF", (255, 255, 255)),
])
def test_parse_hex(value, expected):
    assert planar.parse_hex(value) == expected
