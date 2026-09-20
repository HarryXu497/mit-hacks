"""Grid-crop the BTD6 reference sheets into one file per icon, for LoRA training.

Step 4 of `docs/SUPERPOWER-ICONS.md`. The sheets are regular grids, so a declarative
grid spec plus a blank-cell filter is the whole job; there is nothing to detect.

Output is a **private, local-only** dataset. These crops are Ninja Kiwi's artwork:
they are style reference for training and must not reach the public repository, the
released game, or any shipped asset. `output/` is git-ignored, which is why the
dataset is written there rather than under `assets/`.

    .\\.venv\\Scripts\\python.exe scripts\\crop_icon_sheets.py --output-dir output\\lora\\cells
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image

REFERENCE_DIR = Path("assets/reference/btd6_icons")


@dataclass(frozen=True)
class SheetSpec:
    """Where the icons sit in one sheet. `bottom` excludes non-icon chrome."""

    filename: str
    columns: int
    rows: int
    top: int = 0
    bottom: int | None = None
    inset: float = 0.03

    def cells(self, width: int, height: int):
        bottom = self.bottom if self.bottom is not None else height
        cell_w = width / self.columns
        cell_h = (bottom - self.top) / self.rows
        pad_x, pad_y = cell_w * self.inset, cell_h * self.inset
        for row in range(self.rows):
            for column in range(self.columns):
                left = column * cell_w + pad_x
                top = self.top + row * cell_h + pad_y
                yield row, column, (
                    int(round(left)),
                    int(round(top)),
                    int(round(left + cell_w - 2 * pad_x)),
                    int(round(top + cell_h - 2 * pad_y)),
                )


_PREFIX = "i-made-little-icon-for-all-tower-and-other-stuff-since-i-v0-"

SHEETS: tuple[SheetSpec, ...] = (
    SheetSpec(f"{_PREFIX}zynzebvtr14a1.webp", 6, 6),
    # The last ~105px of this one is a caption, not icons.
    SheetSpec(f"{_PREFIX}l68ih45ur14a1.webp", 6, 5, bottom=535),
    # gprragfg0xi91.webp is deliberately excluded: it is a phone screenshot of a
    # sticker pack, so it carries UI chrome, partial rows at both ends, a dark
    # background, and a white keyline style that is not the badge style being learned.
)


def is_blank(cell: Image.Image, tolerance: int = 12) -> bool:
    """True for a cell that is effectively one flat colour, i.e. an empty grid slot."""
    pixels = np.asarray(cell.convert("RGB"), dtype=np.int16)
    return bool(pixels.reshape(-1, 3).std(axis=0).max() < tolerance)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=Path, default=REFERENCE_DIR)
    parser.add_argument("--output-dir", type=Path, default=Path("output/lora/cells"))
    parser.add_argument("--size", type=int, default=512)
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    manifest, skipped = [], 0

    for spec in SHEETS:
        source = args.reference_dir / spec.filename
        if not source.exists():
            raise SystemExit(f"missing reference sheet: {source}")
        sheet = Image.open(source).convert("RGB")
        stem = source.stem[-12:]

        for row, column, box in spec.cells(*sheet.size):
            cell = sheet.crop(box)
            if is_blank(cell):
                skipped += 1
                continue
            name = f"{stem}_r{row}c{column}.png"
            cell.resize((args.size, args.size), Image.LANCZOS).save(args.output_dir / name)
            manifest.append({"file": name, "sheet": spec.filename, "row": row, "column": column})

        print(f"{spec.filename[-24:]:26s} {spec.columns}x{spec.rows}")

    index = args.output_dir / "cells.json"
    index.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"\n{len(manifest)} cells written to {args.output_dir} ({skipped} blank skipped)")
    print(f"manifest -> {index}")
    print("\nPRIVATE: reference art. Do not commit, publish, or ship these crops.")


if __name__ == "__main__":
    main()
