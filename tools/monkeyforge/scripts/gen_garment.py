"""Generate one isolated garment or accessory sprite with SDXL.

Generating the *item* rather than a dressed character is the thing that made this work. Asking a
diffusion model for "a monkey wearing an MIT jersey" returns a new monkey -- often not even a
monkey -- and throws away the base the game already has. Asking for the jersey alone returns
something that can be projected onto the base that already exists, which is the whole point: the
character is never regenerated, only dressed.

Lettering is deliberately NOT asked for here. Diffusion models cannot spell; ten attempts at "MIT"
produced "3I", "XJZ" and "VU". Text is stamped with a font downstream, in `project_planar.py`,
where it also gets warped with the cloth.

Run (on the GX10):
    python scripts/gen_garment.py --prompt "a maroon and white football jersey" \
        --output out.png --seed 11
"""

from __future__ import annotations

import argparse
from pathlib import Path

#: A flat lay on a plain ground is the easiest thing to cut out and the easiest thing to project:
#: no model wearing it, no perspective to fight, no background to segment away.
STYLE = (
    "isolated on a plain flat light grey background, flat lay, centred, "
    "single object, cartoon video game asset, clean bold outlines, cel shaded, "
    "soft even studio lighting, high detail, front view"
)

NEGATIVE = (
    "person, human, model wearing it, body, face, hands, mannequin, text, letters, words, "
    "logo, watermark, signature, multiple objects, cluttered background, photograph, "
    "realistic fabric photo, shadows across the background, cropped, out of frame"
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--prompt", required=True, help="the garment, e.g. 'a maroon polo shirt'")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=11)
    parser.add_argument("--steps", type=int, default=30)
    parser.add_argument("--guidance", type=float, default=7.0)
    parser.add_argument("--size", type=int, default=1024)
    parser.add_argument("--model", default="stabilityai/stable-diffusion-xl-base-1.0")
    args = parser.parse_args()

    import torch
    from diffusers import AutoencoderKL, StableDiffusionXLPipeline

    # The stock SDXL VAE produces black patches in fp16; this is the standard fix and costs
    # nothing. Omitting `variant` re-downloads the full-precision weights, which looks like a hang.
    vae = AutoencoderKL.from_pretrained("madebyollin/sdxl-vae-fp16-fix", torch_dtype=torch.float16)
    pipe = StableDiffusionXLPipeline.from_pretrained(
        args.model, vae=vae, torch_dtype=torch.float16, variant="fp16", use_safetensors=True
    ).to("cuda")
    pipe.set_progress_bar_config(disable=True)

    # CLIP truncates at 77 tokens, so the subject goes first: a style suffix that pushes the
    # garment out of the window is how an earlier run lost "maroon" entirely.
    prompt = f"{args.prompt}, {STYLE}"

    image = pipe(
        prompt=prompt,
        negative_prompt=NEGATIVE,
        num_inference_steps=args.steps,
        guidance_scale=args.guidance,
        width=args.size,
        height=args.size,
        generator=torch.Generator("cuda").manual_seed(args.seed),
    ).images[0]

    args.output.parent.mkdir(parents=True, exist_ok=True)
    image.save(args.output)
    print(f"wrote {args.output} (seed {args.seed})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
