"""Generate every sprite an outfit needs, in one model load.

Calling `gen_garment.py` once per item pays SDXL's 37-second load each time, which is three times
longer than the 12 seconds of actual generation. Loading once and looping is the single biggest
latency win available on the GPU side, and it costs nothing but this file.

The prompts are built from the spec the VLM produced, so what gets generated follows the drawing
rather than a hardcoded list.

Run (on the GX10):
    python scripts/gen_outfit.py --spec spec.json --out-dir output/run1
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

STYLE = (
    "isolated on a plain flat light grey background, flat lay, centred, "
    "single object, cartoon video game asset, clean bold outlines, cel shaded, "
    "soft even studio lighting, high detail, front view"
)

NEGATIVE = (
    "person, human, model wearing it, body, face, hands, mannequin, text, letters, words, "
    "logo, watermark, signature, multiple objects, two garments, several views, back view, "
    "turnaround, character sheet, reference sheet, side by side, cluttered background, "
    "photograph, realistic fabric photo, shadows across the background, cropped, out of frame"
)

#: Accessories are generated at three-quarter view because they go on to become 3D geometry, and
#: TRELLIS reconstructs far better from a view that shows depth than from a flat-on elevation.
ACCESSORY_STYLE = (
    "isolated on a plain flat light grey background, single object, three quarter view, "
    "cartoon video game asset, clean bold outlines, cel shaded, soft even studio lighting"
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--jobs", type=Path, required=True,
                        help="JSON list of {name, prompt, seed}. Prompts are built on the "
                             "orchestrating machine, where the garment vocabulary lives and is "
                             "tested; this box only executes them")
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=11)
    parser.add_argument("--steps", type=int, default=30)
    parser.add_argument("--guidance", type=float, default=7.0)
    parser.add_argument("--size", type=int, default=1024)
    parser.add_argument("--model", default="stabilityai/stable-diffusion-xl-base-1.0")
    args = parser.parse_args()

    jobs = json.loads(args.jobs.read_text(encoding="utf-8"))
    args.out_dir.mkdir(parents=True, exist_ok=True)
    print(f"OUTFIT {len(jobs)} sprite(s): {[job['name'] for job in jobs]}")

    import torch
    from diffusers import AutoencoderKL, StableDiffusionXLPipeline

    vae = AutoencoderKL.from_pretrained("madebyollin/sdxl-vae-fp16-fix", torch_dtype=torch.float16)
    pipe = StableDiffusionXLPipeline.from_pretrained(
        args.model, vae=vae, torch_dtype=torch.float16, variant="fp16", use_safetensors=True
    ).to("cuda")
    pipe.set_progress_bar_config(disable=True)
    print("OUTFIT pipeline ready")

    written: dict[str, str] = {}
    for job in jobs:
        name, subject, seed = job["name"], job["prompt"], int(job.get("seed", args.seed))
        style = ACCESSORY_STYLE if name.startswith("accessory_") else STYLE
        # Subject first: CLIP truncates at 77 tokens, and a long style suffix has previously
        # pushed the garment's own colour out of the window.
        image = pipe(
            prompt=f"{subject}, {style}",
            negative_prompt=NEGATIVE,
            num_inference_steps=args.steps,
            guidance_scale=args.guidance,
            width=args.size,
            height=args.size,
            generator=torch.Generator("cuda").manual_seed(seed),
        ).images[0]
        path = args.out_dir / f"{name}.png"
        image.save(path)
        written[name] = path.name
        print(f"OUTFIT wrote {path}")

    manifest = args.out_dir / "sprites.json"
    manifest.write_text(json.dumps(written, indent=2), encoding="utf-8")
    print(f"OUTFIT manifest {manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
