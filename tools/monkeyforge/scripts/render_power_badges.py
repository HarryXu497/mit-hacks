"""Render the four placeholder power badges and a contact sheet.

Step 3 of `docs/SUPERPOWER-ICONS.md`: prove the compositing path with a flat
placeholder before any model is involved. No GPU, no weights, no network.

    .\\.venv\\Scripts\\python.exe scripts\\render_power_badges.py --output-dir output\\powers
"""

from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image

from monkeyforge.powers import POWERS
from monkeyforge.powers.badge import BadgeStyle, placeholder_badge


def contact_sheet(images: list[Image.Image], pad: int = 24) -> Image.Image:
    size = images[0].width
    width = size * len(images) + pad * (len(images) + 1)
    sheet = Image.new("RGBA", (width, size + pad * 2), (245, 246, 248, 255))
    for index, image in enumerate(images):
        sheet.paste(image, (pad + index * (size + pad), pad), image)
    return sheet


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=Path("output/powers"))
    parser.add_argument("--size", type=int, default=512)
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    style = BadgeStyle(size=args.size)

    images = []
    for power in POWERS:
        badge = placeholder_badge(power.id, style)
        destination = args.output_dir / f"{power.id}.png"
        badge.save(destination)
        images.append(badge)
        print(f"{power.display_name:12s} -> {destination}")

    sheet_path = args.output_dir / "contact_sheet.png"
    contact_sheet(images).save(sheet_path)
    print(f"{'sheet':12s} -> {sheet_path}")


if __name__ == "__main__":
    main()
