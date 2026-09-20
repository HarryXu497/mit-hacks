from __future__ import annotations

import numpy as np
import pytest
from PIL import Image, ImageDraw

from monkeyforge.powers.sketchmask import (
    apply_mask,
    line_art,
    otsu_threshold,
    subject_mask,
    trim,
)

SIZE = 256


def drawing(render) -> Image.Image:
    image = Image.new("L", (SIZE, SIZE), 255)
    render(ImageDraw.Draw(image))
    return image.convert("RGB")


def closed_circle() -> Image.Image:
    return drawing(lambda d: d.ellipse([60, 60, 196, 196], outline=0, width=5))


def open_stroke() -> Image.Image:
    return drawing(lambda d: d.line([40, 128, 216, 128], fill=0, width=5))


def coverage(mask: Image.Image) -> float:
    return float((np.asarray(mask) > 128).mean())


def test_otsu_finds_the_split_of_a_bimodal_image():
    """Returns the first maximising level, so the dark mode itself is the boundary.

    `_ink` classifies with `<= threshold`, so 20 correctly puts the dark pixels in the
    ink class and leaves the 220s as paper.
    """
    gray = np.concatenate([np.full(500, 20, np.uint8), np.full(500, 220, np.uint8)])
    assert 20 <= otsu_threshold(gray) < 220


def test_otsu_survives_a_uniform_image():
    assert 0 <= otsu_threshold(np.full(100, 128, np.uint8)) <= 255


def test_line_art_is_white_strokes_on_black():
    art = np.asarray(line_art(closed_circle(), size=SIZE).convert("L"))
    assert art[0, 0] == 0, "paper becomes black"
    assert art.max() == 255, "ink becomes white"
    # A thin outline should stay a small minority of the frame, not flood it.
    assert 0.0 < (art > 128).mean() < 0.25


def test_line_art_output_is_square_at_the_requested_size():
    assert line_art(open_stroke(), size=384).size == (384, 384)


def test_a_closed_shape_has_its_interior_filled():
    mask = subject_mask(closed_circle(), size=SIZE)
    centre = np.asarray(mask)[SIZE // 2, SIZE // 2]
    assert centre > 200, "the inside of a drawn circle belongs to the subject"
    # The circle spans 136 of 256 px, so it covers pi/4 * (136/256)^2 ~= 0.22 of the
    # frame. Well above the outline alone, and nowhere near flooding the canvas.
    assert 0.18 < coverage(mask) < 0.45


def test_an_open_stroke_is_not_flood_filled():
    """Nothing is enclosed, so the mask must stay near the line itself."""
    assert coverage(subject_mask(open_stroke(), size=SIZE)) < 0.3


def test_the_mask_grows_the_stroke_beyond_the_drawn_line():
    """The rendered outline is far thicker than a pencil line; a tight mask crops it."""
    thin = (np.asarray(open_stroke().convert("L")) < 128).mean()
    # A 5px line under a 5px growth window becomes ~9px: about 1.8x, not 2x.
    assert coverage(subject_mask(open_stroke(), size=SIZE)) > thin * 1.5


def test_blank_paper_produces_an_almost_empty_mask():
    blank = Image.new("RGB", (SIZE, SIZE), (255, 255, 255))
    assert coverage(subject_mask(blank, size=SIZE)) < 0.5


def test_apply_mask_makes_the_background_transparent_and_crops():
    flood = Image.new("RGB", (SIZE, SIZE), (255, 0, 255))
    result = apply_mask(flood, subject_mask(closed_circle(), size=SIZE))
    assert result.mode == "RGBA"
    assert result.width <= SIZE and result.height <= SIZE
    assert np.asarray(result)[..., 3].max() > 200


def test_trim_removes_transparent_margins():
    image = Image.new("RGBA", (100, 100), (0, 0, 0, 0))
    image.paste((255, 0, 0, 255), (40, 45, 60, 55))
    assert trim(image).size == (20, 10)


def test_trim_leaves_a_fully_transparent_image_alone():
    empty = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
    assert trim(empty).size == (32, 32)


@pytest.mark.parametrize("size", [128, 256, 512])
def test_mask_matches_the_requested_size(size):
    assert subject_mask(closed_circle(), size=size).size == (size, size)
