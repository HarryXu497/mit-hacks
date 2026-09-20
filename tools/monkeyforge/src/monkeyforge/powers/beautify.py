"""Turn a rough drawing into a clean icon subject, keeping the player's shape.

The generator only ever makes the *subject*. The badge frame is drawn deterministically
by `powers.badge`, so a bad generation degrades to a poor subject in a correct badge
rather than to a warped rim.

The prompt recipe below is not guesswork; it is the surviving result of a measured
sweep on the GX10, and each clause is there because removing it broke something:

* **The composition clause goes first.** "a single ... one isolated object centered on
  a plain background" at the front produced one object every time; the same words at
  the back let SDXL render a collage of little game assets instead. Early tokens
  dominate.
* **Colour comes from the drawn thing, not from adjectives and not from the power.**
  Asking for "vivid saturated colours" turned whole frames into rainbow noise, while
  saying nothing produced grey extruded plastic. Naming a palette fixes both -- but it
  has to be the *motif's* palette, so a spring that maps to Boost renders as steel
  rather than Boost-gold. Colouring by power instead homogenises every subject in a
  power into one look, which defeats the point of letting a player draw. The power
  shows up in the badge backdrop, never in the subject.
* **Volume is requested explicitly and hollowness is named in the negative.** Without
  that, ControlNet's line conditioning came back as a re-tinted pencil outline rather
  than a rendered object.
* **Everything fits in 77 tokens.** CLIP truncates silently, and an over-long negative
  prompt drops exactly the terms at the end that were doing the work.

What the prompt still cannot supply is BTD6's specific look -- SDXL never learned it.
That is the gap the style LoRA is for; see `scripts/build_lora_dataset.py`.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

from PIL import Image

from monkeyforge.powers.badge import BadgeStyle, placeholder_subject
from monkeyforge.powers.registry import Motif, Power
from monkeyforge.powers.sketchmask import apply_mask, line_art, subject_mask

PROMPT_TEMPLATE = (
    "a single solid {palette} {subject}, one isolated object centered on a plain "
    "background, glossy cartoon game icon, thick black outline, smooth rounded 3d "
    "volume, shiny plastic surface, soft highlight"
)

NEGATIVE_PROMPT = (
    "line art, outline only, hollow, unfilled, wireframe, thin lines, sketch, "
    "flat, grey, desaturated, pattern, collage, many objects, cluttered, "
    "rainbow, photo, realistic, text, watermark, frame, border, blurry"
)

SDXL_MODEL = "stabilityai/stable-diffusion-xl-base-1.0"
CONTROLNET_MODEL = "diffusers/controlnet-canny-sdxl-1.0"
# The stock SDXL VAE produces NaNs in fp16; this is the standard drop-in fix.
VAE_MODEL = "madebyollin/sdxl-vae-fp16-fix"


@dataclass(frozen=True)
class BeautifySettings:
    """Sampler settings. Defaults are the sweep's best, not library defaults."""

    # 0.8 reproduced the strokes so literally that subjects stayed flat outlines; 0.65
    # keeps the silhouette recognisable while leaving room to render volume.
    control_scale: float = 0.65
    guidance: float = 8.0
    steps: int = 28
    # SDXL degrades below its native 1024 training resolution.
    resolution: int = 1024
    seed: int = 0


# The style LoRA is trained with every caption prefixed by this token. Without it in
# the prompt the adapter has nothing to activate on and contributes almost nothing --
# which looks exactly like "the LoRA didn't work" rather than "the LoRA wasn't asked".
# Must stay in step with `CAPTION` in scripts/build_lora_dataset.py.
LORA_TRIGGER = "btd6icon style"


def build_prompt(motif: Motif, trigger: str = "") -> str:
    """The subject prompt. Takes no power: the power must not colour the artwork.

    `trigger` is prepended only when a style LoRA is loaded.
    """
    prompt = PROMPT_TEMPLATE.format(palette=motif.palette, subject=motif.phrase)
    return f"{trigger}, {prompt}" if trigger else prompt


class SubjectGenerator(Protocol):
    """Anything that turns a drawing into a cut-out subject on transparency.

    `power` is passed for the placeholder fallback's glyph only. A real generator
    renders `motif` and should ignore it.
    """

    def generate(
        self, sketch_path: Path, power: Power, motif: Motif, settings: BeautifySettings
    ) -> Image.Image: ...


class PlaceholderGenerator:
    """The no-GPU generator: the power's flat glyph, ignoring the drawing.

    Keeps the whole pipeline runnable and testable on a laptop, and doubles as the
    fallback when the GPU worker is unreachable -- a plain glyph in a correct badge is
    a worse icon but never a broken one.
    """

    name = "placeholder"

    def generate(
        self, sketch_path: Path, power: Power, motif: Motif, settings: BeautifySettings
    ) -> Image.Image:
        return placeholder_subject(power.id, size=BadgeStyle().size)


class SdxlControlNetGenerator:
    """SDXL + canny ControlNet, conditioned on the player's line art.

    torch and diffusers are imported in `__init__` so importing this module stays free
    on a machine that only uses the placeholder path. Loading the pipeline takes tens
    of seconds and about 8 GB, so construct it once and keep it.
    """

    name = "sdxl-controlnet"

    def __init__(
        self,
        device: str = "cuda",
        lora_path: str | Path | None = None,
        lora_scale: float = 1.0,
        lora_trigger: str = LORA_TRIGGER,
    ) -> None:
        import torch
        from diffusers import (
            AutoencoderKL,
            ControlNetModel,
            StableDiffusionXLControlNetPipeline,
        )

        self._torch = torch
        self.device = device
        dtype = torch.float16 if device == "cuda" else torch.float32

        # `variant="fp16"` matters: without it diffusers ignores already-downloaded
        # fp16 shards and silently re-fetches full-precision weights, which looks
        # exactly like a hang.
        controlnet = ControlNetModel.from_pretrained(
            CONTROLNET_MODEL, torch_dtype=dtype, variant="fp16"
        )
        vae = AutoencoderKL.from_pretrained(VAE_MODEL, torch_dtype=dtype)
        self._pipe = StableDiffusionXLControlNetPipeline.from_pretrained(
            SDXL_MODEL,
            controlnet=controlnet,
            vae=vae,
            torch_dtype=dtype,
            variant="fp16",
            use_safetensors=True,
        ).to(device)
        self._pipe.set_progress_bar_config(disable=True)

        self.lora_path = str(lora_path) if lora_path else None
        self.lora_scale = lora_scale
        self.lora_trigger = lora_trigger if lora_path else ""
        if self.lora_path:
            self._pipe.load_lora_weights(self.lora_path)

    def generate(
        self, sketch_path: Path, power: Power, motif: Motif, settings: BeautifySettings
    ) -> Image.Image:
        sketch = Image.open(sketch_path)
        control = line_art(sketch, size=settings.resolution)

        generator = self._torch.Generator(self.device).manual_seed(settings.seed)
        # Only pass the scale when a LoRA is loaded: without one, diffusers warns that
        # the cross-attention kwarg has nothing to apply to.
        extra = {"cross_attention_kwargs": {"scale": self.lora_scale}} if self.lora_path else {}
        raw = self._pipe(
            prompt=build_prompt(motif, self.lora_trigger),
            negative_prompt=NEGATIVE_PROMPT,
            image=control,
            num_inference_steps=settings.steps,
            guidance_scale=settings.guidance,
            controlnet_conditioning_scale=settings.control_scale,
            generator=generator,
            **extra,
        ).images[0]

        return apply_mask(raw, subject_mask(sketch, size=settings.resolution))
