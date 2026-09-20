"""Sleeve-reading logic tests with a stand-in embedder, so they run without torch.

`FakeEmbedder` puts each sleeve length on its own basis vector and resolves text by which
length's motifs it mentions. That is a caricature of CLIP, but it exercises what this module
owns: prototype construction, sketch/description fusion, the softmax, the unsure rule, and --
most importantly -- that an unsure reading falls back rather than dressing a monkey in sleeves
it never had.
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

import numpy as np
import pytest
from PIL import Image, ImageDraw

from monkeyforge.garments import (
    FALLBACK_SLEEVE,
    GARMENT_PROMPTS,
    SLEEVES,
    GarmentReader,
    garment_prompt,
    reach_for,
    resolve_sleeves,
    sleeve_ids,
    sleeves_from_garment,
)


class FakeEmbedder:
    """Maps any text or image to the basis vector of whichever sleeve length it mentions."""

    def __init__(self) -> None:
        self.text_calls = 0

    @staticmethod
    def _vector_for(text: str) -> np.ndarray:
        vector = np.zeros(len(SLEEVES), dtype=np.float64)
        lowered = text.lower()
        for index, style in enumerate(SLEEVES):
            if any(motif in lowered for motif in style.motifs):
                vector[index] += 1.0
        if not vector.any():
            vector[:] = 1.0 / len(SLEEVES)
        return vector

    def embed_texts(self, texts: Sequence[str]) -> np.ndarray:
        self.text_calls += 1
        return np.stack([self._vector_for(text) for text in texts])

    def embed_image(self, image_path: Path) -> np.ndarray:
        return self._vector_for(image_path.read_bytes().decode("utf-8", "ignore"))


@pytest.fixture
def sketch(tmp_path):
    def write(motif: str) -> Path:
        image = Image.new("RGB", (64, 64), (255, 255, 255))
        ImageDraw.Draw(image).line([8, 8, 56, 56], fill=(0, 0, 0), width=3)
        path = tmp_path / f"{abs(hash(motif))}.png"
        image.save(path)
        # The fake embedder reads the file as text, so the motif has to survive alongside the
        # PNG bytes; appending works because PNG readers stop at IEND.
        path.write_bytes(path.read_bytes() + motif.encode("utf-8"))
        return path

    return write


def test_prototypes_are_built_once(sketch):
    embedder = FakeEmbedder()
    reader = GarmentReader(embedder=embedder)
    assert embedder.text_calls == 1
    reader.read(sketch_path=sketch("a short sleeved t-shirt"))
    assert embedder.text_calls == 1, "prototype pass must not repeat per request"


def test_short_sleeves_read_from_the_sketch(sketch):
    reading = GarmentReader(embedder=FakeEmbedder()).read(
        sketch_path=sketch("a short sleeved t-shirt")
    )
    assert reading.sleeve == "short"
    assert reading.reach == pytest.approx(0.38)
    assert reading.used_sketch and not reading.used_description
    assert not reading.unsure


def test_a_suit_gets_sleeves_to_the_wrist_not_over_the_hands(sketch):
    """A suit must not be built with t-shirt sleeves -- nor with cloth over the fingers."""
    reading = GarmentReader(embedder=FakeEmbedder()).read(
        sketch_path=sketch("a long sleeved suit jacket")
    )
    assert reading.sleeve == "long"
    assert reading.reach == pytest.approx(0.70)
    assert reading.reach < 1.0, "sleeves must stop at the wrist and leave the hands visible"


def test_a_vest_covers_no_arm_at_all(sketch):
    reading = GarmentReader(embedder=FakeEmbedder()).read(
        sketch_path=sketch("a sleeveless vest with bare arms")
    )
    assert reading.sleeve == "sleeveless"
    assert reading.reach == 0.0


def test_description_alone_is_enough():
    reading = GarmentReader(embedder=FakeEmbedder()).read(
        description="a business suit with full length sleeves"
    )
    assert reading.sleeve == "long"
    assert reading.used_description and not reading.used_sketch


def test_scores_are_a_distribution_over_every_length(sketch):
    reading = GarmentReader(embedder=FakeEmbedder()).read(
        sketch_path=sketch("a basketball singlet")
    )
    assert set(reading.scores) == set(sleeve_ids())
    assert sum(reading.scores.values()) == pytest.approx(1.0)


def test_an_unreadable_sketch_falls_back_rather_than_guessing(sketch):
    """A coin flip between sleeveless and long must not put cloth on the whole arm."""
    reading = GarmentReader(embedder=FakeEmbedder()).read(sketch_path=sketch("a squiggle"))
    assert reading.unsure
    assert reading.sleeve == FALLBACK_SLEEVE
    assert reading.reach == reach_for(FALLBACK_SLEEVE)
    assert "?" in reading.message()


def test_a_confident_reading_asserts_rather_than_asks(sketch):
    reading = GarmentReader(embedder=FakeEmbedder()).read(
        sketch_path=sketch("a long sleeved suit jacket")
    )
    assert reading.message() == "Long sleeves - reach 0.70"


def test_description_conflicting_with_the_sketch_lowers_confidence(sketch):
    reader = GarmentReader(embedder=FakeEmbedder())
    agreeing = reader.read(
        sketch_path=sketch("a short sleeved t-shirt"),
        description="a short sleeved t-shirt",
    )
    conflicting = reader.read(
        sketch_path=sketch("a short sleeved t-shirt"),
        description="a long sleeved suit jacket",
    )
    assert conflicting.confidence < agreeing.confidence


def test_reach_is_always_a_usable_fraction(sketch):
    for style in SLEEVES:
        assert 0.0 <= style.reach <= 1.0
    assert reach_for("sleeveless") == 0.0
    with pytest.raises(KeyError, match="unknown sleeve length"):
        reach_for("three-quarter")


def test_garment_name_implies_its_sleeve_length():
    assert sleeves_from_garment("t-shirt") == "short"
    assert sleeves_from_garment("suit") == "long"
    assert sleeves_from_garment("vest") == "sleeveless"
    assert sleeves_from_garment("a rusty spaceship") is None
    assert sleeves_from_garment("") is None


def test_longest_garment_name_wins():
    """'a dark blue suit jacket' must not resolve on the substring 'jacket' alone."""
    assert sleeves_from_garment("a dark blue suit jacket") == "long"
    assert sleeves_from_garment("a goalkeeper jersey") == "long"
    assert sleeves_from_garment("a football jersey") == "short"


def test_garment_name_beats_an_unsure_sketch_reading(sketch):
    """The measured case: models name garments correctly and misjudge sleeves, so the name wins."""
    unsure = GarmentReader(embedder=FakeEmbedder()).read(sketch_path=sketch("a squiggle"))
    assert unsure.unsure
    sleeve, reach, source = resolve_sleeves("t-shirt", unsure)
    assert (sleeve, reach) == ("short", 0.38)
    assert "t-shirt" in source


def test_a_confident_sketch_reading_is_used_when_the_garment_is_unknown(sketch):
    confident = GarmentReader(embedder=FakeEmbedder()).read(
        sketch_path=sketch("a long sleeved suit jacket")
    )
    sleeve, reach, source = resolve_sleeves("some unknown thing", confident)
    assert (sleeve, reach) == ("long", 0.70)
    assert "sketch" in source


def test_everything_unknown_falls_back(sketch):
    sleeve, reach, source = resolve_sleeves("", None)
    assert sleeve == FALLBACK_SLEEVE
    assert reach == reach_for(FALLBACK_SLEEVE)
    assert "fallback" in source


def test_suit_is_disambiguated_for_the_image_model():
    """The bug this table exists for: bare "suit" made SDXL draw an armoured spacesuit."""
    prompt = garment_prompt({"garment": "suit"})
    assert "business suit jacket" in prompt
    assert "lapels" in prompt
    assert "spacesuit" not in prompt


def test_a_pencil_drawing_gets_a_default_colourway():
    """A pencil sketch names no colour, and an uncoloured prompt comes back washed out."""
    assert "charcoal" in garment_prompt({"garment": "suit"})
    assert "maroon" in garment_prompt({"garment": "jersey"})


def test_a_stated_colour_beats_the_default():
    prompt = garment_prompt({"garment": "suit", "colours": "bright red"})
    assert "bright red" in prompt
    assert "charcoal" not in prompt


def test_a_flat_lying_accessory_is_drawn_into_the_garment():
    """A tie has no silhouette, so it must be asked for as part of the shirt or it never appears."""
    prompt = garment_prompt({"garment": "suit", "accessories": ["tie"]})
    assert "necktie" in prompt


def test_geometry_accessories_stay_out_of_the_garment_prompt():
    """A cap becomes its own mesh; drawing it onto the shirt would be wrong."""
    prompt = garment_prompt({"garment": "t-shirt", "accessories": ["cap"]})
    assert "cap" not in prompt


def test_every_prompt_asks_for_exactly_one_front_view():
    """Diffusion returns product turnarounds unless told not to."""
    for name in GARMENT_PROMPTS:
        prompt = garment_prompt({"garment": name})
        assert "one single garment" in prompt
        assert "front view only" in prompt


def test_an_unknown_garment_still_produces_a_usable_prompt():
    prompt = garment_prompt({"garment": "a wizard poncho"})
    assert "wizard poncho" in prompt
    assert "front view only" in prompt


def test_longest_garment_phrase_wins_in_the_prompt_table():
    assert "satin lapels" in garment_prompt({"garment": "a black tuxedo"})


def test_requires_at_least_one_input():
    with pytest.raises(ValueError, match="needs a sketch"):
        GarmentReader(embedder=FakeEmbedder()).read(description="   ")


def test_rejects_an_out_of_range_image_weight():
    with pytest.raises(ValueError, match="image_weight"):
        GarmentReader(embedder=FakeEmbedder(), image_weight=1.5)
