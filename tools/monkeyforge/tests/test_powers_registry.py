from __future__ import annotations

import pytest

from monkeyforge.powers.registry import (
    POWERS,
    PowerId,
    by_id,
    by_onehot,
    motif_by_phrase,
    onehot,
)


def test_onehot_order_matches_the_rust_enum():
    """Pinned to SuperpowerKind::onehot_index. Changing it invalidates PPO checkpoints."""
    assert [(p.onehot_index, p.rust_name) for p in POWERS] == [
        (0, "BeamBlast"),
        (1, "FreezeRay"),
        (2, "Boost"),
        (3, "Slow"),
    ]


def test_by_id_accepts_enum_and_string():
    assert by_id(PowerId.BOOST) is by_id("boost")


def test_by_id_rejects_unknown_power_and_names_the_alternatives():
    with pytest.raises(KeyError, match="beam_blast"):
        by_id("super_speed")


def test_by_onehot_roundtrips_every_power():
    for power in POWERS:
        assert by_onehot(power.onehot_index) is power


def test_by_onehot_rejects_out_of_range():
    with pytest.raises(KeyError):
        by_onehot(4)


def test_onehot_vector_is_the_width_the_observation_expects():
    assert onehot(PowerId.FREEZE_RAY) == [0.0, 1.0, 0.0, 0.0]
    assert all(len(onehot(p.id)) == 4 for p in POWERS)


def test_accents_are_hex_and_distinct():
    accents = [p.accent for p in POWERS]
    assert len(set(accents)) == len(accents)
    for accent in accents:
        assert len(accent) == 7 and accent[0] == "#"
        int(accent[1:], 16)


def test_every_power_has_motifs_and_every_motif_has_a_palette():
    for power in POWERS:
        assert len(power.motifs) >= 5, power.id
        for motif in power.motifs:
            assert motif.phrase.strip()
            assert motif.palette.strip()


def test_no_motif_is_shared_between_two_powers():
    """A motif in two prototypes is a permanent confusion the classifier cannot resolve."""
    seen: dict[str, PowerId] = {}
    for power in POWERS:
        for motif in power.motifs:
            assert motif.phrase not in seen, f"{motif.phrase!r} is in two powers"
            seen[motif.phrase] = power.id


def test_motif_palettes_are_not_uniform_within_a_power():
    """The whole point: two drawings of one power must not become one look.

    If every Boost motif carried the same colours, a spring and a bolt would render
    identically and drawing would stop mattering.
    """
    for power in POWERS:
        palettes = {motif.palette for motif in power.motifs}
        assert len(palettes) > len(power.motifs) // 2, power.id


def test_the_spring_is_steel_not_boost_gold():
    """Regression guard for the case that motivated per-motif palettes."""
    spring = motif_by_phrase("a coiled spring")
    assert "steel" in spring.palette
    assert "gold" not in spring.palette
    assert by_id(PowerId.BOOST).accent == "#F5C518", "the power keeps its badge colour"


def test_motif_lookup_rejects_an_unknown_phrase():
    with pytest.raises(KeyError):
        motif_by_phrase("a thing nobody drew")
