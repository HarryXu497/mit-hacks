# Branch index: what is in here and who owns it

Two features were built in parallel by separate agents against the same checkout. They share a
package and a test suite and almost nothing else. This is the map, so a reviewer can tell at a
glance which half a file belongs to, what the two halves share, and what must not be broken.

Status at time of writing: **165 tests pass, 0 failures**, `ruff check` clean except six
pre-existing errors in `scripts/blender_{uv_overlay,…}.py` that predate both features.

---

## Feature A — Drawing → dressed 3D character

A child's drawing of a monkey in clothes becomes a game-ready GLB. Ends at a `.glb` plus a render.

### Pipeline, in order

| stage | file | runs on |
|---|---|---|
| read the drawing | `scripts/read_sketch_vlm.py` (Qwen2-VL-2B) | GX10 |
| decide sleeve length etc. | `src/monkeyforge/garments.py` | local |
| generate every sprite in one model load | `scripts/gen_outfit.py` (SDXL) | GX10 |
| accessory → mesh | `scripts/image_to_glb.py` (TRELLIS.2) | GX10 |
| project garment onto the body by 3D position | `scripts/project_planar.py` | local |
| recover relief from flat art | `scripts/garment_height.py` | local |
| restyle the base's own features | `scripts/restyle_features.py` | local |
| assemble, shell, socket, export | `scripts/blender_assemble_character.py` | local (Blender 5.2) |
| orchestrate the lot | **`scripts/forge_character.py`** | local, drives both |

`scripts/dress_monkey.py` is the local half on its own, for when sprites already exist.

### Supporting

`scripts/{island_mask,uv_islands,uv_probe,uv_masks,uv_regions,uv_overlay}.py` — UV analysis that
located the torso, arms and legs. `scripts/blender_export_uv_layout.py` exports the per-face
positions the projector needs. `scripts/restyle_base.py` is a palette restyler, **superseded** by
`restyle_features.py` (a hue shift changed everything; editing the drawn features was what was
actually wanted).

### Superseded, kept only as reference

`scripts/blender_build_btd6_base.py` and `..._skinned.py` carry a docstring saying so. They export
to GLB and re-import, which destroys the OBJ's `map_d` alpha cutout and turns the eyelid quad into
an opaque rectangle across the face. Everything in them that looks like an eye or UV fix is
compensating for that damage. `blender_assemble_character.py` never round-trips.

### Tests

`tests/test_garments.py` (sleeve reading, the garment vocabulary),
`tests/test_project_planar.py` (the warp arithmetic, including a mirror check).

---

## Feature B — Sketch → superpower → badge icon

A sketch plus an optional description becomes one of the four runtime superpowers and a
BTD6-style badge PNG. No 3D, no GLB, no wardrobe. Ends at a PNG and a Python API.

`src/monkeyforge/powers/` — `registry` (the four powers as data), `classify` (CLIP zero-shot with
prompt ensembling), `sketchmask`, `badge` (procedural frame compositor), `beautify`, `pipeline`,
`demo`.

`scripts/` — `build_power_icons`, `serve_power_icons`, `classify_sketch`, `eval_classifier`,
`draw_test_sketches`, `render_power_badges`, `crop_icon_sheets`, `build_lora_dataset`,
`compare_lora`, `train_style_ranker`.

`tests/test_powers_*.py` — registry, classify, badge, sketchmask, pipeline, demo.

`docs/superpower-icons-status.md`, `docs/SUPERPOWER-ICONS.md`.

Measured: 92% top-1 from sketch alone, 100% with a description (n=12, **synthetic** doodles, and
the confidences are uncalibrated — treat them as an ordering, not a probability). ~21 s/icon warm.

---

## What the two features share

Only three things, and each is worth knowing about.

1. **`src/monkeyforge/powers/classify.py`'s `Embedder` protocol.** Feature A's `GarmentReader`
   (`garments.py`) reuses it, so both halves score against CLIP through one interface and both are
   testable without torch. Changing that protocol breaks both.
2. **`pyproject.toml`** — Feature B added `powers` and `powers-gpu` extras. Purely additive.
   Feature A needs no extra; torch and transformers are installed directly in the venv.
3. **`tests/`** — one suite, run together.

Nothing else overlaps. Feature B touched nothing under `src/monkeyforge/{skin,wardrobe,features,
measurements,validation}.py`, `scripts/blender_*.py`, `scripts/{paint_garment,uv_*}.py` or
`worker/`.

---

## Do not break

- **`powers/registry.py` mirrors `SuperpowerKind` in `HarryXu497/mit-hacks`** — BeamBlast,
  FreezeRay, Boost, Slow at one-hot 0/1/2/3. The order is asserted at import because it is written
  into the RL observation vector; reordering invalidates every PPO checkpoint.
- **The badge frame is drawn procedurally, never cropped from BTD6 art** (`powers/badge.py`). A
  copied rim would put Ninja Kiwi artwork in every shipped icon.
- **Feature A's collider contract.** The game's `CubePlayerBundle` keeps
  `Collider::cuboid(CUBE_SIZE/2, ..)`, mass and locked axes; the generated model is parented to it
  as appearance only. `ACTION_SIZE = 4` and `OBSERVATION_SIZE = 18 + 12*PLAYERS_PER_TEAM` are
  compile-time constants feeding trained policies.
- **Sleeve length comes from the garment's name, not from looking at the arms.** Measured: CLIP
  split 36/34 on a drawn t-shirt and Qwen answered "long" for both a t-shirt and a suit. Both name
  garments correctly. See the comment block in `garments.py`.

---

## Open risks

### 1. The base mesh is gitignored, and Feature A needs it

Feature B added `assets/reference/btd6_extract/*` to `.gitignore` — correct policy, that is Ninja
Kiwi art. But `scripts/dress_monkey.py` and `scripts/forge_character.py` read
`assets/reference/btd6_extract/Dart Monkey/dartmonkey.obj` and its 2048² atlas.

**Consequence: a fresh clone cannot run the character pipeline.** The other ignored reference dirs
each keep a tracked `README.md` explaining how to obtain them; `btd6_extract` has no such
exception. It should get one.

### 2. The GLBs already pushed to the game repo embed BTD6-derived art

This one is mine and it contradicts the policy Feature B was careful to honour. The two characters
committed to `HarryXu497/mit-hacks` on `feat/generated-character-skins` are built from the
extracted dart monkey mesh and its texture. The restyling (hue-held palette, star pupils, narrowed
eyes, reshaped muzzle) changes the artwork but does not make it ours.

Feature B avoided exactly this: the badge frame is procedural, and the trained LoRA — derived from
BTD6 reference art — is deliberately left on the GX10 and never committed.

Options, in increasing order of work:

- Keep them, but only if the submission never claims the base art as original.
- Swap the base for `scripts/blender_build_original_base.py`, which builds a team-owned low-poly
  monkey. The whole dressing pipeline is base-agnostic — it measures the mesh rather than assuming
  it — so this is a swap of one input, not a rewrite. The sleeve wrist fraction (0.70) and the
  spike region table would need re-measuring against the new proportions, both documented in place.
- Generate a base from a drawing through the same pipeline.

Worth deciding before the submission, not after.

### 3. MonkeyForge is not under version control

There is no `.git` here and git is not installed on this machine. Feature A's work reached the game
repo only because that checkout lives on the GX10, which does have git. Everything in this document
is currently unversioned local state on one laptop.
