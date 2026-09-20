from __future__ import annotations

import numpy as np
import pytest
from PIL import Image

from monkeyforge.powers.badge import (
    BadgeStyle,
    _hex_to_rgb,
    geometry,
    placeholder_badge,
    placeholder_subject,
    render_badge,
    render_frame,
)
from monkeyforge.powers.registry import POWERS, PowerId

STYLE = BadgeStyle(size=128)


def test_frame_is_square_rgba_at_the_requested_size():
    frame, _ = render_frame("#3FC6F0", STYLE)
    assert frame.size == (128, 128)
    assert frame.mode == "RGBA"


def test_badge_is_a_disc_transparent_outside_and_opaque_inside():
    frame, _ = render_frame("#3FC6F0", STYLE)
    alpha = np.asarray(frame)[..., 3]
    assert alpha[0, 0] == 0 and alpha[-1, -1] == 0, "corners must be cut away"
    assert alpha[64, 64] == 255, "centre must be opaque"


def test_backdrop_takes_the_accent_hue():
    frame, _ = render_frame("#3FC6F0", STYLE)
    red, green, blue = np.asarray(frame)[64, 64, :3]
    assert blue > green > red, "an icy accent should stay blue-dominant at the centre"


def test_each_power_renders_a_visibly_different_badge():
    rendered = [np.asarray(placeholder_badge(power.id, STYLE)) for power in POWERS]
    for index, first in enumerate(rendered):
        for second in rendered[index + 1 :]:
            assert not np.array_equal(first, second)


def test_rendering_is_deterministic():
    first = np.asarray(placeholder_badge(PowerId.BOOST, STYLE))
    second = np.asarray(placeholder_badge(PowerId.BOOST, STYLE))
    assert np.array_equal(first, second)


def test_an_oversized_subject_is_clipped_to_the_badge_not_overflowing_it():
    """A bad generation must degrade to a cropped subject, never a broken silhouette."""
    flood = Image.new("RGBA", (512, 512), (255, 0, 255, 255))
    badge = render_badge(flood, "#F5C518", STYLE)
    alpha = np.asarray(badge)[..., 3]
    assert alpha[0, 0] == 0 and alpha[0, -1] == 0
    assert alpha[64, 64] == 255


def test_subject_is_centred_inside_the_ring():
    geo = geometry(STYLE)
    dot = Image.new("RGBA", (16, 16), (255, 0, 255, 255))
    badge = np.asarray(render_badge(dot, "#8A5BD6", STYLE))
    magenta = np.argwhere((badge[..., 0] > 200) & (badge[..., 1] < 60) & (badge[..., 2] > 200))
    assert magenta.size, "the subject should be visible"
    centre = magenta.mean(axis=0)
    assert abs(centre[0] - geo.centre) < 1.5 and abs(centre[1] - geo.centre) < 1.5


def test_render_badge_without_a_subject_equals_the_bare_frame():
    frame, _ = render_frame("#F0483C", STYLE)
    assert np.array_equal(np.asarray(render_badge(None, "#F0483C", STYLE)), np.asarray(frame))


def test_geometry_keeps_the_subject_box_inside_the_canvas():
    geo = geometry(STYLE)
    left, top, right, bottom = geo.subject_box
    assert 0 <= left < right <= STYLE.size
    assert 0 <= top < bottom <= STYLE.size
    assert geo.subject_radius < geo.ring_inner_radius < geo.outer_radius


def test_placeholder_subject_is_transparent_at_its_corners():
    subject = placeholder_subject(PowerId.BOOST, size=128)
    assert np.asarray(subject)[0, 0, 3] == 0


def test_placeholder_subject_rejects_an_unknown_power():
    with pytest.raises(KeyError):
        placeholder_subject("time_stop")


@pytest.mark.parametrize("value", ["#ABC", "336699", ""])
def test_hex_parsing_rejects_anything_that_is_not_rrggbb(value):
    with pytest.raises(ValueError, match="RRGGBB"):
        _hex_to_rgb(value)


def test_hex_parsing_accepts_both_cases():
    assert _hex_to_rgb("#3fc6f0") == _hex_to_rgb("#3FC6F0") == (63, 198, 240)
