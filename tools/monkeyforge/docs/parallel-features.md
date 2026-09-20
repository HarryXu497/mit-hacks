# Parallel feature brief

**Audience:** a second agent working alongside the GX10/TRELLIS.2 build.
**Status:** reconciled against `HarryXu497/mit-hacks` on 2026-09-19. The proposal below is kept
intact for its reasoning; **read "Reconciliation" first — it reorders the list and reshapes #5.**

Everything below is CPU-only and does not touch the GPU worker, so it can proceed while TRELLIS.2 is
being brought up on the ASUS Ascent GX10.

---

## The filter

MonkeyForge describes itself as a *"constraint-aware compiler for generated, game-ready monkey
characters."* That word — compiler — is the thesis, and it is the right one. Image-to-3D generation
is becoming commodity; anyone can call TRELLIS. The defensible part is the constraint system that
makes generated content **shippable**: canonical sockets, triangle ceilings, hard validation,
deterministic rebuilds.

So: **features that deepen the compiler, not the generator.** Anything whose pitch is "nicer meshes"
was rejected. Ranked below by (identity value x parallel-safety).

---

## Reconciliation — what `mit-hacks` actually is

Surveyed 2026-09-19 at commit `62d9838`.

**It is a vendored third-party project.** `README.md`: *"Original environment, game, and RL bindings
from [Aijo24's Cube Soccer 3D](https://github.com/Aijo24/Cube-soccer-3D)."* `Cargo.toml` still carries
`name = "cube-soccer"` and Aijo24's repo URL. It shares no code with MonkeyForge and is not an earlier
version of this pipeline.

**What it is:** Bevy 0.13 + `bevy_rapier3d` 0.25, a 5v5 soccer game (`PLAYERS_PER_TEAM = 5`,
`NUM_AGENTS = 10`) built as a **reinforcement-learning environment** — gymnasium bindings via PyO3,
PPO training, self-play. Players are `Cuboid::new(1.5, 1.5, 1.5)` primitives with googly-eye child
spheres. It implements **none** of the five features, not partially.

### The finding that reorders this document

`Cargo.toml` sets `default-features = false` and the explicit feature list omits `bevy_gltf`,
`bevy_scene` and `bevy_animation`. Grepping the repo for `gltf|glb|AssetServer|SceneBundle|
SkinnedMesh|AnimationPlayer` returns exactly one hit — `PPO.load` in `python/evaluate.py` — and there
is no `assets/` directory.

**The game cannot load a GLB at all.** Not "cannot load skinned GLBs". There is no asset pipeline.
Every visual is a procedural primitive. MonkeyForge's output currently has no consumer, which is a
bigger gap than anything in the list below and is not in the list below.

### Revised priority

| # | Feature | Verdict |
|---|---|---|
| 4 | Lockfile / digest | **First.** Urgent — see below. Done 2026-09-19. |
| — | Enable GLB loading in `mit-hacks` | **New, gates everything.** Not in the original list. |
| 5 | Geometry -> gameplay | **Reshape** as passive physics scalars, not abilities. |
| 3 | `semantic_similarity` | Keep; blocked on `features.py`. |
| 2 | Part binding | Drop for now — needs the contended GPU. |
| 1 | Rigging | **Last.** Needs an animation system that does not exist. |

**#5 must be reshaped.** As written it adds to the action and observation spaces — both compile-time
constants feeding a trained policy. That invalidates every PPO checkpoint and forces retraining on the
GPU that TRELLIS.2 is using. Worse, the env assumes all 10 agents are **homogeneous** and
`self_play.py` shares one policy across them; per-character abilities make them heterogeneous, which is
a different learning problem. Instead map measured geometry onto **passive physics scalars** Rapier
already reads per entity — back-prop area -> `linear_damping`/`CUBE_MAX_SPEED`, shoulder-plate coverage
-> `ColliderMassProperties::Mass`, head-prop compactness -> `TOUCH_RANGE`. No space changes, no
retraining, and it demos better: *the monkey you drew is faster because the cape you drew is bigger.*

**#1 must be deprioritised.** There is no `AnimationPlayer`, no locomotion state machine, and players
have `LockedAxes::ROTATION_LOCKED_X | ROTATION_LOCKED_Z` so there is no facing to drive a gait from.
The headless RL path does not render, so rigging contributes zero to training. The googly-eye system
already supplies most of the perceived character for free.

### The open questions, answered

1. **What is in `mit-hacks`?** Aijo24's Cube Soccer 3D, vendored. It is the runtime, adopted not
   written. Solves none of the five.
2. **Is there a real-time runtime?** Yes — Bevy 0.13/Rust. It *would* need skinned GLB with bone
   animations, which is exactly why #1 is too expensive now. **The triangle ceiling is not binding:**
   the scene draws ~12 primitives today, so ten characters at 2500 tris/accessory is nothing. The real
   constraint is **physics** — `TOUCH_RANGE = CUBE_SIZE/2 + BALL_RADIUS + 0.5 = 1.85`, mass 10.0, a
   1.5 m cuboid collider, and possession/steal/reward all tuned against it. **Rule: generated geometry
   is cosmetic. The collider stays a 1.5 m box and the visual mesh parents to the physics body.**
   Anything else invalidates trained policies.
3. **Who owns the gameplay loop?** Not a person — a trained PPO policy (`train_ppo.py`,
   `self_play.py`). "Tune against real play" means *retrain*, on contended GPU. Independently the
   strongest argument for the #5 reshape.
4. **Is `RankerArtifact.score()` live or offline?** Keep it **offline**. Live selection means N
   generations per request, multiplying load on the contended resource, and nothing in the runtime
   consumes a ranker.

---

## Current state — verified facts

Established by direct inspection this session. Trust these over memory.

```text
src/monkeyforge/models.py          CharacterSpec, AccessorySpec, Socket, Palette, AbilitySpec
src/monkeyforge/pipeline.py        job state machine; calls compiler, measurer, validation, features
src/monkeyforge/measurements.py    GlbMeasurement contract + BlenderMeasurer
src/monkeyforge/features.py        measurements/renders -> the six ranker features
src/monkeyforge/validation.py      geometry-aware validation
src/monkeyforge/variants.py        procedural accessory variants (clean -> deliberately broken)
src/monkeyforge/ranking.py         pairwise ranker; RankerArtifact.score() has NO caller yet
scripts/blender_compile.py         normalises scale, seats accessory on socket, decimates to budget
scripts/blender_measure_glb.py     structured measurement; --bare measures a raw uncompiled asset
scripts/generate_candidates.py     builds a labelable candidate set with no GPU
scripts/review_pairs.py            plan -> CSV -> label -> build -> dataset JSONL
worker/                            contract worker; stub + trellis generators
```

- Seven canonical sockets: `SOCKET_HEAD_TOP`, `FACE`, `BACK`, `WAIST`, `HAND_LEFT`, `HAND_RIGHT`,
  `TAIL_TIP`. Four rigged bases: balanced, runner, defender, goalkeeper.
- Tests: 29 passing. Ruff clean. Run pytest with
  `-p no:cacheprovider --basetemp=.\.pytest-tmp` (the default temp dir hits a PermissionError).
- Six ranker features. Three are measured from geometry; two from a render; **`semantic_similarity`
  is hardcoded to 0.5** and is the single largest hole in the system.
- The compiler currently compiles **only the first accessory** in a spec.
- `AbilitySpec` is assigned from text by `spec_provider.py` and is **completely decoupled from
  geometry**.

---

## 1. Rig once, inherit forever

**The gap.** `docs/HANDOFF.md` names this first: *"Rigid per-part weights need real smooth
deformation weights and a small animation test set."* The bases are rigged with rigid per-part
weights, so they cannot deform. For a sports game, characters that cannot animate are not
characters.

**The insight that makes it interesting.** Do not rig generated content at all. There are four
canonical bases with a fixed skeleton. Solve smooth skinning **once per base**, and every generated
variant inherits it. An accessory inherits the bone binding of whatever socket it attaches to — a
helmet on `SOCKET_HEAD_TOP` is bound to the head bone by construction, not by inference.

That is the compiler thesis applied to rigging: **generated content is never rigged, it is bound.**
Rigging cost becomes O(4) instead of O(characters), which is also what makes it viable in real time.

**Tech.**
- [UniRig](https://github.com/VAST-AI-Research/UniRig) (SIGGRAPH 2025) — skeleton + skinning prediction
- [SkinTokens](https://github.com/VAST-AI-Research/SkinTokens) — successor; 98-133% better skinning accuracy

**Integration points.** `scripts/blender_build_original_base.py` produces the bases; weights are set
there. `scripts/blender_compile.py:82-88` already parents the accessory to the socket object — bone
binding hangs off that same step.

**Parallel-safe.** The inheritance mechanism is pure Blender/Python, no GPU. Use UniRig only offline
as a reference for the four base weight sets, or hand-author them. Do not put a neural rigger in the
request path.

**Done when.** A base can be posed through a walk/kick cycle without visible seams at the joints, an
attached accessory follows its socket bone, and there is a small animation test set to check against.

---

## 2. Semantic part binding

**The gap.** `AccessorySpec.part_schema` declares semantic parts (e.g. `["shell", "crest"]`) and
**nothing verifies them**. `features.disconnected_penalty` only compares mesh island count against
`len(part_schema)` — a proxy that cannot distinguish a legitimate crest from a broken shard.

**What real part segmentation unlocks, all at once:**
- **Validation becomes semantic.** "This helmet has a shell and a crest" instead of "this has 2
  islands."
- **Palette stops being blind.** `Palette` carries `fur/face/jersey/accent`, but accent is applied
  without knowing what anything is. Identify the crest and accent lands *on the crest*,
  automatically, every time.
- **Multi-accessory compilation** (an open TODO) gets much easier when parts are named and can be
  reasoned about individually.

**Tech.**
- [PartField](https://github.com/nv-tlabs/PartField) (ICCV 2025, NVIDIA) — fast feedforward part feature fields
- [PartSAM](https://github.com/czvvd/PartSAM) (ICLR 2026) — promptable, trained on native 3D data
- [SAMPart3D](https://yhyang-myron.github.io/SAMPart3D-website/) — zero-shot, multi-granularity

**Integration points.** Extend `GlbMeasurement` in `measurements.py` with a per-part block; teach
`validation.validate_compiled_asset` to check declared vs. detected parts; palette application lives
in `scripts/blender_compile.py`.

**Done when.** A generated helmet reports named parts, validation fails when a declared part is
absent, and accent colour lands on the correct part without a hand-written rule.

---

## 3. Fill the hole in the feature vector

**The gap.** `semantic_similarity` is **hardcoded to `0.5`** (`features.NEUTRAL_SEMANTIC_SIMILARITY`).
It is one of six ranker features and the only one that is pure placeholder. `docs/training.md`
already calls for it: *"Add frozen visual embeddings (CLIP/DINO/render encoder)."*

**The interesting version.** A 3D-native encoder embeds the **mesh** into CLIP space, so
`semantic_similarity` becomes cosine distance between the mesh and the prompt — judging geometry
semantically **without rendering it**. No camera rig, no lighting, no view-dependence. "Does this
actually look like a goalkeeper helmet?" answered in 3D, deterministically.

**Tech.**
- [Uni3D](https://arxiv.org/pdf/2310.06773) — ViT-based 3D encoder, CLIP-aligned
- [OpenShape](https://arxiv.org/html/2305.10764) — tri-modal text/image/point-cloud contrastive

**Integration points.** `features.extract_features` already accepts `semantic_similarity` as a
parameter — it is a genuine drop-in. The `imaging` extra exists in `pyproject.toml`; add a `semantic`
extra rather than widening `imaging`. Keep the encoder frozen and out of the request path: compute it
during dataset building (`scripts/review_pairs.py build`), not during a live job.

**Done when.** `semantic_similarity` varies meaningfully across candidates for the same prompt, and
the trained ranker gives it non-zero weight.

---

## 4. Every character has a lockfile

**Mostly already built, never named.** `pipeline.request_digest` content-addresses jobs; jobs are
cached by digest; `manifest.json` records spec, provider, compiler and base profile. Formalise it:
**a character spec + seed + model version rebuilds bit-identically, forever.**

**Why it matters.** Nobody in generative 3D is telling this story. It is Nix/Bazel for game assets —
content-addressed, reproducible builds. It reframes the project from "AI makes stuff" to "AI output
you can actually ship," which is the difference between a demo and a pipeline.

**The demo move.** Show a character. Delete it. Rebuild it live from a ~200-byte spec. Hash-match the
original. It takes twenty seconds and it makes every other claim credible.

**Integration points.** `pipeline.py` (digest, manifest), `storage.py` (job store). Add the model
version and compiler version to the digest inputs — they are currently missing, which means a model
upgrade silently reuses stale cached output. **That is a real bug, not just a feature gap.**

**Parallel-safe.** 100% CPU, no new dependencies, mostly wiring what exists.

**Done when.** `monkeyforge rebuild <spec.json>` reproduces a byte-identical GLB, and changing the
generator version correctly invalidates the cache.

### Status: cache-correctness half done, 2026-09-19

The bug was worse than described. `api.create_character` computes the job id from the digest and, if a
record with that id exists, **returns it without rebuilding** — and the digest hashed only description,
seed and sketch bytes. So flipping `MONKEYFORGE_PROVIDER` from `mock` to `http` to bring up TRELLIS.2
keeps the id identical and serves the cached placeholder OBJ forever. TRELLIS appears to do nothing and
the failure is silent. `worker/server.py:75` already hashed `generator.version` into *its* cache key —
the worker was right, the orchestrator was not.

Shipped:

- `models.BuildIdentity` — pipeline, provider, provider version, compiler, compiler version, base-model
  version. `request_digest(..., build)` mixes its digest into the job id.
- **Versions derive from content where possible.** `BlenderCompiler.version` hashes
  `scripts/blender_compile.py`, so editing the compile script invalidates builds with no manual bump;
  `base_models_version` hashes the base GLBs.
- Provider version resolution: explicit `MONKEYFORGE_GENERATOR_VERSION` pin > the worker's `/health`
  report (probed at startup, tolerant of the worker being down) > a provider constant > `unknown`.
- `HttpGeometryProvider` records the worker's `X-Generator-Version` response header as ground truth,
  stored alongside the believed version so a misconfiguration is visible rather than silent.
- **Second line of defence:** `api._should_rebuild` rejects a cached job whose recorded identity no
  longer matches, including jobs predating build tracking. This catches drift the digest misses.
- Also fixed, same area: a **FAILED job was cached forever**. A transient worker error during GPU
  bring-up would pin the failure to that exact request permanently. Failed jobs are never served.
- `manifest.json` gains a `build` block — the lockfile proper.

**Still open:** the `monkeyforge rebuild <spec.json>` CLI and the byte-identical hash-match demo. The
cache-correctness half was done first because it blocks the GX10 work; the CLI is the demo move and
does not block anyone.

---

## 5. Geometry compiles to gameplay

**The most distinctive idea here, and the one judges will remember.**

`AbilitySpec` (`tailwind`, `curve_shot`, `rally`, `intercept_read`, `goalie_focus`, `shield_wall`) is
currently assigned from **text** by `spec_provider.py`, with no relationship to the geometry that
gets built. Invert that: **let the generated prop determine the mechanic.**

- A back-slot prop with large surface area -> `tailwind`, magnitude scaled by measured area
- A broad shoulder plate -> `shield_wall`, scaled by how much of the torso it actually covers
- A dense, compact head prop -> `goalie_focus`

Gameplay emerging from geometry the player drew. **The art is the mechanic.** It is a genuinely novel
loop, it reuses the measurement system that already exists, and it makes `ValidationMetrics` do
double duty as a balance system — an oversized prop is not just a validation warning, it is a
balance problem with a number attached.

**Integration points.** `measurements.py` already reports `extent`, `bbox_min/max`, `triangles` and
`socket` per accessory. `AbilitySpec.magnitude` is bounded `0..0.35` and `duration_seconds` `0..8`,
so there are natural ranges to map measured geometry onto. Ability assignment currently lives in
`spec_provider.py` and would move to a post-measurement step in `pipeline.py`.

**Parallel-safe.** Pure CPU, builds directly on `measurements.py` and `features.py`.

**Done when.** Two visibly different props for the same slot produce measurably different ability
parameters, and the mapping is documented and bounded so it cannot produce degenerate builds.

---

## What to skip, and why

- **Multi-view / video generation** — serves the generator, not the compiler. Bloat.
- **An LLM chat interface for character creation** — every hackathon demo has one; adds nothing to
  the thesis.
- **Gaussian splatting / NeRF rendering** — actively wrong for a low-poly cel-shaded game.
- **Physics simulation** — out of scope while the gameplay loop is still unstable.

**If only two get built: #5 for the wow, #4 for the credibility.** Both are CPU-only and build on
code that already exists.

---

## Hard-won gotchas

Discovered the hard way this session. Each of these cost real debugging time.

1. **glTF splits vertices at every hard edge and UV seam.** Counting connected components on raw
   imported topology reports a flat-shaded cube as six parts. The measurement pass welds coincident
   vertices first; without that the goalkeeper helmet reported 56 islands instead of 2.

2. **Socket fit must be a gap to the bounding box, not a distance to its centre.** A hat correctly
   seated on `SOCKET_HEAD_TOP` legitimately extends upward, so centre-distance penalises exactly the
   props that are right.

3. **Measure generator quality on the raw accessory, not the compiled character.**
   `scripts/blender_compile.py` re-seats the prop onto its socket (lines 82-88) and decimates it to
   budget (lines 93-103). Both are correct compiler behaviour, and both destroy exactly what
   `socket_fit_score` and `triangle_budget_score` try to observe — measured after compilation they
   are constant by construction and the ranker assigns them zero weight. Use
   `blender_measure_glb.py --bare`.

4. **`blender_render_preview.py` sets `film_transparent = False`.** Renders are fully opaque, so an
   alpha mask selects the entire frame. `features._subject_mask` infers the background from the
   border ring instead; without it every silhouette scored 0.0 and every palette score sat at a
   near-constant 0.70.

5. **`target_triangles` is a ceiling, not a quota.** `spec_provider.py` hardcodes 2500 for every
   accessory. A prop that reads correctly with fewer triangles is better, not worse. Degeneracy is an
   absolute floor (`MINIMUM_VIABLE_TRIANGLES = 50`), not a fraction of the ceiling.

6. **The ranker only learns from features that differ within a pair.** Candidates sharing one
   `features.json` train a model that ignores geometry entirely. Give every candidate its own.

7. **`tests/test_ranking.py` asserts `pairwise_accuracy == 1.0`** on four synthetic examples with six
   free parameters — trivially separable, and meaningless as a quality signal. It **will** fail when
   real labels arrive. That failure is correct; relax the bound, do not tune until it goes green.

---

## Open questions for whoever picks this up

1. What is actually in `HarryXu497/mit-hacks`? Is it the game client, a separate prototype, or an
   earlier version of this pipeline? Does it already solve any of the above?
2. Is there a real-time runtime (Unity/Godot/three.js) that consumes these GLBs? That decides whether
   #1 needs to export skinned GLB with bone animations, and constrains the triangle ceiling.
3. Who owns the gameplay loop? #5 only works if someone can tune the ability mapping against real
   play.
4. Is `RankerArtifact.score()` meant to select among multiple candidates in the live path, or stay an
   offline analysis tool? It currently has no caller, and the compiler only handles the first
   accessory in a spec — those two limitations land together.
