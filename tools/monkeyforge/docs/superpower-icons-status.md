# Superpower icons — build status

Companion to `SUPERPOWER-ICONS.md`, which is the brief. This is what was built against
it, what was measured, and what is left. **Written 2026-09-19.**

---

## The power list is four, and that changes the brief's examples

`HarryXu497/mit-hacks:src/systems/superpowers.rs` defines exactly:

```rust
pub enum SuperpowerKind { BeamBlast, FreezeRay, Boost, Slow }
```

The brief's worked examples ("a spring means the acceleration power; a lightning bolt
means the speed one") describe two powers that do not exist separately — both are
`Boost`. `src/monkeyforge/powers/registry.py` mirrors the enum, including the
`onehot_index` order (blast=0, freeze=1, boost=2, slow=3) that the RL observation
vector depends on. That order is asserted at import: reordering it silently
invalidates every trained PPO checkpoint.

---

## What exists

```text
src/monkeyforge/powers/registry.py    the four powers as data, pinned to the Rust enum
src/monkeyforge/powers/classify.py    CLIP zero-shot: power + confidence + runner-up + motif
src/monkeyforge/powers/sketchmask.py  drawing -> ControlNet line art, and -> subject alpha
src/monkeyforge/powers/beautify.py    SDXL+ControlNet subject generator, and a no-GPU stand-in
src/monkeyforge/powers/badge.py       the procedurally drawn badge frame and compositor
src/monkeyforge/powers/pipeline.py    sketch + description -> PowerIcon

src/monkeyforge/powers/demo.py        FastAPI app for the draw-in-browser demo
src/monkeyforge/powers/demo.html      the demo page

scripts/build_power_icons.py          end-to-end CLI (the deliverable)
scripts/serve_power_icons.py          run the demo page (port 8610, not 8600)
scripts/classify_sketch.py            classify one sketch
scripts/eval_classifier.py            measure accuracy against a labelled directory
scripts/draw_test_sketches.py         synthetic evaluation doodles
scripts/render_power_badges.py        the four placeholder badges
scripts/crop_icon_sheets.py           reference sheets -> 66 icon cells
scripts/build_lora_dataset.py         cells -> subjects-only LoRA training set
```

133 tests pass and `ruff check` is clean across all of it. Everything except
`beautify.SdxlControlNetGenerator` runs on the laptop; only the badge compositor and
sketch masking need no torch at all.

Dependencies are declared as two extras: `powers` (numpy, pillow, torch, transformers)
for classification and compositing, and `powers-gpu` (diffusers, accelerate, peft,
torchvision) for the subject generator, which only ever runs on the CUDA box.

### Running it

```powershell
# four placeholder badges, no model at all
.\.venv\Scripts\python.exe scripts\render_power_badges.py

# the demo page, real classification, placeholder subjects
.\.venv\Scripts\python.exe scripts\serve_power_icons.py --generator placeholder
```

```bash
# on the GX10, with the real generator, reachable from the laptop
python scripts/serve_power_icons.py --generator sdxl --host 0.0.0.0 --port 8610
python scripts/build_power_icons.py --sketch-dir output/sketches --generator sdxl
```

---

## Measured, not assumed

**Classification — the brief was right, do not train it.** Zero-shot CLIP ViT-B/32
over prompt-ensembled labels, measured on 12 synthetic doodles:

| input | top-1 | top-2 |
| --- | --- | --- |
| sketch only | 11/12 (92%) | 11/12 |
| sketch + typed description | 12/12 (100%) | 12/12 |

The single sketch-only miss (icicles → Slow) was flagged `unsure`, so it surfaces as
"I think this is Slow — or did you mean Freeze Ray?" rather than as a silent error.
Confidences are **not calibrated**: `DEFAULT_LOGIT_SCALE = 50` was chosen to give a
usable spread, not fitted. Treat the number as an ordering until it is re-fit on real
drawings. The doodles are synthetic and n=12; this says the path works, not that it is
92% accurate on humans.

**Two accuracy-critical details.** Each power is represented by the *average of ~50
embedded phrasings* of what someone might draw, not by one label — single-label
zero-shot is materially worse. And `Power.motifs` names drawable things ("a coiled
spring"), never the engineering name; nobody sketches "BeamBlast".

---

## The beautifier: four things that cost real time

The prompt recipe in `beautify.py` is the survivor of a measured sweep. Each clause is
load-bearing, and the failures are worth not repeating:

1. **Put the composition clause first.** "a single … one isolated object centered on a
   plain background" at the front gives one object; the same words at the back let
   SDXL produce a *collage of little game assets*. Early tokens dominate. This flipped
   the result twice in both directions, so it is not noise.
2. **Keep prompts under 77 tokens.** An 82-token negative prompt was silently truncated
   and quietly dropped the "photo, photorealistic" terms that were doing the work.
   CLIP does not warn twice; check token counts when a negative stops having an effect.
3. **Get colour from the power, not from adjectives.** "vivid saturated colors" turned
   whole frames into rainbow noise; saying nothing produced grey extruded plastic.
   `Power.palette_words` is specific and deterministic, and keeps the four powers
   distinguishable at a glance.
4. **Ask for volume and name hollowness in the negative.** Otherwise ControlNet's line
   conditioning comes back as the player's pencil outline in a new colour.

**Control scale 0.65.** At 0.8 the strokes were reproduced so literally that subjects
stayed flat; below ~0.5 the drawing stopped being recognisable.

**Cutting the subject out: use the drawing, not just saliency.** RMBG-2.0 alone fails
exactly when the generation is cluttered — it returns nearly the whole canvas, and the
badge ends up showing a *square patch* of background. But the silhouette is already
known, because the player drew it. `sketchmask.subject_mask` thickens the strokes and
fills what they enclose; that cannot fail that way, and it enforces the actual product
promise that the icon keeps *your* shape. The pipeline intersects the two and falls
back to the drawn mask alone when saliency erases almost everything.

`sketchmask` uses only Pillow and numpy — no OpenCV — so it installs and tests in the
API environment. It was validated against the OpenCV implementation it replaced:
IoU 0.95–0.99 on the evaluation sketches (0.89 on the spiral).

---

## The badge frame is drawn, not cropped

`badge.py` builds the rim from gradients and signed-distance masks rather than
compositing a cropped BTD6 badge. Two reasons: a copied rim would put Ninja Kiwi's art
in every shipped icon, which `HANDOFF.md` explicitly forbids; and a drawn frame is
byte-identical across every icon and tunable. Antialiasing comes from distance fields,
so a 512px badge renders in one pass.

The subject is clipped to the inner circle, so a bad generation degrades to a cropped
subject rather than a broken silhouette.

**`.gitignore` gap, now fixed.** Only `btd6_prototype/` was ignored; the icon sheets
and 2D style sheets were not, so they were committable. The rule now covers them. If
they were already committed, `.gitignore` does not untrack them — that still needs
checking (git is not installed on this machine).

---

## Performance

On the GX10, warm: **~16 s per icon** at 1024×1024, 28 steps, SDXL + ControlNet. The
first generation takes ~60 s because PTX JIT compilation lands in it — measure after
warmup, as the brief warns. Pipeline load is ~40 s and about 8 GB, so construct
`SdxlControlNetGenerator` once and keep it.

---

## What is left

1. **Train the style LoRA.** The dataset is built and waiting: 65 subjects at 1024²
   with captions, in `output/lora/train` on the box, trigger token `btd6icon`. This is
   the remaining quality gap — prompting alone gets a clean, correctly-coloured,
   correctly-shaped object, but not BTD6's specific look, exactly as the brief
   predicted. `beautify.SdxlControlNetGenerator` already takes a `lora_path`.
2. **Re-measure the classifier on real human drawings** and re-fit `logit_scale`.
3. **Serve it.** The generator currently runs via SSH. If it becomes a service, use a
   port other than 8600 and copy the contract in `docs/gpu-worker-contract.md`.

### Note on the GX10

The box reboots without restarting the 8600 worker — it was down when this work
started and needed only a restart, not a network fix. Nothing here uses port 8600.

### Note on the reference art

`output/lora/` and the cropped cells are derived from Ninja Kiwi artwork. Training
input only: never the public repo, the released game, or a shipped asset. `output/` is
git-ignored.
