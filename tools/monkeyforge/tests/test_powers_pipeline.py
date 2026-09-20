"""End-to-end pipeline tests on the placeholder generator, so they need no GPU."""

from __future__ import annotations

import importlib
import sys
from pathlib import Path

import numpy as np
import pytest

# `scripts/` is not a package; the trigger-token test needs to read the dataset builder
# to prove training captions and inference prompts agree.
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from PIL import Image, ImageDraw

from monkeyforge.powers.badge import BadgeStyle
from monkeyforge.powers.beautify import (
    LORA_TRIGGER,
    NEGATIVE_PROMPT,
    PROMPT_TEMPLATE,
    BeautifySettings,
    PlaceholderGenerator,
    build_prompt,
)
from monkeyforge.powers.classify import PROMPT_TEMPLATES, PowerClassifier
from monkeyforge.powers.pipeline import IconPipeline, replace_seed
from monkeyforge.powers.registry import POWERS, PowerId, by_id, motif_by_phrase
from tests.test_powers_classify import FakeEmbedder

STYLE = BadgeStyle(size=128)


@pytest.fixture
def sketch(tmp_path):
    def write(motif: str) -> Path:
        image = Image.new("RGB", (128, 128), (255, 255, 255))
        ImageDraw.Draw(image).ellipse([30, 30, 98, 98], outline=(0, 0, 0), width=4)
        path = tmp_path / f"{motif.replace(' ', '_')}.png"
        image.save(path)
        # FakeEmbedder resolves an image by reading the file as text, so the motif has
        # to survive alongside the PNG bytes; appending it as a trailing comment works
        # because PNG readers stop at IEND.
        path.write_bytes(path.read_bytes() + motif.encode("utf-8"))
        return path

    return write


def pipeline() -> IconPipeline:
    return IconPipeline(
        classifier=PowerClassifier(embedder=FakeEmbedder()),
        generator=PlaceholderGenerator(),
        badge_style=STYLE,
    )


def test_run_produces_a_badge_and_a_decision(sketch):
    result = pipeline().run(sketch_path=sketch("a snowflake"))
    assert result.power.id is PowerId.FREEZE_RAY
    assert result.icon.size == (STYLE.size, STYLE.size)
    assert result.icon.mode == "RGBA"
    assert result.generator == "placeholder"


def test_the_icon_is_a_disc_not_a_square(sketch):
    icon = pipeline().run(sketch_path=sketch("an hourglass")).icon
    alpha = np.asarray(icon)[..., 3]
    assert alpha[0, 0] == 0
    assert alpha[STYLE.size // 2, STYLE.size // 2] == 255


def test_description_only_still_yields_an_icon():
    result = IconPipeline(
        classifier=PowerClassifier(embedder=FakeEmbedder()), badge_style=STYLE
    ).run(description="a lightning bolt, make me fast")
    assert result.power.id is PowerId.BOOST
    assert not result.guess.used_sketch


def test_save_writes_the_png(tmp_path, sketch):
    destination = tmp_path / "nested" / "icon.png"
    written = pipeline().run(sketch_path=sketch("a snail")).save(destination)
    assert written.exists() and written.stat().st_size > 0
    assert Image.open(written).size == (STYLE.size, STYLE.size)


def test_message_is_surfaced_from_the_guess(sketch):
    result = pipeline().run(sketch_path=sketch("a squiggle nobody can read"))
    assert result.message == result.guess.message()
    assert "or did you mean" in result.message


def test_the_classifier_names_a_motif_inside_the_chosen_power(sketch):
    guess = pipeline().run(sketch_path=sketch("a coiled spring")).guess
    assert guess.motif in {m.phrase for m in by_id(guess.power).motifs}


def test_motif_and_its_own_palette_feed_the_generation_prompt(sketch):
    guess = pipeline().run(sketch_path=sketch("a tortoise")).guess
    prompt = build_prompt(motif_by_phrase(guess.motif))
    assert guess.motif in prompt
    assert guess.motif_palette in prompt


def test_the_power_palette_never_reaches_the_subject_prompt(sketch):
    """A spring that maps to Boost must render as steel, not as Boost-gold.

    Two players drawing different things for the same power have to get different
    artwork, or drawing it stops meaning anything.
    """
    spring = pipeline().run(sketch_path=sketch("a coiled spring")).guess
    bolt = pipeline().run(sketch_path=sketch("a lightning bolt")).guess
    assert spring.power is bolt.power is PowerId.BOOST
    assert build_prompt(motif_by_phrase(spring.motif)) != build_prompt(
        motif_by_phrase(bolt.motif)
    )
    assert "steel" in spring.motif_palette and "yellow" in bolt.motif_palette


def test_the_badge_still_identifies_the_power(sketch):
    """Identity drives the subject; the power still has to be readable from the badge."""
    spring = pipeline().run(sketch_path=sketch("a coiled spring"))
    snail = pipeline().run(sketch_path=sketch("a snail"))
    assert spring.power.accent != snail.power.accent


def test_seed_override_does_not_disturb_the_other_settings():
    base = BeautifySettings(control_scale=0.4, guidance=9.0, steps=12, resolution=768, seed=1)
    rerolled = replace_seed(base, 99)
    assert rerolled.seed == 99
    assert (rerolled.control_scale, rerolled.guidance, rerolled.steps, rerolled.resolution) == (
        0.4,
        9.0,
        12,
        768,
    )


def test_every_motif_composes_a_prompt_within_clip_token_budget():
    """CLIP truncates at 77 tokens silently, dropping whatever sits at the end.

    Counted as whitespace words, which overestimates nothing: sub-word tokenisation
    only ever splits further, so a comfortable word-count margin is the check that can
    run without a tokenizer installed.
    """
    for power in POWERS:
        for motif in power.motifs:
            assert len(build_prompt(motif).split()) < 60, motif.phrase
    assert len(NEGATIVE_PROMPT.split()) < 60


def test_the_lora_trigger_is_prepended_only_when_asked():
    """A LoRA trained on a trigger token does nothing if the prompt omits it."""
    motif = by_id(PowerId.BOOST).motifs[0]
    assert LORA_TRIGGER not in build_prompt(motif)
    assert build_prompt(motif, LORA_TRIGGER).startswith(LORA_TRIGGER)


def test_the_dataset_caption_and_the_prompt_share_one_trigger():
    """Train and inference must spell the token identically or the adapter is inert."""
    caption = importlib.import_module("build_lora_dataset").CAPTION
    assert caption.startswith(LORA_TRIGGER)


def test_prompt_template_puts_composition_before_style():
    """Sweeps showed SDXL builds a collage when this clause moves to the end."""
    assert PROMPT_TEMPLATE.index("isolated object centered") < PROMPT_TEMPLATE.index("glossy")


def test_classifier_templates_name_the_medium():
    """Sketches are not photos; the text side has to say so to land near them."""
    assert any("sketch" in t or "drawing" in t or "doodle" in t for t in PROMPT_TEMPLATES)
