"""sketch + description -> power, confidence, and a finished badge PNG.

    guess -> subject -> composite

Classification and beautification are kept apart because they fail differently and
want different tools. A misclassification is a product question -- ask the player,
using `PowerIcon.message`. A poor subject is a quality question -- reroll the seed.
Neither can produce a malformed badge, because the frame is drawn, not generated.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from PIL import Image

from monkeyforge.powers.badge import BadgeStyle, render_badge
from monkeyforge.powers.beautify import BeautifySettings, PlaceholderGenerator, SubjectGenerator
from monkeyforge.powers.classify import PowerClassifier, PowerGuess
from monkeyforge.powers.registry import Power, motif_by_phrase


@dataclass(frozen=True)
class PowerIcon:
    """Everything a caller needs: the gameplay decision and the artwork."""

    guess: PowerGuess
    icon: Image.Image
    subject: Image.Image
    generator: str

    @property
    def power(self) -> Power:
        return self.guess.resolved

    @property
    def message(self) -> str:
        return self.guess.message()

    def save(self, path: Path) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        self.icon.save(path)
        return path


@dataclass
class IconPipeline:
    classifier: PowerClassifier
    generator: SubjectGenerator = field(default_factory=PlaceholderGenerator)
    badge_style: BadgeStyle = field(default_factory=BadgeStyle)
    settings: BeautifySettings = field(default_factory=BeautifySettings)

    def run(
        self,
        sketch_path: Path | None = None,
        description: str = "",
        seed: int | None = None,
    ) -> PowerIcon:
        guess = self.classifier.classify(sketch_path=sketch_path, description=description)
        power = guess.resolved

        motif = motif_by_phrase(guess.motif)

        settings = self.settings if seed is None else replace_seed(self.settings, seed)
        if sketch_path is None:
            # Nothing was drawn, so there is no shape to preserve and nothing for the
            # generator to condition on. The power's own glyph is the honest answer.
            subject = PlaceholderGenerator().generate(sketch_path, power, motif, settings)
            generator = PlaceholderGenerator.name
        else:
            subject = self.generator.generate(sketch_path, power, motif, settings)
            generator = getattr(self.generator, "name", type(self.generator).__name__)

        return PowerIcon(
            guess=guess,
            icon=render_badge(subject, power.accent, self.badge_style),
            subject=subject,
            generator=generator,
        )


def replace_seed(settings: BeautifySettings, seed: int) -> BeautifySettings:
    """A per-request seed, so a player can reroll the art without a new classification."""
    return BeautifySettings(
        control_scale=settings.control_scale,
        guidance=settings.guidance,
        steps=settings.steps,
        resolution=settings.resolution,
        seed=seed,
    )
