"""A resident forge on the GPU box: models stay loaded between requests.

The character pipeline used to pay a model load on every request. Measured on the GX10, a
character cost ~138 s, of which ~45 s was nothing but loading Qwen2-VL and SDXL off disk and
letting CUDA JIT its kernels -- work that is identical every time and thrown away immediately.

This holds both models in memory and answers over HTTP, so that cost is paid once at startup
rather than once per player. It deliberately does *not* hold TRELLIS: that model is 4B, is only
needed when a drawing has an accessory, and would double the daemon's resident footprint for a
path most characters never take. `worker/server.py` already serves it separately.

Prompt wording is NOT decided here. The orchestrating machine owns the garment vocabulary and the
tests for it, and this daemon executes whatever jobs it is handed -- otherwise the wording that
turns "suit" into a business suit rather than a spacesuit would live on a box with no test suite.

Run:
    source ~/trellis-env/bin/activate
    HF_HUB_DISABLE_XET=1 uvicorn worker.forge_daemon:app --host 0.0.0.0 --port 8601
"""

from __future__ import annotations

import base64
import io
import logging
import os
import re
import time
from typing import Any

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

LOGGER = logging.getLogger("monkeyforge.forge_daemon")
logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")

VLM_MODEL = os.environ.get("MONKEYFORGE_VLM", "Qwen/Qwen2-VL-2B-Instruct")
SDXL_MODEL = os.environ.get("MONKEYFORGE_SDXL", "stabilityai/stable-diffusion-xl-base-1.0")

STYLE = (
    "isolated on a plain flat light grey background, flat lay, centred, "
    "single object, cartoon video game asset, clean bold outlines, cel shaded, "
    "soft even studio lighting, high detail, front view"
)
ACCESSORY_STYLE = (
    "isolated on a plain flat light grey background, single object, three quarter view, "
    "cartoon video game asset, clean bold outlines, cel shaded, soft even studio lighting"
)
NEGATIVE = (
    "person, human, model wearing it, body, face, hands, mannequin, text, letters, words, "
    "logo, watermark, signature, multiple objects, two garments, several views, back view, "
    "turnaround, character sheet, reference sheet, side by side, cluttered background, "
    "photograph, realistic fabric photo, shadows across the background, cropped, out of frame"
)

#: One focused question per field. Asked for a whole JSON schema at once, a 2B model slides
#: answers between fields -- it once answered "short" into `garment` and left `sleeves` as
#: "false". The information was there; the formatting destroyed it.
QUESTIONS: dict[str, str] = {
    "garment": (
        "What kind of clothing is this character wearing on its body? "
        "Answer with just the garment name, for example: t-shirt, suit jacket, hoodie, vest."
    ),
    "sleeves": (
        "Look at the character's arms. How far down the arms does the clothing go? "
        "Answer with EXACTLY ONE of these words and nothing else: sleeveless, short, long."
    ),
    "text": (
        "Are there any letters or words written on the character's clothing? "
        "Answer with just the letters you can see, or the word NONE."
    ),
    "trousers": (
        "Is the character wearing trousers or long pants covering its legs? "
        "Answer with exactly one word: yes or no."
    ),
    "accessories": (
        "What is the character wearing that sticks out from its body, such as a hat, cap, "
        "sunglasses, or a tie? List them separated by commas, or answer NONE."
    ),
}

SLEEVES = ("sleeveless", "short", "long")
NEGATIVES = frozenset({"NONE", "NO", "N/A", "NA", "NOTHING", "NIL", "", "-"})

app = FastAPI(title="MonkeyForge resident forge", version="1.0.0")

STATE: dict[str, Any] = {
    "ready": False,
    "loading": None,
    "warm_seconds": None,
    "generations": 0,
    "error": None,
}


class ReadRequest(BaseModel):
    sketch: str = Field(..., description="base64 PNG of the drawing")
    max_tokens: int = 40


class SpriteJob(BaseModel):
    name: str
    prompt: str
    seed: int = 11


class SpritesRequest(BaseModel):
    jobs: list[SpriteJob]
    steps: int = 30
    guidance: float = 7.0
    size: int = 1024


def _decode(data: str):
    from PIL import Image

    raw = base64.b64decode(data.split(",", 1)[-1])
    return Image.open(io.BytesIO(raw)).convert("RGB")


@app.on_event("startup")
def load_models() -> None:
    """Load both models and force CUDA to compile its kernels, before anyone is waiting.

    The first generation after a fresh load pays a PTX JIT that later ones do not -- measured at
    ~60 s against ~16 s steady state. Doing a throwaway one-step image here moves that cost off
    the first player's request and into startup, which is the whole point of the daemon.
    """
    started = time.monotonic()
    STATE["loading"] = "vlm"
    try:
        import torch
        from diffusers import AutoencoderKL, StableDiffusionXLPipeline
        from transformers import AutoProcessor, Qwen2VLForConditionalGeneration

        STATE["processor"] = AutoProcessor.from_pretrained(VLM_MODEL)
        STATE["vlm"] = Qwen2VLForConditionalGeneration.from_pretrained(
            VLM_MODEL, torch_dtype=torch.float16, device_map="cuda"
        )
        LOGGER.info("VLM loaded in %.1fs", time.monotonic() - started)

        STATE["loading"] = "sdxl"
        # The stock SDXL VAE produces black patches in fp16; this is the standard fix.
        vae = AutoencoderKL.from_pretrained(
            "madebyollin/sdxl-vae-fp16-fix", torch_dtype=torch.float16
        )
        pipe = StableDiffusionXLPipeline.from_pretrained(
            SDXL_MODEL, vae=vae, torch_dtype=torch.float16, variant="fp16", use_safetensors=True
        ).to("cuda")
        pipe.set_progress_bar_config(disable=True)
        STATE["sdxl"] = pipe

        STATE["loading"] = "jit"
        pipe(prompt="warmup", num_inference_steps=1, width=512, height=512,
             generator=torch.Generator("cuda").manual_seed(0))

        STATE["ready"] = True
        STATE["loading"] = None
        STATE["warm_seconds"] = round(time.monotonic() - started, 1)
        LOGGER.info("resident forge ready in %ss", STATE["warm_seconds"])
    except Exception as error:  # noqa: BLE001 - surfaced through /health, never silently dead
        STATE["error"] = f"{type(error).__name__}: {error}"
        STATE["loading"] = None
        LOGGER.exception("startup failed")


@app.get("/health")
def health() -> dict[str, Any]:
    return {
        "ready": bool(STATE["ready"]),
        "loading": STATE["loading"],
        "warm_seconds": STATE["warm_seconds"],
        "generations": STATE["generations"],
        "error": STATE["error"],
        "models": {"vlm": VLM_MODEL, "sdxl": SDXL_MODEL},
    }


@app.post("/read")
def read(request: ReadRequest) -> dict[str, Any]:
    """Read a drawing into a garment spec, one focused question at a time."""
    if not STATE["ready"]:
        raise HTTPException(status_code=503, detail=f"still loading: {STATE['loading']}")

    processor, model = STATE["processor"], STATE["vlm"]
    image = _decode(request.sketch)
    started = time.monotonic()

    def ask(question: str) -> str:
        messages = [{"role": "user",
                     "content": [{"type": "image"}, {"type": "text", "text": question}]}]
        prompt = processor.apply_chat_template(messages, tokenize=False,
                                               add_generation_prompt=True)
        inputs = processor(text=[prompt], images=[image], return_tensors="pt").to("cuda")
        # Greedy: this is extraction, and sampling only adds ways to be creatively wrong.
        generated = model.generate(**inputs, max_new_tokens=request.max_tokens, do_sample=False)
        return processor.batch_decode(
            [generated[0][inputs.input_ids.shape[1]:]], skip_special_tokens=True
        )[0].strip()

    answers = {field: ask(question) for field, question in QUESTIONS.items()}
    spec: dict[str, Any] = {"garment": answers["garment"].strip().rstrip(".").lower()}

    lowered = answers["sleeves"].lower()
    spec["sleeves"] = next((word for word in SLEEVES if word in lowered), "")

    text = answers["text"].strip().strip(".\"'")
    spec["text"] = "" if text.upper() in NEGATIVES or text.upper().startswith(
        ("NONE", "NO ", "N/A", "THERE IS NO", "THERE ARE NO")
    ) else text

    spec["trousers"] = answers["trousers"].strip().lower().startswith("yes")

    raw = answers["accessories"].strip().rstrip(".")
    spec["accessories"] = [] if raw.upper() in NEGATIVES or raw.upper().startswith(
        ("NONE", "NO ", "NOTHING")
    ) else [item.strip().lower() for item in re.split(r",", raw)
            if item.strip() and item.strip().upper() not in NEGATIVES]

    spec["_seconds"] = round(time.monotonic() - started, 1)
    LOGGER.info("read %s in %ss", spec.get("garment"), spec["_seconds"])
    return spec


@app.post("/sprites")
def sprites(request: SpritesRequest) -> dict[str, Any]:
    """Generate every sprite an outfit needs, with the pipeline already resident."""
    if not STATE["ready"]:
        raise HTTPException(status_code=503, detail=f"still loading: {STATE['loading']}")
    import torch

    pipe = STATE["sdxl"]
    started = time.monotonic()
    out: dict[str, str] = {}
    for job in request.jobs:
        style = ACCESSORY_STYLE if job.name.startswith("accessory_") else STYLE
        image = pipe(
            prompt=f"{job.prompt}, {style}",
            negative_prompt=NEGATIVE,
            num_inference_steps=request.steps,
            guidance_scale=request.guidance,
            width=request.size,
            height=request.size,
            generator=torch.Generator("cuda").manual_seed(job.seed),
        ).images[0]
        buffer = io.BytesIO()
        image.save(buffer, format="PNG")
        out[job.name] = base64.b64encode(buffer.getvalue()).decode("ascii")
        STATE["generations"] += 1

    elapsed = round(time.monotonic() - started, 1)
    LOGGER.info("generated %d sprite(s) in %ss", len(out), elapsed)
    return {"sprites": out, "seconds": elapsed}
