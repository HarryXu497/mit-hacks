"""Render the same sketches at several LoRA strengths, through the real pipeline.

A LoRA trained on 65 images can easily be too strong -- it starts imposing reference
subjects instead of style. A strength ladder against an identical base run, same seed
and same sketches, is the only honest way to pick a value.

This drives `IconPipeline`, classifier included, so what it renders is exactly what a
player would get. Scale 0.0 still has the LoRA loaded, so the rungs differ only in
strength.

    python scripts/compare_lora.py --lora output/lora/btd6icon --scales 0 0.7 1.0
"""

from __future__ import annotations

import argparse
import time
from pathlib import Path

from monkeyforge.powers.badge import BadgeStyle
from monkeyforge.powers.beautify import BeautifySettings, SdxlControlNetGenerator
from monkeyforge.powers.classify import DEFAULT_CLIP_MODEL, ClipEmbedder, PowerClassifier
from monkeyforge.powers.pipeline import IconPipeline

SKETCH_SUFFIXES = {".png", ".jpg", ".jpeg", ".webp"}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sketch-dir", type=Path, default=Path("output/sketches"))
    parser.add_argument("--output-dir", type=Path, default=Path("output/lora_compare"))
    parser.add_argument("--lora", type=Path, required=True)
    parser.add_argument("--scales", nargs="+", type=float, default=[0.0, 0.7, 1.0])
    parser.add_argument("--motifs", nargs="*", default=None, help="sketch stems; default all")
    parser.add_argument("--size", type=int, default=384)
    parser.add_argument("--seed", type=int, default=1234)
    args = parser.parse_args()

    sketches = sorted(
        p
        for p in args.sketch_dir.iterdir()
        if p.suffix.lower() in SKETCH_SUFFIXES and not p.stem.startswith("_")
    )
    if args.motifs:
        sketches = [p for p in sketches if p.stem in set(args.motifs)]
    if not sketches:
        raise SystemExit(f"no matching sketches in {args.sketch_dir}")

    generator = SdxlControlNetGenerator(lora_path=args.lora)
    pipeline = IconPipeline(
        classifier=PowerClassifier(embedder=ClipEmbedder(DEFAULT_CLIP_MODEL)),
        generator=generator,
        badge_style=BadgeStyle(size=args.size),
        settings=BeautifySettings(seed=args.seed),
    )
    args.output_dir.mkdir(parents=True, exist_ok=True)
    print(f"loaded LoRA from {args.lora}")

    for scale in args.scales:
        generator.lora_scale = scale
        for path in sketches:
            start = time.time()
            result = pipeline.run(sketch_path=path)
            result.save(args.output_dir / f"{path.stem}__lora{scale:.1f}.png")
            print(
                f"  scale {scale:.1f}  {path.stem:13s} {time.time() - start:5.1f}s  "
                f"{result.guess.power} / {result.guess.motif}"
            )

    print(f"\nicons -> {args.output_dir}")


if __name__ == "__main__":
    main()
