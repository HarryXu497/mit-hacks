"""Read sleeve length off a drawing, and print the reach the projector needs.

    python scripts/read_garment.py input/mit_monkey.png
    python scripts/read_garment.py input/suit_monkey.png --description "a suit and tie"

Needs torch + transformers. CLIP ViT-B/32 runs on CPU in a couple of seconds.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from monkeyforge.garments import GarmentReader
from monkeyforge.powers.classify import DEFAULT_CLIP_MODEL, ClipEmbedder


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sketch", type=Path, nargs="?")
    parser.add_argument("--description", default="")
    parser.add_argument("--model", default=DEFAULT_CLIP_MODEL)
    parser.add_argument("--device", default=None)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    if args.sketch is None and not args.description:
        parser.error("give a sketch, a --description, or both")

    reader = GarmentReader(embedder=ClipEmbedder(args.model, args.device))
    reading = reader.read(sketch_path=args.sketch, description=args.description)

    if args.json:
        print(json.dumps(reading.model_dump(mode="json"), indent=2))
    else:
        print(reading.message())
        for sleeve, score in sorted(reading.scores.items(), key=lambda kv: -kv[1]):
            print(f"  {sleeve:12s} {score:6.1%}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
