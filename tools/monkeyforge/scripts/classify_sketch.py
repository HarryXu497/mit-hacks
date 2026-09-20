"""Classify one sketch into a runtime power, and optionally render its badge.

    python scripts/classify_sketch.py sketch.png --description "make me fast" --icon out.png

Needs torch + transformers (the `powers` extra). CLIP ViT-B/32 is small enough to run
on CPU in a couple of seconds; `--device cuda` uses the GPU when there is one.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from monkeyforge.powers.badge import BadgeStyle, placeholder_badge
from monkeyforge.powers.classify import DEFAULT_CLIP_MODEL, ClipEmbedder, PowerClassifier


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sketch", type=Path, nargs="?", help="PNG/JPEG of the drawing")
    parser.add_argument("--description", default="", help="what the player typed")
    parser.add_argument("--model", default=DEFAULT_CLIP_MODEL)
    parser.add_argument("--device", default=None, help="cuda, cpu; default picks automatically")
    parser.add_argument(
        "--icon",
        type=Path,
        default=None,
        help="write the placeholder badge for the chosen power here",
    )
    parser.add_argument("--json", action="store_true", help="print the full result as JSON")
    args = parser.parse_args()

    if args.sketch is None and not args.description:
        parser.error("give a sketch, a --description, or both")

    classifier = PowerClassifier(embedder=ClipEmbedder(args.model, args.device))
    guess = classifier.classify(sketch_path=args.sketch, description=args.description)

    if args.json:
        print(json.dumps(guess.model_dump(mode="json"), indent=2))
    else:
        print(guess.message())
        for power_id, score in sorted(guess.scores.items(), key=lambda kv: -kv[1]):
            print(f"  {power_id:12s} {score:6.1%}")

    if args.icon:
        args.icon.parent.mkdir(parents=True, exist_ok=True)
        placeholder_badge(guess.power, BadgeStyle()).save(args.icon)
        print(f"icon -> {args.icon}")


if __name__ == "__main__":
    main()
