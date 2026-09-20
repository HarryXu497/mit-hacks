"""Train the small MonkeyForge pairwise style ranker from JSONL preferences."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from monkeyforge.ranking import load_preferences, save_ranker, train_pairwise_ranker


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=400)
    parser.add_argument("--learning-rate", type=float, default=0.2)
    parser.add_argument("--l2", type=float, default=0.01)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    examples = load_preferences(args.dataset)
    artifact = train_pairwise_ranker(
        examples,
        epochs=args.epochs,
        learning_rate=args.learning_rate,
        l2=args.l2,
    )
    save_ranker(artifact, args.output)
    print(json.dumps(artifact.model_dump(), indent=2))


if __name__ == "__main__":
    main()
