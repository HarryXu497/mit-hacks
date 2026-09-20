"""Read a garment's measurable properties out of a drawing.

`wardrobe.py` decides *how* a drawn item is built -- texture for things that lie flat, geometry for
things with a silhouette. This decides the numbers that build needs, and sleeve length is the one
that matters most: the same t-shirt code, given a suit, produces a suit with bare forearms.

Sleeve length is deliberately a **closed three-way choice scored by retrieval**, not something a
language model writes in prose. Two reasons:

* A vision-language model asked "describe this drawing" will happily invent cuffs and collars that
  were never drawn, and there is no signal in its output saying how sure it was. Retrieval against
  a fixed vocabulary returns a probability per option, so the pipeline can decline to guess.
* The answer is consumed as a float. Free text has to be parsed back into one anyway, and that
  parse is where a confident-sounding wrong answer becomes a silently wrong mesh.

What a VLM *is* good for is the open-ended half -- which items appear at all, and what the lettering
says -- because that has no closed vocabulary to retrieve against. The two are complementary;
`GarmentReader` covers the closed half.

Reach is expressed as a fraction of the arm's span from the body's midline, so the number means the
same thing on a different base mesh.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from pydantic import Field

from monkeyforge.models import StrictModel
from monkeyforge.powers.classify import PROMPT_TEMPLATES, Embedder


@dataclass(frozen=True)
class SleeveStyle:
    """One sleeve length, with the fraction of the arm it covers.

    `motifs` are things somebody would actually draw or say, not the engineering name. As with the
    power classifier, an ensemble of phrasings is markedly more accurate than a single label.
    """

    id: str
    reach: float
    motifs: tuple[str, ...]


#: The three lengths worth distinguishing. Anything between them rounds to one of these -- a
#: three-quarter sleeve drawn in pencil is not reliably separable from a long one, and pretending
#: otherwise would add a category the classifier cannot actually resolve.
SLEEVES: tuple[SleeveStyle, ...] = (
    SleeveStyle(
        id="sleeveless",
        reach=0.0,
        motifs=(
            "a sleeveless vest with bare arms",
            "a tank top showing both shoulders",
            "a basketball singlet",
            "a character wearing a shirt with no sleeves",
            "bare arms with no cloth on them",
        ),
    ),
    SleeveStyle(
        id="short",
        reach=0.38,
        motifs=(
            "a short sleeved t-shirt",
            "a football jersey with short sleeves",
            "a soccer kit with sleeves ending above the elbow",
            "a character wearing a tee shirt with stubby sleeves",
            "a sports top whose sleeves stop at the upper arm",
        ),
    ),
    SleeveStyle(
        id="long",
        # A long sleeve ends at the wrist, leaving the hands bare. 1.0 would run cloth over the
        # fingers. 0.70 is where the dart monkey's hand begins, measured from the arm island: face
        # density doubles there and the arm's front-to-back extent jumps from 2.78 to 3.57 as the
        # forearm becomes a hand. Re-measure this if the base mesh is ever swapped -- the wrist is
        # a property of the body, not of the garment.
        reach=0.70,
        motifs=(
            "a long sleeved suit jacket",
            "a hoodie with long sleeves down to the wrists",
            "a business suit with full length sleeves",
            "a goalkeeper jersey with long sleeves",
            "a character wearing a coat covering the whole arm",
        ),
    ),
)

#: Sleeve length implied by what the garment *is*.
#:
#: This is the primary path, and the measurement is why. Asked to judge sleeve length directly from
#: a rough drawing, CLIP split 36/34 between short and long on a t-shirt, and Qwen2-VL-2B answered
#: "long" for both a t-shirt and a suit -- neither is reading the arms. Asked what the garment is,
#: both answered correctly ("t-shirt", "suit"). A t-shirt has short sleeves by definition, so
#: naming the garment is a question the models can actually answer, and the sleeve length follows
#: from it without anyone squinting at pencil lines.
GARMENT_SLEEVES: dict[str, str] = {
    "t-shirt": "short",
    "tshirt": "short",
    "tee": "short",
    "tee shirt": "short",
    "jersey": "short",
    "football jersey": "short",
    "soccer jersey": "short",
    "kit": "short",
    "polo": "short",
    "polo shirt": "short",
    "shirt": "short",
    "top": "short",
    "suit": "long",
    "suit jacket": "long",
    "jacket": "long",
    "blazer": "long",
    "hoodie": "long",
    "sweater": "long",
    "jumper": "long",
    "coat": "long",
    "robe": "long",
    "armour": "long",
    "armor": "long",
    "goalkeeper jersey": "long",
    "vest": "sleeveless",
    "tank top": "sleeveless",
    "singlet": "sleeveless",
    "sleeveless shirt": "sleeveless",
}


#: How to say each garment to a diffusion model, and what colour it defaults to.
#:
#: The word a vision model uses for a garment is not the word a diffusion model draws. Asked for
#: "suit" -- which is exactly what the VLM returns for a drawn business suit -- SDXL produced an
#: armoured sci-fi spacesuit, because that is what "suit" most often labels in its training data.
#: Each entry is (subject phrase, default colourway); the phrase names the garment unambiguously
#: and the colourway fills in when the drawing is pencil and says nothing about colour.
GARMENT_PROMPTS: dict[str, tuple[str, str]] = {
    "suit": ("a business suit jacket with notch lapels over a white dress shirt",
             "dark charcoal grey"),
    "suit jacket": ("a business suit jacket with notch lapels over a white dress shirt",
                    "dark charcoal grey"),
    "blazer": ("a tailored blazer with notch lapels", "navy blue"),
    "tuxedo": ("a tuxedo jacket with satin lapels over a white dress shirt", "black"),
    "jacket": ("a zip-up jacket", "olive green"),
    "t-shirt": ("a plain short sleeved t-shirt", "white"),
    "tshirt": ("a plain short sleeved t-shirt", "white"),
    "tee": ("a plain short sleeved t-shirt", "white"),
    "shirt": ("a short sleeved shirt", "white"),
    "top": ("a short sleeved top", "white"),
    "jersey": ("a football jersey with a v-neck collar", "maroon and white"),
    "football jersey": ("a football jersey with a v-neck collar", "maroon and white"),
    "soccer jersey": ("a soccer jersey with a v-neck collar", "maroon and white"),
    "polo": ("a polo shirt with a buttoned collar", "navy blue"),
    "hoodie": ("a hooded sweatshirt with a front pocket", "grey"),
    "sweater": ("a knitted sweater", "burgundy"),
    "vest": ("a sleeveless vest with no sleeves at all", "black"),
    "tank top": ("a sleeveless tank top", "white"),
    "robe": ("a long flowing robe", "deep blue"),
    "armour": ("a fantasy plate armour chestplate", "polished steel"),
    "armor": ("a fantasy plate armour chestplate", "polished steel"),
    "lab coat": ("an open white laboratory coat", "white"),
    "coat": ("a long overcoat", "camel"),
}

#: Painted extras that belong in the garment image rather than as geometry, and how to ask for
#: them. A tie lies flat on the chest, so it is drawn *with* the shirt or it never appears at all.
CONFORMING_EXTRAS: dict[str, str] = {
    "tie": "with a dark necktie",
    "necktie": "with a dark necktie",
    "bowtie": "with a bow tie",
    "scarf": "with a scarf around the collar",
}


def garment_prompt(spec: dict) -> str:
    """Build the SDXL subject phrase for the garment a drawing described.

    Keeps the subject first: CLIP truncates at 77 tokens, and a style suffix has previously pushed
    the garment's own colour out of the window entirely.
    """
    raw = str(spec.get("garment") or "t-shirt").strip().lower()
    matches = [known for known in GARMENT_PROMPTS if known in raw]
    key = max(matches, key=len) if matches else None
    subject, default_colour = GARMENT_PROMPTS.get(key, (raw or "a t-shirt", ""))

    colour = str(spec.get("colours") or "").strip() or default_colour
    parts = [f"{colour} {subject}".strip() if colour else subject]

    for item in spec.get("accessories", []):
        word = str(item).strip().lower()
        for extra, phrase in CONFORMING_EXTRAS.items():
            if extra in word:
                parts.append(phrase)
                break

    # Diffusion loves returning a product turnaround. Saying "one" and naming the view is the
    # cheapest defence; the negative prompt carries the rest.
    parts.append("one single garment, front view only")
    return ", ".join(parts)


def sleeves_from_garment(name: str) -> str | None:
    """The sleeve length a named garment implies, or None if the name is not recognised.

    Matches the longest known name contained in the reply, so "a dark blue suit jacket" resolves
    to `suit jacket` rather than to `jacket`, and a model that answers in a phrase rather than a
    bare noun still gets read correctly.
    """
    lowered = name.strip().lower()
    if not lowered:
        return None
    matches = [known for known in GARMENT_SLEEVES if known in lowered]
    if not matches:
        return None
    return GARMENT_SLEEVES[max(matches, key=len)]


#: A three-way softmax sits at 0.33 knowing nothing, so this is "clearly better than a guess".
DEFAULT_CONFIDENCE_FLOOR = 0.50
DEFAULT_MARGIN_FLOOR = 0.12
#: Matches the power classifier: cosine similarities live in a narrow band and need scaling.
DEFAULT_LOGIT_SCALE = 50.0
DEFAULT_IMAGE_WEIGHT = 0.65
#: What to use when the reader will not commit. A t-shirt is the commonest drawn garment, and
#: getting a suit's forearms wrong is far more visible than a tee's.
FALLBACK_SLEEVE = "short"


class GarmentReading(StrictModel):
    """What the reader believes about the garment, and how much it believes it."""

    sleeve: str
    reach: float = Field(ge=0.0, le=1.0)
    confidence: float = Field(ge=0.0, le=1.0)
    runner_up: str
    unsure: bool
    scores: dict[str, float]
    used_sketch: bool
    used_description: bool

    def message(self) -> str:
        if self.unsure:
            return (f"Sleeves look {self.sleeve} (or {self.runner_up}?) "
                    f"- using reach {self.reach:.2f}")
        return f"{self.sleeve.capitalize()} sleeves - reach {self.reach:.2f}"


def _l2_normalise(matrix: np.ndarray) -> np.ndarray:
    return matrix / np.maximum(np.linalg.norm(matrix, axis=-1, keepdims=True), 1e-12)


def _softmax(logits: np.ndarray) -> np.ndarray:
    exponentiated = np.exp(logits - logits.max())
    return exponentiated / exponentiated.sum()


def sleeve_phrases(style: SleeveStyle) -> list[str]:
    """Every phrasing used to represent one sleeve length in the text encoder."""
    return [template.format(motif=motif) for motif in style.motifs for template in PROMPT_TEMPLATES]


@dataclass
class GarmentReader:
    """Scores a sketch against the sleeve-length prototypes.

    Prototypes are built once, so reading a drawing costs one image embedding.
    """

    embedder: Embedder
    styles: tuple[SleeveStyle, ...] = SLEEVES
    logit_scale: float = DEFAULT_LOGIT_SCALE
    image_weight: float = DEFAULT_IMAGE_WEIGHT
    confidence_floor: float = DEFAULT_CONFIDENCE_FLOOR
    margin_floor: float = DEFAULT_MARGIN_FLOOR

    def __post_init__(self) -> None:
        if not 0.0 <= self.image_weight <= 1.0:
            raise ValueError("image_weight must be in [0, 1]")
        self._prototypes = self._build_prototypes()

    def _build_prototypes(self) -> np.ndarray:
        phrases: list[str] = []
        spans: list[tuple[int, int]] = []
        for style in self.styles:
            start = len(phrases)
            phrases.extend(sleeve_phrases(style))
            spans.append((start, len(phrases)))
        embedded = _l2_normalise(np.asarray(self.embedder.embed_texts(phrases), dtype=np.float64))
        return _l2_normalise(
            np.stack([embedded[start:end].mean(axis=0) for start, end in spans])
        )

    def read(self, sketch_path: Path | None = None, description: str = "") -> GarmentReading:
        description = description.strip()
        if sketch_path is None and not description:
            raise ValueError("read needs a sketch, a description, or both")

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
        if weight <= 0.0:
            raise ValueError(
                f"image_weight={self.image_weight} gives the supplied inputs no weight at all"
            )

        probabilities = _softmax(self._prototypes @ query / weight * self.logit_scale)
        order = np.argsort(probabilities)[::-1]
        best, second = int(order[0]), int(order[1])
        top, runner_up = float(probabilities[best]), float(probabilities[second])
        unsure = top < self.confidence_floor or (top - runner_up) < self.margin_floor

        chosen = self.styles[best]
        if unsure:
            # Do not act on a coin flip. Fall back to the commonest garment rather than letting a
            # 34%-confident "long" put sleeves down to the monkey's hands.
            chosen = next(s for s in self.styles if s.id == FALLBACK_SLEEVE)

        return GarmentReading(
            sleeve=chosen.id,
            reach=chosen.reach,
            confidence=top,
            runner_up=self.styles[second].id,
            unsure=unsure,
            scores={style.id: float(p)
                    for style, p in zip(self.styles, probabilities, strict=True)},
            used_sketch=sketch_path is not None,
            used_description=bool(description),
        )


def resolve_sleeves(
    garment_name: str = "", reading: GarmentReading | None = None
) -> tuple[str, float, str]:
    """Settle on a sleeve length from everything available, and say where it came from.

    Order of preference, and it is the order of measured reliability:

    1. What the garment *is*, when a model named something recognisable. Both models name garments
       correctly and neither judges sleeve length correctly, so this is the strongest signal.
    2. CLIP's direct reading, but only when it was confident -- it was on the drawn suit (83%) and
       was not on the drawn t-shirt (36%).
    3. The fallback, because a wrong default on a t-shirt is far less visible than sleeves running
       to the hands on one.

    Returns `(sleeve_id, reach, source)`; the source is for telling a user why, not for control
    flow.
    """
    from_name = sleeves_from_garment(garment_name)
    if from_name is not None:
        return from_name, reach_for(from_name), f"garment name {garment_name.strip().lower()!r}"
    if reading is not None and not reading.unsure:
        return reading.sleeve, reading.reach, f"sketch ({reading.confidence:.0%} confident)"
    return FALLBACK_SLEEVE, reach_for(FALLBACK_SLEEVE), "fallback (nothing was confident)"


def reach_for(sleeve_id: str) -> float:
    """The arm fraction for a named sleeve length, for callers that already know the answer."""
    for style in SLEEVES:
        if style.id == sleeve_id:
            return style.reach
    raise KeyError(f"unknown sleeve length {sleeve_id!r}; expected one of "
                   f"{[s.id for s in SLEEVES]}")


def sleeve_ids() -> Sequence[str]:
    return [style.id for style in SLEEVES]
