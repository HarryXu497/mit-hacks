"""Serve the draw-a-superpower demo page.

    # laptop, no GPU (placeholder subjects)
    python scripts/serve_power_icons.py --generator placeholder

    # GPU box, reachable from the laptop
    python scripts/serve_power_icons.py --generator sdxl --host 0.0.0.0 --port 8610

Port 8610 by default, not 8600: the MonkeyForge character worker owns 8600 and the
brief asks any second service to pick its own port.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import uvicorn

from monkeyforge.powers.badge import BadgeStyle
from monkeyforge.powers.beautify import BeautifySettings, PlaceholderGenerator
from monkeyforge.powers.classify import DEFAULT_CLIP_MODEL, ClipEmbedder, PowerClassifier
from monkeyforge.powers.demo import create_app
from monkeyforge.powers.pipeline import IconPipeline


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--generator", choices=["placeholder", "sdxl"], default="placeholder")
    parser.add_argument("--lora", type=Path, default=None)
    # 1.0 over-applies a LoRA this small: it starts overriding the per-motif palettes
    # and washing subjects toward the reference set's own colours.
    parser.add_argument("--lora-scale", type=float, default=0.6)
    parser.add_argument("--clip-model", default=DEFAULT_CLIP_MODEL)
    parser.add_argument("--device", default=None)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8610)
    parser.add_argument("--size", type=int, default=512)
    parser.add_argument("--steps", type=int, default=BeautifySettings.steps)
    args = parser.parse_args()

    if args.generator == "placeholder":
        generator = PlaceholderGenerator()
    else:
        from monkeyforge.powers.beautify import SdxlControlNetGenerator

        generator = SdxlControlNetGenerator(
            device=args.device or "cuda", lora_path=args.lora, lora_scale=args.lora_scale
        )

    # Both models load before the port opens, so a reachable server is a working one.
    print(f"loading CLIP ({args.clip_model})…")
    pipeline = IconPipeline(
        classifier=PowerClassifier(embedder=ClipEmbedder(args.clip_model, args.device)),
        generator=generator,
        badge_style=BadgeStyle(size=args.size),
        settings=BeautifySettings(steps=args.steps),
    )

    name = getattr(generator, "name", type(generator).__name__)
    if args.lora:
        name = f"{name}+lora@{args.lora_scale:g}"
    print(f"ready: http://{args.host}:{args.port}/  (generator: {name})")
    uvicorn.run(create_app(pipeline, name), host=args.host, port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
