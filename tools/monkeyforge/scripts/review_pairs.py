"""Build pairwise preference data from rendered candidates.

Two steps, deliberately dumb so labelling never blocks on tooling:

    plan   candidates manifest -> review CSV with an empty `choice` column
    build  filled-in CSV       -> training dataset JSONL

Open the two renders named in each row, type `a` or `b` in `choice`, save, then run `build`.

Candidate manifest is JSONL, one object per rendered candidate:

    {"id": "job/7b94/helmet", "prompt": "rounded goalkeeper helmet with a bold crest",
     "provenance": "generated", "render": "output/review/7b94.png",
     "features": "data/jobs/7b94/output/features.json"}
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from itertools import combinations
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY_ROOT / "src"))

from monkeyforge.features import image_features  # noqa: E402
from monkeyforge.models import Palette  # noqa: E402
from monkeyforge.ranking import (  # noqa: E402
    CandidateFeatures,
    PreferenceCandidate,
    PreferenceExample,
)

REVIEW_COLUMNS = [
    "pair_id",
    "prompt",
    "a_id",
    "a_render",
    "b_id",
    "b_render",
    "choice",
    "reasons",
]


def load_candidates(path: Path) -> list[dict]:
    candidates = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError(f"invalid candidate at {path}:{number}: {exc}") from exc
        for field in ("id", "prompt", "provenance", "features"):
            if field not in record:
                raise ValueError(f"candidate at {path}:{number} is missing '{field}'")
        candidates.append(record)
    if not candidates:
        raise ValueError(f"no candidates in {path}")
    return candidates


def plan(candidates_path: Path, output_path: Path, max_pairs_per_prompt: int) -> int:
    candidates = load_candidates(candidates_path)

    by_prompt: dict[str, list[dict]] = {}
    for candidate in candidates:
        by_prompt.setdefault(candidate["prompt"], []).append(candidate)

    rows = []
    for prompt in sorted(by_prompt):
        group = by_prompt[prompt]
        if len(group) < 2:
            print(f"skipping (needs 2+ candidates): {prompt!r} has {len(group)}")
            continue
        for left, right in list(combinations(group, 2))[:max_pairs_per_prompt]:
            rows.append(
                {
                    "pair_id": f"{left['id']}|{right['id']}",
                    "prompt": prompt,
                    "a_id": left["id"],
                    "a_render": left.get("render", ""),
                    "b_id": right["id"],
                    "b_render": right.get("render", ""),
                    "choice": "",
                    "reasons": "",
                }
            )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=REVIEW_COLUMNS)
        writer.writeheader()
        writer.writerows(rows)

    print(f"wrote {len(rows)} pairs to {output_path}")
    print("Open each a_render/b_render, put 'a' or 'b' in choice, then run: review_pairs.py build")
    return len(rows)


def build(candidates_path: Path, review_path: Path, output_path: Path) -> int:
    candidates = {candidate["id"]: candidate for candidate in load_candidates(candidates_path)}

    def resolve(value: str) -> Path:
        path = Path(value)
        return path if path.is_absolute() else REPOSITORY_ROOT / path

    def features_for(candidate_id: str) -> CandidateFeatures:
        candidate = candidates.get(candidate_id)
        if candidate is None:
            raise ValueError(f"unknown candidate id: {candidate_id}")
        features_path = resolve(candidate["features"])
        if not features_path.exists():
            raise ValueError(f"missing features for {candidate_id}: {features_path}")
        features = CandidateFeatures.model_validate_json(
            features_path.read_text(encoding="utf-8")
        )

        # The pipeline writes features.json before anything is rendered, so silhouette and palette
        # are still neutral placeholders there. Measure them now that a render exists.
        render = candidate.get("render")
        if render:
            render_path = resolve(render)
            if not render_path.exists():
                raise ValueError(f"missing render for {candidate_id}: {render_path}")
            features = features.model_copy(update=image_features(render_path, Palette()))

        return features

    examples: list[PreferenceExample] = []
    skipped = 0
    with review_path.open(encoding="utf-8", newline="") as handle:
        for number, row in enumerate(csv.DictReader(handle), start=2):
            choice = (row.get("choice") or "").strip().lower()
            if choice not in {"a", "b"}:
                skipped += 1
                continue
            preferred_id = row["a_id"] if choice == "a" else row["b_id"]
            rejected_id = row["b_id"] if choice == "a" else row["a_id"]
            reasons = [
                reason.strip()
                for reason in (row.get("reasons") or "").split(";")
                if reason.strip()
            ][:8]
            try:
                examples.append(
                    PreferenceExample(
                        prompt=row["prompt"],
                        preferred=PreferenceCandidate(
                            id=preferred_id,
                            provenance=candidates[preferred_id]["provenance"],
                            features=features_for(preferred_id),
                        ),
                        rejected=PreferenceCandidate(
                            id=rejected_id,
                            provenance=candidates[rejected_id]["provenance"],
                            features=features_for(rejected_id),
                        ),
                        reasons=reasons,
                    )
                )
            except (ValueError, KeyError) as exc:
                raise ValueError(f"{review_path}:{number}: {exc}") from exc

    if not examples:
        raise ValueError("no labelled pairs found; fill in the choice column first")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8") as handle:
        for example in examples:
            handle.write(example.model_dump_json() + "\n")

    print(f"wrote {len(examples)} labelled pairs to {output_path} ({skipped} unlabelled skipped)")
    return len(examples)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    plan_parser = subparsers.add_parser("plan", help="generate a review CSV from candidates")
    plan_parser.add_argument("--candidates", type=Path, default=Path("training/candidates.jsonl"))
    plan_parser.add_argument("--output", type=Path, default=Path("output/review/pairs.csv"))
    plan_parser.add_argument("--max-pairs-per-prompt", type=int, default=6)

    build_parser = subparsers.add_parser("build", help="turn a filled review CSV into a dataset")
    build_parser.add_argument("--candidates", type=Path, default=Path("training/candidates.jsonl"))
    build_parser.add_argument("--review", type=Path, default=Path("output/review/pairs.csv"))
    build_parser.add_argument("--output", type=Path, default=Path("training/dataset.jsonl"))

    args = parser.parse_args()
    if args.command == "plan":
        plan(args.candidates, args.output, args.max_pairs_per_prompt)
    else:
        build(args.candidates, args.review, args.output)


if __name__ == "__main__":
    main()
