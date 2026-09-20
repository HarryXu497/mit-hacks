"""Map a drawing (and optional description) onto one of the four runtime powers.

This is retrieval against a closed list, not classification that needs training. CLIP
already scores an image against arbitrary text; with a fixed four-power vocabulary
there is nothing to learn that hand-labelling hundreds of sketches would add.

Two things make it work better than the naive version:

* **Prompt ensembling.** Each power is represented by the average of many embedded
  phrasings of what a player might draw (`Power.motifs` x `PROMPT_TEMPLATES`), not by
  one embedded label. A single label is the single biggest accuracy loss in zero-shot
  CLIP use.
* **Both inputs.** The sketch and the typed description are scored against the same
  prototypes and fused. A squiggle plus "make me fast" is unambiguous even when the
  squiggle alone is not.

The embedder is a protocol so the scoring logic is testable without torch installed;
`ClipEmbedder` is the real one and imports torch lazily.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

import numpy as np
from pydantic import Field

from monkeyforge.models import StrictModel
from monkeyforge.powers.registry import POWERS, Motif, Power, PowerId, by_id

DEFAULT_CLIP_MODEL = "openai/clip-vit-base-patch32"

# Sketches are not photographs. Naming the medium in the prompt moves the text
# embedding towards the same region of the space the drawing lands in.
PROMPT_TEMPLATES: tuple[str, ...] = (
    "a rough pencil sketch of {motif}",
    "a child's crayon drawing of {motif}",
    "a simple black and white doodle of {motif}",
    "a video game ability icon of {motif}",
    "{motif}",
)

# Cosine similarities between CLIP embeddings live in a narrow band (roughly 0.15-0.35
# for prompt-ensembled prototypes), so they need scaling before softmax or every class
# comes out near 0.25. CLIP's own trained logit scale is ~100, which drives the result
# to a hard one-hot and destroys exactly the "or did you mean...?" signal we want. 50 is
# a deliberate middle. It is NOT calibrated: treat the number as an ordering with a
# usable spread, and re-fit it against real sketches before showing it as a percentage.
DEFAULT_LOGIT_SCALE = 50.0

# How much the typed description counts against the drawing. The drawing leads because
# it is the thing the user deliberately made; the text disambiguates.
DEFAULT_IMAGE_WEIGHT = 0.65

# Below either bar we ask instead of asserting. A four-way softmax is at 0.25 when it
# knows nothing, so 0.45 is "meaningfully better than a guess" rather than "confident".
DEFAULT_CONFIDENCE_FLOOR = 0.45
DEFAULT_MARGIN_FLOOR = 0.12


class Embedder(Protocol):
    """Anything that can put images and text in one shared unit-norm space."""

    def embed_image(self, image_path: Path) -> np.ndarray: ...

    def embed_texts(self, texts: Sequence[str]) -> np.ndarray: ...


class PowerGuess(StrictModel):
    """What the classifier believes, including how much it believes it."""

    power: PowerId
    confidence: float = Field(ge=0, le=1)
    runner_up: PowerId
    runner_up_confidence: float = Field(ge=0, le=1)
    unsure: bool
    scores: dict[PowerId, float]
    used_sketch: bool
    used_description: bool
    # The closest single motif within the chosen power -- "a coiled spring" rather than
    # just "boost" -- and that thing's own colours. Together these are what keep the
    # artwork the player's: two people who both draw Boost get a steel spring and a
    # yellow bolt, not two copies of one Boost icon. Free from scores already computed
    # for the power decision.
    motif: str
    motif_palette: str

    @property
    def resolved(self) -> Power:
        return by_id(self.power)

    def message(self) -> str:
        """The line to show a user: an assertion when sure, a question when not."""
        first = by_id(self.power).display_name
        second = by_id(self.runner_up).display_name
        if self.unsure:
            return f"I think this is {first} — or did you mean {second}?"
        return f"That's {first}."


def _l2_normalise(matrix: np.ndarray) -> np.ndarray:
    norms = np.linalg.norm(matrix, axis=-1, keepdims=True)
    return matrix / np.maximum(norms, 1e-12)


def _softmax(logits: np.ndarray) -> np.ndarray:
    shifted = logits - logits.max()
    exponentiated = np.exp(shifted)
    return exponentiated / exponentiated.sum()


def power_phrases(power: Power) -> list[str]:
    """Every phrasing used to represent one power in the text encoder."""
    return [
        template.format(motif=motif.phrase)
        for motif in power.motifs
        for template in PROMPT_TEMPLATES
    ]


@dataclass
class PowerClassifier:
    """Scores a sketch against the four power prototypes.

    Prototypes are built once on construction (one text-encoder pass over ~200 short
    phrases) and reused for every sketch, so per-request cost is a single image
    embedding.
    """

    embedder: Embedder
    powers: tuple[Power, ...] = POWERS
    logit_scale: float = DEFAULT_LOGIT_SCALE
    image_weight: float = DEFAULT_IMAGE_WEIGHT
    confidence_floor: float = DEFAULT_CONFIDENCE_FLOOR
    margin_floor: float = DEFAULT_MARGIN_FLOOR

    def __post_init__(self) -> None:
        if not 0.0 <= self.image_weight <= 1.0:
            raise ValueError("image_weight must be in [0, 1]")
        self._prototypes = self._build_prototypes()

    def _build_prototypes(self) -> np.ndarray:
        """Build motif prototypes, then a power prototype as the mean of its motifs.

        Going via motifs costs nothing extra -- the same phrases are embedded either
        way -- and leaves per-motif vectors available for naming what was drawn.
        """
        phrases: list[str] = []
        motif_spans: list[tuple[int, int]] = []
        power_spans: list[tuple[int, int]] = []

        for power in self.powers:
            first_motif = len(motif_spans)
            for motif in power.motifs:
                start = len(phrases)
                phrases.extend(
                    template.format(motif=motif.phrase) for template in PROMPT_TEMPLATES
                )
                motif_spans.append((start, len(phrases)))
            power_spans.append((first_motif, len(motif_spans)))

        embedded = _l2_normalise(np.asarray(self.embedder.embed_texts(phrases), dtype=np.float64))
        self._motifs = _l2_normalise(
            np.stack([embedded[start:end].mean(axis=0) for start, end in motif_spans])
        )
        self._motif_list = [motif for power in self.powers for motif in power.motifs]
        self._motif_spans = power_spans
        return _l2_normalise(
            np.stack([self._motifs[start:end].mean(axis=0) for start, end in power_spans])
        )

    def _best_motif(self, power_index: int, query: np.ndarray) -> Motif:
        start, end = self._motif_spans[power_index]
        offset = int(np.argmax(self._motifs[start:end] @ query))
        return self._motif_list[start + offset]

    def classify(self, sketch_path: Path | None = None, description: str = "") -> PowerGuess:
        description = description.strip()
        if sketch_path is None and not description:
            raise ValueError("classify needs a sketch, a description, or both")

        # One fused query vector rather than two similarity passes: identical
        # arithmetic, and it leaves something to compare the motifs against.
        query = np.zeros(self._prototypes.shape[1], dtype=np.float64)
        weight = 0.0

        if sketch_path is not None:
            query += self.image_weight * _l2_normalise(
                np.asarray(self.embedder.embed_image(sketch_path), dtype=np.float64)
            )
            weight += self.image_weight

        if description:
            query += (1.0 - self.image_weight) * _l2_normalise(
                np.asarray(self.embedder.embed_texts([description]), dtype=np.float64)[0]
            )
            weight += 1.0 - self.image_weight

        similarity = self._prototypes @ query

        if weight <= 0.0:
            # image_weight of exactly 0 or 1 can starve the only input we were given.
            raise ValueError(
                f"image_weight={self.image_weight} gives the supplied inputs no weight at all"
            )

        probabilities = _softmax(similarity / weight * self.logit_scale)
        order = np.argsort(probabilities)[::-1]
        best, second = int(order[0]), int(order[1])
        motif = self._best_motif(best, query)
        top = float(probabilities[best])
        runner_up = float(probabilities[second])

        return PowerGuess(
            power=self.powers[best].id,
            confidence=top,
            runner_up=self.powers[second].id,
            runner_up_confidence=runner_up,
            unsure=top < self.confidence_floor or (top - runner_up) < self.margin_floor,
            scores={
                power.id: float(probability)
                for power, probability in zip(self.powers, probabilities, strict=True)
            },
            used_sketch=sketch_path is not None,
            used_description=bool(description),
            motif=motif.phrase,
            motif_palette=motif.palette,
        )


class ClipEmbedder:
    """The real embedder: HuggingFace CLIP, loaded once.

    torch and transformers are imported inside `__init__` so that importing this module
    -- and therefore the whole `powers` package -- costs nothing on a machine that only
    needs the registry or the badge compositor.
    """

    def __init__(self, model_name: str = DEFAULT_CLIP_MODEL, device: str | None = None) -> None:
        import torch
        from transformers import CLIPModel, CLIPProcessor

        self._torch = torch
        self.model_name = model_name
        self.device = device or ("cuda" if torch.cuda.is_available() else "cpu")
        self._model = CLIPModel.from_pretrained(model_name).to(self.device).eval()
        self._processor = CLIPProcessor.from_pretrained(model_name)

    def embed_image(self, image_path: Path) -> np.ndarray:
        from PIL import Image

        with Image.open(image_path) as handle:
            # Sketches are routinely RGBA or 1-bit; CLIP's processor wants 3 channels,
            # and flattening onto white keeps pen strokes dark rather than turning a
            # transparent background into black.
            image = handle.convert("RGBA")
            canvas = Image.new("RGBA", image.size, (255, 255, 255, 255))
            canvas.alpha_composite(image)
            rgb = canvas.convert("RGB")

        inputs = self._processor(images=rgb, return_tensors="pt").to(self.device)
        with self._torch.no_grad():
            features = self._model.get_image_features(**inputs)
        return self._to_array(features)[0]

    def embed_texts(self, texts: Sequence[str]) -> np.ndarray:
        inputs = self._processor(
            text=list(texts), return_tensors="pt", padding=True, truncation=True
        ).to(self.device)
        with self._torch.no_grad():
            features = self._model.get_text_features(**inputs)
        return self._to_array(features)

    @staticmethod
    def _to_array(features: object) -> np.ndarray:
        """Normalise the return type across transformers versions.

        transformers <5 returns the projected tensor directly; 5.x wraps it in a
        `BaseModelOutputWithPooling`. The projected, shared-space vector is
        `pooler_output` -- for the vision tower `last_hidden_state` is the unprojected
        768-d hidden state, so reading that would silently score images and text in
        two different spaces instead of raising.
        """
        pooled = getattr(features, "pooler_output", features)
        return pooled.float().cpu().numpy()
