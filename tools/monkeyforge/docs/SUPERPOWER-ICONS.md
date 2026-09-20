# Superpower icon model — context brief

**For:** a second agent building the sketch → superpower pipeline.
**Written:** 2026-09-19, from a day spent standing up the character pipeline on the same hardware.

---

## The task

A user draws a superpower roughly and describes it. Two things must happen:

1. **Map it to an existing power.** A drawn spring means the acceleration power; a lightning bolt
   means the speed one. The authoritative list lives in `HarryXu497/mit-hacks` (pushed) — read it
   there, do not invent powers.
2. **Beautify it into a BTD6-style icon.** Circular frame, metallic rim, glossy centred subject.

These are two different problems and want two different tools. Conflating them is the main way this
goes wrong.

---

## Do not train the classifier

Mapping a sketch to one of N known powers is **zero-shot** with an off-the-shelf model. CLIP scores
an image against text labels directly; a small VLM does the same and can also explain itself. With
a fixed power list, this is a retrieval problem, not a learning problem.

Training a classifier here would mean hand-labelling hundreds of sketches to reproduce what CLIP
already does out of the box. Spend that time on the beautifier instead.

The one thing worth building around it: **return a confidence and a runner-up.** A drawn spring is
unambiguous; a drawn squiggle is not. Showing the user "I think this is Acceleration — or did you
mean Bounce?" is better product behaviour than a silent wrong guess, and it is free.

---

## The beautifier is where a LoRA earns its keep

Rough sketch → clean BTD6 icon is subjective and stylistic, which is exactly what a LoRA is for.

**Style reference is already collected** in `assets/reference/btd6_icons/` — three sheets, roughly
70 icons, saved from the same source images the team gathered.

That style is unusually **templated**, and this is the most useful observation in this document:

- circular badge, consistent metallic ring
- subject centred, glossy highlight top-left
- flat saturated colours, dark outline
- consistent lighting angle across every icon

Because the frame is near-identical every time, **do not generate the whole icon.** Generate only
the subject on a transparent or flat background, then composite it into a fixed frame template.
That gives a perfectly consistent rim on every icon for free, removes the model's hardest job, and
makes failures obvious (a bad subject, not a warped badge).

A LoRA on ~70 icons is small but workable for style. Crop each icon out of the sheets first; the
sheets are grids, so a simple grid crop plus an alpha/background trim gets you the dataset.

---

## Hard-won lessons from the character pipeline

These cost real time today. They apply directly.

- **Diffusion models cannot spell.** Asking for "MIT" produced `3I`, `XJZ`, `VU`. If an icon needs
  text or a number, render it with a font and composite it. Never prompt for it.
- **Prompt for the right medium.** Asking for "flat vector, no shading, no lighting" produced
  sticker art when a 3D-style render was wanted. The BTD6 icons are *glossy and shaded* — prompt for
  that, not for flatness.
- **SD 1.5 does not know what a BTD6 monkey is.** It produced a generic CGI mouse. Either use SDXL,
  or train the LoRA, or both. A prompt cannot supply identity the model never learned.
- **Check `variant="fp16"`.** If only fp16 shards were downloaded and `from_pretrained` is called
  without it, diffusers silently re-downloads full-precision weights and appears to hang.
- **Inpainting preserves; text-to-image replaces.** If part of an image must survive untouched, use
  inpainting — it copies everything outside the mask by construction.

---

## The GX10 box

ASUS Ascent GX10, NVIDIA GB10 Grace Blackwell, aarch64, Ubuntu 24.04 / DGX OS.

```
host      10.189.122.101      (hostname gx10-f3fb; .local mDNS is unreliable)
user      asus
ssh key   ~/.ssh/gx10_ed25519     (already authorised; password auth also works)
sudo      passwordless
```

Verified capability:

```
GPU            NVIDIA GB10, driver 580.159.03, CUDA 13.0, compute capability 12.1 (sm_121)
Memory         128 GB unified (117 GB free); ~44 GB used by a 4B model
Compute        85.8 TFLOP/s fp16   (measure AFTER warmup: a cold first matmul reads ~12x slower
               because PTX JIT compilation lands inside the timing loop)
Bandwidth      240 GB/s measured, 88% of the 273 GB/s spec
Disk           916 GB, ~780 GB free
```

**Bandwidth is the constraint, not capacity.** 240 GB/s against an H100's 3.35 TB/s. The 128 GB
means big models *fit*; it does not make them fast. Diffusion at 512 runs in ~3 s, which is plenty
for icons.

### Python environment

```
venv        ~/trellis-env          (source ~/trellis-env/bin/activate)
torch       2.14.0+cu130 aarch64
installed   diffusers, transformers, accelerate, peft, numpy, pillow, opencv,
            trimesh, nvdiffrast, utils3d
```

`peft` is present, so LoRA training needs no extra install.

### Models already downloaded (no need to re-fetch)

```
stable-diffusion-v1-5/stable-diffusion-v1-5              ~2 GB   fp16
lllyasviel/control_v11p_sd15_scribble                    ~1.4 GB fp16
stabilityai/stable-diffusion-xl-base-1.0                 ~7 GB   fp16   (in progress)
diffusers/controlnet-canny-sdxl-1.0                             fp16
microsoft/TRELLIS.2-4B                                   16 GB   (image-to-3D, not needed for icons)
```

### Ports

```
22     ssh
8600   MonkeyForge worker (FastAPI). Currently the stub generator.
       Health: curl http://10.189.122.101:8600/health
       Token:  MONKEYFORGE_WORKER_TOKEN=hackmit-gx10
```

Port 8600 is reachable from the laptop on the current network. If you stand up a second service,
pick another port and confirm reachability before building against it — the worker contract lives
in `docs/gpu-worker-contract.md` and is worth copying rather than reinventing.

### Downloads

Always set `HF_HUB_DISABLE_XET=1`. HuggingFace's Xet chunk reassembly deadlocks on this box under
unauthenticated parallel requests — observed as open connections, `futex_do_wait`, and 33 KB moved
in 45 seconds. Classic HTTP sustains 3–6 MB/s. Use the CLI (`hf download`) rather than downloading
inside your pipeline, so it is resumable and diagnosable on its own.

An `HF_TOKEN` is configured on the box. Some models are gated: DINOv3 is `gated: manual` (needed a
human review, granted) and `briaai/RMBG-2.0` is `gated: auto` (click-through, granted). Check
`"gated"` on the HF API *before* starting a long download.

---

## Suggested order

1. Read the power list from `HarryXu497/mit-hacks`. Everything else depends on its shape.
2. Zero-shot classifier with CLIP over the power labels. Measure accuracy on a handful of sketches
   you draw yourself. This should take under an hour and may simply be done.
3. Build the icon frame template by compositing a subject into a cropped BTD6 badge. Prove the
   compositing path with a flat placeholder before any model is involved.
4. Crop the reference sheets into a LoRA dataset (~70 icons).
5. Train the LoRA on subjects only, not full badges.
6. Wire: sketch → classify → generate subject → composite into frame.

Steps 1–3 need no GPU. Do them while anything downloads.

---

## Coordination

Another agent is working the character pipeline in this repo. To avoid collisions, **do not edit**:

```
src/monkeyforge/{skin,wardrobe,features,measurements,validation}.py
scripts/blender_*.py
scripts/{paint_garment,uv_*}.py
worker/
```

Tests: `pytest -p no:cacheprovider --basetemp=.\.pytest-tmp` (the default temp dir hits a
PermissionError on the Windows box). Keep ruff clean.
