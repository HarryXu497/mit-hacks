from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Literal

from pydantic import Field

from monkeyforge.models import StrictModel

FEATURE_NAMES = (
    "semantic_similarity",
    "silhouette_score",
    "palette_score",
    "socket_fit_score",
    "triangle_budget_score",
    "disconnected_penalty",
)


class CandidateFeatures(StrictModel):
    semantic_similarity: float = Field(ge=0, le=1)
    silhouette_score: float = Field(ge=0, le=1)
    palette_score: float = Field(ge=0, le=1)
    socket_fit_score: float = Field(ge=0, le=1)
    triangle_budget_score: float = Field(ge=0, le=1)
    disconnected_penalty: float = Field(ge=0, le=1)

    def vector(self) -> list[float]:
        return [getattr(self, name) for name in FEATURE_NAMES]


class PreferenceCandidate(StrictModel):
    id: str = Field(min_length=1, max_length=120)
    provenance: Literal["team_owned", "generated", "procedural"]
    features: CandidateFeatures


class PreferenceExample(StrictModel):
    prompt: str = Field(min_length=3, max_length=500)
    preferred: PreferenceCandidate
    rejected: PreferenceCandidate
    reasons: list[str] = Field(default_factory=list, max_length=8)


class RankerArtifact(StrictModel):
    schema_version: str = "1.0"
    feature_names: list[str]
    weights: list[float]
    example_count: int
    epochs: int
    final_loss: float
    pairwise_accuracy: float

    def score(self, features: CandidateFeatures) -> float:
        return sum(
            weight * value
            for weight, value in zip(self.weights, features.vector(), strict=True)
        )


def load_preferences(path: Path) -> list[PreferenceExample]:
    examples = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        try:
            examples.append(PreferenceExample.model_validate_json(line))
        except Exception as exc:
            raise ValueError(f"invalid preference at {path}:{line_number}: {exc}") from exc
    if not examples:
        raise ValueError("preference dataset is empty")
    return examples


def _difference(example: PreferenceExample) -> list[float]:
    preferred = example.preferred.features.vector()
    rejected = example.rejected.features.vector()
    return [left - right for left, right in zip(preferred, rejected, strict=True)]


def _sigmoid(value: float) -> float:
    if value >= 0:
        inverse = math.exp(-value)
        return 1 / (1 + inverse)
    exponential = math.exp(value)
    return exponential / (1 + exponential)


def train_pairwise_ranker(
    examples: list[PreferenceExample],
    epochs: int = 400,
    learning_rate: float = 0.2,
    l2: float = 0.01,
) -> RankerArtifact:
    if not examples:
        raise ValueError("at least one preference example is required")
    weights = [0.0] * len(FEATURE_NAMES)
    differences = [_difference(example) for example in examples]
    final_loss = 0.0

    for _epoch in range(epochs):
        gradients = [l2 * weight for weight in weights]
        final_loss = 0.0
        for difference in differences:
            margin = sum(
                weight * value for weight, value in zip(weights, difference, strict=True)
            )
            probability = _sigmoid(margin)
            final_loss += -math.log(max(probability, 1e-9))
            for index, value in enumerate(difference):
                gradients[index] += (probability - 1.0) * value
        scale = 1 / len(differences)
        weights = [
            weight - learning_rate * gradient * scale
            for weight, gradient in zip(weights, gradients, strict=True)
        ]

    correct = sum(
        sum(weight * value for weight, value in zip(weights, difference, strict=True)) > 0
        for difference in differences
    )
    return RankerArtifact(
        feature_names=list(FEATURE_NAMES),
        weights=weights,
        example_count=len(examples),
        epochs=epochs,
        final_loss=final_loss / len(examples),
        pairwise_accuracy=correct / len(examples),
    )


def save_ranker(artifact: RankerArtifact, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(artifact.model_dump(), indent=2), encoding="utf-8")
