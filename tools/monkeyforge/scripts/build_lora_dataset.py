"""Turn cropped reference cells into a subjects-only LoRA training set.

Step 5 of `docs/SUPERPOWER-ICONS.md` says to train on subjects, not full badges, and
that is the whole point of this script. The badge rim is supplied deterministically by
`powers.badge`; a LoRA that also learned the rim would fight it, and every failure
would show up as a warped ring rather than as a subject that can simply be regenerated.

So each cell has its badge stripped by salient-object segmentation, leaving the
character or prop on a flat background. Cells where segmentation clearly failed --
almost nothing kept, or almost everything -- are dropped rather than trained on.

Runs on the GPU box (needs torch + transformers + RMBG-2.0):

    python scripts/build_lora_dataset.py --cells output/lora/cells --output-dir output/lora/train

PRIVATE: the output is derived from Ninja Kiwi artwork. Training-only, never shipped.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image

from monkeyforge.powers.beautify import LORA_TRIGGER

# One consistent caption with a rare trigger token is the standard recipe for a style
# LoRA: the token carries the style, and describing each subject individually would
# teach the model those subjects instead.
#
# The *generation* prompt has to contain this same token or the adapter contributes
# nothing at inference -- which reads as "the LoRA didn't work" rather than "the LoRA
# was never asked". Imported from the one place inference also uses, so the two cannot
# drift apart.
TRIGGER = LORA_TRIGGER
CAPTION = (
    f"{TRIGGER}, a glossy cartoon game icon object, bold black outline, "
    "bright saturated colors, soft highlight from the upper left, plain background"
)

# Segmentation that keeps under 5% of the frame lost the subject; over 85% means it
# kept the badge too. Both produce training images that teach the wrong thing.
MIN_COVERAGE = 0.05
MAX_COVERAGE = 0.85


def load_rmbg():
    import torch
    from transformers import AutoModelForImageSegmentation

    device = "cuda" if torch.cuda.is_available() else "cpu"
    model = (
        AutoModelForImageSegmentation.from_pretrained("briaai/RMBG-2.0", trust_remote_code=True)
        .to(device)
        .eval()
    )
    return model, device, torch


def alpha_for(model, device, torch, image: Image.Image) -> np.ndarray:
    from torchvision import transforms

    tensor = (
        transforms.Compose(
            [
                transforms.Resize((1024, 1024)),
                transforms.ToTensor(),
                transforms.Normalize([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]),
            ]
        )(image.convert("RGB"))
        .unsqueeze(0)
        .to(device)
    )
    with torch.no_grad():
        mask = model(tensor)[-1].sigmoid().cpu()[0].squeeze().numpy()
    return np.asarray(Image.fromarray((mask * 255).astype(np.uint8)).resize(image.size))


def on_background(image: Image.Image, alpha: np.ndarray, size: int, pad: float) -> Image.Image:
    """Trim to the subject, pad to square, and flatten onto white."""
    rows = np.where(alpha.max(axis=1) > 16)[0]
    cols = np.where(alpha.max(axis=0) > 16)[0]
    cut = image.convert("RGBA")
    cut.putalpha(Image.fromarray(alpha))
    cut = cut.crop((cols[0], rows[0], cols[-1] + 1, rows[-1] + 1))

    side = int(max(cut.size) * (1 + pad))
    canvas = Image.new("RGBA", (side, side), (255, 255, 255, 255))
    canvas.alpha_composite(cut, ((side - cut.width) // 2, (side - cut.height) // 2))
    return canvas.convert("RGB").resize((size, size), Image.LANCZOS)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cells", type=Path, default=Path("output/lora/cells"))
    parser.add_argument("--output-dir", type=Path, default=Path("output/lora/train"))
    parser.add_argument("--size", type=int, default=1024)
    parser.add_argument("--pad", type=float, default=0.12)
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    model, device, torch = load_rmbg()
    print(f"RMBG-2.0 on {device}")

    kept, rejected = [], []
    for path in sorted(args.cells.glob("*.png")):
        if path.stem.startswith("_"):
            continue
        image = Image.open(path).convert("RGB")
        alpha = alpha_for(model, device, torch, image)
        coverage = float((alpha > 128).mean())
        if not MIN_COVERAGE <= coverage <= MAX_COVERAGE:
            rejected.append({"file": path.name, "coverage": round(coverage, 3)})
            continue
        on_background(image, alpha, args.size, args.pad).save(args.output_dir / path.name)
        kept.append({"file": path.name, "text": CAPTION, "coverage": round(coverage, 3)})

    # metadata.jsonl is what `datasets` imagefolder expects, so the training script can
    # read this directory directly.
    with (args.output_dir / "metadata.jsonl").open("w", encoding="utf-8") as handle:
        for row in kept:
            handle.write(json.dumps({"file_name": row["file"], "text": row["text"]}) + "\n")

    print(f"kept {len(kept)}, rejected {len(rejected)}")
    for row in rejected:
        print(f"  rejected {row['file']:34s} coverage {row['coverage']}")
    print(f"trigger token: {TRIGGER}")
    print("PRIVATE: reference-derived training data. Do not commit or ship.")


if __name__ == "__main__":
    main()
