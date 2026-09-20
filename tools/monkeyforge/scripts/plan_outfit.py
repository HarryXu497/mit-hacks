"""Turn a garment spec into the exact sprite jobs to generate. No models, no GPU, no torch.

The resident daemon on the GPU box executes prompts; it does not write them. The wording lives
here because this is where the garment vocabulary and its tests live -- asked for the bare word a
vision model returns, SDXL drew an armoured spacesuit for a business suit, and the table that
fixes that is only trustworthy next to the tests that pin it.

Kept free of torch deliberately: the server calls this on every character, and paying a torch
import (several seconds) to decide a sentence would be absurd.

Run:
    python scripts/plan_outfit.py --spec spec.json [--seed 11]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from monkeyforge.garments import garment_prompt, resolve_sleeves  # noqa: E402

#: Things worth their own sprite because they will become geometry. A tie is absent on purpose:
#: it lies flat against the chest and is drawn into the garment image instead.
GEOMETRY_WORDS = ("cap", "hat", "helmet", "crown", "glasses", "goggles", "visor", "mask",
                  "backpack", "cape", "headband")

TROUSERS_PROMPT = ("a pair of formal dress trousers, two trouser legs only, waistband at top, "
                   "no jacket no shirt no sleeves, laid flat")


def plan(spec: dict, seed: int) -> dict:
    jobs = [{"name": "garment", "prompt": garment_prompt(spec), "seed": seed}]
    if spec.get("trousers"):
        jobs.append({"name": "trousers", "prompt": TROUSERS_PROMPT, "seed": seed + 3})
    for item in spec.get("accessories", []):
        word = str(item).strip().lower()
        if any(key in word for key in GEOMETRY_WORDS):
            jobs.append({
                "name": f"accessory_{word.replace(' ', '_')}",
                "prompt": f"one single {word}, only one {word}, three quarter view",
                "seed": seed + 7,
            })

    sleeve, reach, source = resolve_sleeves(str(spec.get("garment") or ""))
    return {
        "jobs": jobs,
        "sleeve": sleeve,
        "reach": reach,
        "sleeve_source": source,
        # What the player should be told is being made, in their words rather than a prompt.
        "summary": describe(spec),
    }


def describe(spec: dict) -> str:
    """A short human sentence for the waiting UI, so progress can name the thing being built."""
    parts = [str(spec.get("garment") or "outfit").strip()]
    if spec.get("text"):
        parts.append(f"reading {spec['text']}")
    if spec.get("trousers"):
        parts.append("with trousers")
    extras = [str(item) for item in spec.get("accessories", [])]
    if extras:
        parts.append("and " + ", ".join(extras))
    return " ".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=11)
    args = parser.parse_args()

    spec = json.loads(args.spec.read_text(encoding="utf-8"))
    print(json.dumps(plan(spec, args.seed), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
