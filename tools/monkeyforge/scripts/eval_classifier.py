"""Measure the zero-shot classifier against a labelled sketch directory.

Reads `expected.json` (as written by `draw_test_sketches.py`, or hand-written for real
drawings) and reports top-1 accuracy, top-2 accuracy, and how often the classifier
correctly admits it is unsure.

Top-2 matters as much as top-1 here: the product shows a runner-up, so a sketch whose
true power is the runner-up is a recoverable near-miss, not a failure.

    python scripts/eval_classifier.py --sketch-dir output/sketches --device cuda
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from monkeyforge.powers.classify import DEFAULT_CLIP_MODEL, ClipEmbedder, PowerClassifier


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sketch-dir", type=Path, default=Path("output/sketches"))
    parser.add_argument("--model", default=DEFAULT_CLIP_MODEL)
    parser.add_argument("--device", default=None)
    parser.add_argument("--report", type=Path, default=None, help="write the full result as JSON")
    parser.add_argument(
        "--with-description",
        action="store_true",
        help="also feed the motif name as the player's typed description",
    )
    args = parser.parse_args()

    entries = json.loads((args.sketch_dir / "expected.json").read_text(encoding="utf-8"))
    classifier = PowerClassifier(embedder=ClipEmbedder(args.model, args.device))

    rows = []
    for entry in entries:
        description = entry["motif"].replace("_", " ") if args.with_description else ""
        guess = classifier.classify(
            sketch_path=args.sketch_dir / entry["sketch"], description=description
        )
        rows.append(
            {
                "motif": entry["motif"],
                "expected": entry["expected"],
                "predicted": str(guess.power),
                "confidence": guess.confidence,
                "runner_up": str(guess.runner_up),
                "unsure": guess.unsure,
                "correct": str(guess.power) == entry["expected"],
                "in_top_2": entry["expected"] in {str(guess.power), str(guess.runner_up)},
                "scores": {str(k): v for k, v in guess.scores.items()},
            }
        )

    total = len(rows)
    top1 = sum(row["correct"] for row in rows)
    top2 = sum(row["in_top_2"] for row in rows)
    # A wrong answer the classifier flagged as unsure is a prompt, not a silent error.
    honest = sum(row["unsure"] for row in rows if not row["correct"])
    wrong = total - top1

    print(f"{'motif':13s} {'expected':11s} {'predicted':11s} {'conf':>6s} {'runner-up':11s}  ok")
    print("-" * 66)
    for row in rows:
        mark = "OK " if row["correct"] else ("?  " if row["unsure"] else "XX ")
        print(
            f"{row['motif']:13s} {row['expected']:11s} {row['predicted']:11s} "
            f"{row['confidence']:6.1%} {row['runner_up']:11s}  {mark}"
        )
    print("-" * 66)
    print(f"top-1 accuracy   {top1}/{total}  ({top1 / total:.0%})")
    print(f"top-2 accuracy   {top2}/{total}  ({top2 / total:.0%})")
    if wrong:
        print(f"wrong but flagged unsure  {honest}/{wrong}")
    print(f"flagged unsure overall    {sum(row['unsure'] for row in rows)}/{total}")

    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(
            json.dumps(
                {
                    "model": args.model,
                    "with_description": args.with_description,
                    "top_1": top1 / total,
                    "top_2": top2 / total,
                    "rows": rows,
                },
                indent=2,
            ),
            encoding="utf-8",
        )
        print(f"report -> {args.report}")


if __name__ == "__main__":
    main()
