"""End-to-end: sketches in, classified and beautified badge PNGs out.

    # laptop, no GPU: placeholder subjects, real classification
    python scripts/build_power_icons.py --sketch-dir output/sketches --generator placeholder

    # GPU box: SDXL + ControlNet subjects
    python scripts/build_power_icons.py --sketch-dir output/sketches --generator sdxl

Every sketch in the directory is processed. If the directory has an `expected.json`
the run also reports classification accuracy, so a change to prompts or settings can
be judged on both axes at once rather than by eye alone.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from monkeyforge.powers.badge import BadgeStyle
from monkeyforge.powers.beautify import BeautifySettings, PlaceholderGenerator
from monkeyforge.powers.classify import DEFAULT_CLIP_MODEL, ClipEmbedder, PowerClassifier
from monkeyforge.powers.pipeline import IconPipeline

SKETCH_SUFFIXES = {".png", ".jpg", ".jpeg", ".webp"}


def make_generator(kind: str, device: str | None, lora: Path | None):
    if kind == "placeholder":
        return PlaceholderGenerator()
    from monkeyforge.powers.beautify import SdxlControlNetGenerator

    return SdxlControlNetGenerator(device=device or "cuda", lora_path=lora)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sketch-dir", type=Path, default=Path("output/sketches"))
    parser.add_argument("--output-dir", type=Path, default=Path("output/icons"))
    parser.add_argument("--generator", choices=["placeholder", "sdxl"], default="placeholder")
    parser.add_argument("--lora", type=Path, default=None, help="LoRA weights for the subject")
    parser.add_argument("--clip-model", default=DEFAULT_CLIP_MODEL)
    parser.add_argument("--device", default=None)
    parser.add_argument("--size", type=int, default=512)
    parser.add_argument("--seed", type=int, default=1234)
    parser.add_argument("--control-scale", type=float, default=BeautifySettings.control_scale)
    parser.add_argument("--guidance", type=float, default=BeautifySettings.guidance)
    parser.add_argument("--steps", type=int, default=BeautifySettings.steps)
    args = parser.parse_args()

    sketches = sorted(
        p
        for p in args.sketch_dir.iterdir()
        if p.suffix.lower() in SKETCH_SUFFIXES and not p.stem.startswith("_")
    )
    if not sketches:
        raise SystemExit(f"no sketches in {args.sketch_dir}")

    expected_path = args.sketch_dir / "expected.json"
    expected = (
        {e["sketch"]: e["expected"] for e in json.loads(expected_path.read_text())}
        if expected_path.exists()
        else {}
    )

    pipeline = IconPipeline(
        classifier=PowerClassifier(embedder=ClipEmbedder(args.clip_model, args.device)),
        generator=make_generator(args.generator, args.device, args.lora),
        badge_style=BadgeStyle(size=args.size),
        settings=BeautifySettings(
            control_scale=args.control_scale,
            guidance=args.guidance,
            steps=args.steps,
            seed=args.seed,
        ),
    )

    args.output_dir.mkdir(parents=True, exist_ok=True)
    correct = 0
    for path in sketches:
        result = pipeline.run(sketch_path=path)
        result.save(args.output_dir / f"{path.stem}.png")
        result.subject.save(args.output_dir / f"{path.stem}_subject.png")

        truth = expected.get(path.name)
        mark = ""
        if truth:
            hit = str(result.guess.power) == truth
            correct += hit
            mark = "  OK" if hit else f"  XX (wanted {truth})"
        print(
            f"{path.stem:13s} {str(result.guess.power):11s} {result.guess.confidence:5.1%} "
            f"[{result.guess.motif}]{mark}"
        )

    if expected:
        print(f"\nclassification: {correct}/{len(sketches)} ({correct / len(sketches):.0%})")
    print(f"icons -> {args.output_dir}")


if __name__ == "__main__":
    main()
