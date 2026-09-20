"""Scoring-logic tests with a stand-in embedder, so they run without torch.

`FakeEmbedder` puts each power on its own basis vector of a 4-dimensional space and
resolves any text by which power's motifs it mentions. That is a caricature of CLIP,
but it exercises exactly the parts this module owns: prototype construction, weighted
fusion of sketch and description, the softmax, and the unsure rule.
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

import numpy as np
import pytest

from monkeyforge.powers.classify import PowerClassifier
from monkeyforge.powers.registry import POWERS, PowerId

_INDEX = {power.id: power.onehot_index for power in POWERS}


ALL_MOTIFS = [motif.phrase for power in POWERS for motif in power.motifs]


class FakeEmbedder:
    """Puts each *motif* on its own basis vector of a len(ALL_MOTIFS)-dimensional space.

    One dimension per motif rather than per power, because the classifier has to
    resolve which motif inside a power was drawn -- a spring and a bolt are both Boost
    but must not come out as the same subject. A power prototype then falls out
    naturally as the mean of its motifs, exactly as the real code builds it.
    """

    def __init__(self) -> None:
        self.text_calls = 0

    @staticmethod
    def _vector_for(text: str) -> np.ndarray:
        vector = np.zeros(len(ALL_MOTIFS), dtype=np.float64)
        lowered = text.lower()
        for index, phrase in enumerate(ALL_MOTIFS):
            if phrase in lowered:
                vector[index] += 1.0
        if not vector.any():
            vector[:] = 0.25  # nothing recognisable: equidistant from every prototype
        return vector

    def embed_texts(self, texts: Sequence[str]) -> np.ndarray:
        self.text_calls += 1
        return np.stack([self._vector_for(text) for text in texts])

    def embed_image(self, image_path: Path) -> np.ndarray:
        # Read as bytes so the same fake works for a plain text stand-in and for a real
        # PNG with the motif appended after IEND.
        return self._vector_for(image_path.read_bytes().decode("utf-8", errors="ignore"))


@pytest.fixture
def sketch(tmp_path):
    def write(motif: str) -> Path:
        path = tmp_path / "sketch.txt"
        path.write_text(motif, encoding="utf-8")
        return path

    return write


def test_prototypes_are_built_once_not_per_classification(sketch):
    embedder = FakeEmbedder()
    classifier = PowerClassifier(embedder=embedder)
    assert embedder.text_calls == 1
    classifier.classify(sketch_path=sketch("a lightning bolt"))
    assert embedder.text_calls == 1, "prototype pass must not repeat per request"


def test_sketch_alone_picks_the_matching_power(sketch):
    guess = PowerClassifier(embedder=FakeEmbedder()).classify(sketch_path=sketch("a snowflake"))
    assert guess.power is PowerId.FREEZE_RAY
    assert guess.used_sketch and not guess.used_description
    assert not guess.unsure


def test_description_alone_is_enough(sketch):
    guess = PowerClassifier(embedder=FakeEmbedder()).classify(description="an hourglass, but slow")
    assert guess.power is PowerId.SLOW
    assert guess.used_description and not guess.used_sketch


def test_the_brief_examples_both_resolve_to_boost(sketch):
    """A spring and a lightning bolt are the same power here; there is no separate speed one."""
    classifier = PowerClassifier(embedder=FakeEmbedder())
    assert classifier.classify(sketch_path=sketch("a coiled spring")).power is PowerId.BOOST
    assert classifier.classify(sketch_path=sketch("a lightning bolt")).power is PowerId.BOOST


def test_scores_are_a_distribution_over_every_power(sketch):
    guess = PowerClassifier(embedder=FakeEmbedder()).classify(sketch_path=sketch("an explosion"))
    assert set(guess.scores) == {power.id for power in POWERS}
    assert sum(guess.scores.values()) == pytest.approx(1.0)


def test_unrecognisable_sketch_is_flagged_unsure_with_a_runner_up(sketch):
    guess = PowerClassifier(embedder=FakeEmbedder()).classify(sketch_path=sketch("a squiggle"))
    assert guess.unsure
    assert guess.runner_up != guess.power
    assert "or did you mean" in guess.message()


def test_confident_guess_asserts_rather_than_asks(sketch):
    guess = PowerClassifier(embedder=FakeEmbedder()).classify(sketch_path=sketch("a snowflake"))
    assert guess.message() == "That's Freeze Ray."


def test_description_can_override_a_weaker_sketch(sketch):
    """Image leads by default, so a conflicting description should at least raise doubt."""
    classifier = PowerClassifier(embedder=FakeEmbedder())
    agreeing = classifier.classify(sketch_path=sketch("a snowflake"), description="a snowflake")
    conflicting = classifier.classify(sketch_path=sketch("a snowflake"), description="an hourglass")
    assert conflicting.confidence < agreeing.confidence
    assert conflicting.runner_up is PowerId.SLOW


def test_text_only_weighting_still_classifies_when_image_weight_is_zero(sketch):
    classifier = PowerClassifier(embedder=FakeEmbedder(), image_weight=0.0)
    assert classifier.classify(description="a snail").power is PowerId.SLOW


def test_rejects_an_input_combination_that_carries_no_weight(sketch):
    classifier = PowerClassifier(embedder=FakeEmbedder(), image_weight=0.0)
    with pytest.raises(ValueError, match="no weight"):
        classifier.classify(sketch_path=sketch("a snowflake"))


def test_requires_at_least_one_input():
    with pytest.raises(ValueError, match="needs a sketch"):
        PowerClassifier(embedder=FakeEmbedder()).classify(description="   ")


def test_rejects_an_out_of_range_image_weight():
    with pytest.raises(ValueError, match="image_weight"):
        PowerClassifier(embedder=FakeEmbedder(), image_weight=1.5)


def test_resolved_returns_the_registry_entry(sketch):
    guess = PowerClassifier(embedder=FakeEmbedder()).classify(sketch_path=sketch("a tortoise"))
    assert guess.resolved.rust_name == "Slow"
    assert guess.resolved.cooldown_seconds == 8.0
