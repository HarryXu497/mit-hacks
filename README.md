# Monkey Business

An AI-assisted game development loop, built around a playable 5-v-5 soccer game.

Two halves of making a game are slow: *creating* content and *testing* it with
real players. Monkey Business attacks both.

- **Create.** A player sketches a character and a superpower at an easel inside
  the game. Qwen2-VL reads the drawing, SDXL and ControlNet generate the
  visuals, CLIP classifies the power, and TRELLIS.2 lifts it into 3D — with
  deterministic code enforcing scale, geometry and attachment points so the
  result is a game-ready asset rather than an unvalidated mesh. The largest
  models run on an ASUS GX10 with NVIDIA GB10 Grace Blackwell.
- **Coach.** A coach talks over a tactics board. Speech is transcribed and
  interpreted into structured tactical JSON, which becomes seven numbers the
  team's players actually play by.
- **Test.** PPO agents are trained **directly inside the real Bevy game**, not a
  separate simulation — they learn the same physics, possession rules,
  superpowers and interactions real players get. We trained across **78.4
  million environment steps** on a curriculum that scales from 1v1 on a small
  pitch up to full 5v5. Then we added tactical conditioning, so the agents can
  be coached without hard-coding what they do: the coach defines the strategy,
  and the policy still decides how to execute it.

The game itself is inspired by Mario Strikers and Rocket League, plus a joke
about taking "armchair coaching" literally.

The project grew out of a prototype called Tactic Lab, so internal identifiers
still carry that name — the npm package (`tactic-lab`), the native crate
(`tactic-lab-native`), the window title, and the `TACTIC_LAB_*` environment
variables.

## Repository map

| Path | What it is |
| --- | --- |
| `native/coaching` | The Rust/Bevy client: lobby, tactics board, match. The app you run. |
| `native/cube-soccer` | The game — physics, players, superpowers, jungle world — plus the RL environment and Python bindings. |
| `native/player-creation` | The standalone 2D draw-your-player flow (the in-world version lives in `cube-soccer/src/creation`). |
| `native/cube-soccer/python` | PPO training, self-play, evaluation, and the tactic weight transplant. |
| `tools/monkeyforge` | The generative asset pipeline: sketch → spec → geometry → validated GLB, plus the superpower classifier. |
| `server/` | Node service: transcription, tactic interpretation, live shouts, LAN lobby, MonkeyForge bridge. |
| `src/domain` | Shared session/tactic types and the interpretation schema. |
| `ppo_cube_soccer_*_steps.zip` | Trained PPO checkpoints, 25.6M through 78.4M environment steps. |

## Run it

Install Rust 1.85+, Node.js 20+, and your platform's linker prerequisites.

```bash
npm install
cp .env.example .env
# DEEPGRAM_API_KEY (transcription + voices), OPENAI_API_KEY / OPENAI_MODEL
# (interpretation and shouts), MONKEYFORGE_PYTHON (superpower classifier)
npm run dev:native
```

The launcher starts the local credential-holding service and the native window;
no browser is involved. Draw your player, coach your tactic, then click **Next**
to watch the coached Orange team play a Balanced opponent in the jungle stadium.

The API can run on its own:

```bash
npm run dev:api     # 127.0.0.1:8787
```

Provider keys stay in the server process and are never sent to the native client.
A joining machine on the LAN redirects its API calls to the host, so a guest gets
the host's toolchain — Python, credentials and all — for free.

## The tactic, end to end

A coached tactic is resolved to seven numbers — defender depth, attacker push,
width, spacing, press, line height, commitment (`TacticParams` in
`native/cube-soccer/src/systems/heuristic_ai.rs`). That vector is the fixed
interface shared by three consumers:

1. **The interpretation route** (`server/openaiInterpretation.ts`) maps spoken
   coaching onto the shipped presets — Balanced, High Press, Low Block, Wing Play
   — with per-player overrides and blends.
2. **The on-pitch AI** (`native/cube-soccer/src/systems/soccer_ai.rs`) derives
   its play style from it. Every decision there is a pure function of the world
   and the tactic: no sampling, no learned component. A carrier shoots at the
   open part of the goal, passes to the best-placed teammate, clears when pinned
   in its own third, and dribbles with touches; supports run into open receiving
   positions; how many players press, from how far, and how high the line sits
   all come from the tactic. Matches are high-scoring by design — there is no
   goalkeeper.
3. **The RL observation** (`native/cube-soccer/src/rl/observation.rs`) appends
   the same seven numbers, normalized, to each agent's features. The coach sets
   the strategy; the policy still decides how to execute it.

`soccer_ai.rs` has tests that play simulated matches and assert that goals keep
coming, that the ball never leaves play for long, and that the tactic is visible
in where the players spend the match.

## Reinforcement learning

The agents learn **inside the real game**, not a stripped-down reimplementation
of it. `native/cube-soccer` is compiled twice from one source: once as the Bevy
app you play, and once — with its `python` feature, which `pyproject.toml` pins
for maturin — as a CPython extension module through PyO3
(`src/python/bindings.rs`). Training therefore runs the same rapier physics, the
same possession and kick rules, the same superpowers and status effects, and the
same goal detection that a human faces.

`src/rl/sim.rs` builds the headless app: Bevy's minimal plugin set, no renderer,
a fixed 30 Hz `PHYSICS_TIMESTEP` for throughput, and one `Update` schedule run
in a pinned order per step —

```text
apply_ai_actions      Orange takes the policy's actions
apply_heuristic_ai    Blue's inputs are overridden by the scripted opponent
apply_roster_gating   curriculum: ghost benched players on both teams
freeze_inactive_players
tick_superpower_cooldowns / activate_superpowers
tick_status_effects / apply_player_movement / apply_status_forces / clamp_velocities
tick_cooldowns -> update_possession
detect_goals_by_position -> handle_goal_headless
compute_step_rewards -> extract_observations
```

```bash
cd native/cube-soccer
maturin develop --release
PYTHONPATH=python python python/train_ppo.py --timesteps 78400000 --num-envs 16
```

### Observation and action spaces

Each agent sees **85 floats** (`OBSERVATION_SIZE = 18 + 12 * PLAYERS_PER_TEAM +
TACTIC_PARAMS`), built in `src/rl/observation.rs`:

| block | size | notes |
| --- | --- | --- |
| self position, velocity | 6 | normalized by field half-extents and max speed |
| each teammate: relative pos + vel | 24 | ascending index |
| each opponent: relative pos + vel | 30 | ascending index |
| ball: relative pos + vel | 6 | |
| distance to own / opponent goal | 2 | |
| score difference, time remaining | 2 | |
| possession flags | 3 | self / teammate / opponent has the ball |
| superpower one-hot + cooldown ready | 5 | the agents get powers, same as players |
| tactic params, normalized | 7 | the coach's directive |

Every coordinate is mirrored in x for Blue, so both teams see the pitch from
their own attacking direction and one policy is valid for either side.

Actions are 4 continuous values per agent — `move_x`, `move_z`, `jump`, `fire`
(the superpower) — so a full team is 20. `python/env.py` exposes three views of
this: a single-agent `CubeSoccerEnv`, a per-agent-dict `CubeSoccerMultiAgentEnv`,
and `CubeSoccerTeamEnv`, the one we actually trained: a **single "team brain"
policy** that emits all five Orange agents' actions at once from their
concatenated observations (425 in, 20 out) and receives the summed team reward.
Blue's action slice is left zero and overridden in-engine by the heuristic.

### The curriculum

Dropped straight into full 5v5, the policy got almost no learning signal — goals
are rare, and the credit assignment behind one is 300 seconds long. Four
schedules in `train_ppo.py` ramp the difficulty over roughly the first half of
training, each as a Stable-Baselines3 callback that reaches into the running
envs through `env_method`:

- **Roster** (`RosterCurriculumCallback`) — 1v1 grows to 5v5, both teams at
  once. Benched players are ghosted and frozen rather than removed, so the
  observation and action shapes never change and a *single* policy trains
  continuously across every roster size. The field scales with the roster
  (`field_scale` in `game/config.rs`), so 1v1 is played on a small pitch.
- **Goal width** (`GoalWidthCurriculumCallback`) — the scorable mouth starts at
  11 m half-width and narrows to a regulation 2.8 m. A wide goal lets crude,
  off-center pushes score early, so the policy sees +30 and anchors on real
  goals before precision is demanded.
- **Opponent** (`OpponentCurriculumCallback`) — the scripted heuristic
  (`systems/heuristic_ai.rs`, kept in the codebase precisely for this) ramps
  from 0.15 to full strength, so Blue starts nearly inert and hardens into the
  real thing.
- **Shaping and entropy** (`ShapingAnnealCallback`, `EntropyAnnealCallback`) —
  the dense shaping weight decays 1 → 0 over the first 70%, leaving the pure
  goal objective; `ent_coef` decays so the policy sharpens as the scales get
  harder instead of letting the action std run away.

We trained across **78.4 million environment steps**. Checkpoints at 25.6M,
38.4M, 60.8M and 78.4M are committed at the repository root.

### Rewards, and how agents cheat them

The reward (`src/rl/reward.rs`, constants in `game/config.rs`) is a team-shared
term plus per-agent terms:

```text
agent_reward = team_shared + crowding + approach + tactic shape_match
team_shared  = ±30 goal, ±5 match result, potential-based ball progress,
               a finishing ramp inside 6 m of the mouth
```

Everything dense is **potential-based**, so it telescopes: approaching the goal
pays, but camping in the attacking third nets zero over the episode.

The instructive failure: an early agent learned to shove the ball into a
**corner** and sit there. The shaping term measured distance to the goal
*center*, and a corner is closer to the center than midfield is — so cornering
the ball read as progress forever, without ever scoring. The fix is four lines
and it is the most important four lines in the file: measure distance to the
nearest point of the scorable **goal mouth** segment, not the center. A ball
pushed wide of the posts now accrues lateral distance, and the exploit pays
nothing. A small per-teammate `crowding_penalty` inside 3 m does the same job
for the other degenerate strategy — all five players piling onto the ball.

### Tactical conditioning

The point of steerable testers is that a developer can say "press higher" and
watch what that does to a new mechanic, without anyone hand-coding a "press
higher" behaviour. So the seven tactic parameters go **into the observation**,
and a bounded positional-imitation bump goes into the reward: `shape_match` is a
Gaussian, peaking at `TACTIC_WEIGHT` when an agent stands where its tactic
prescribes and decaying over 5 m. It is non-negative and bounded, so it rewards
matching the tactic without ever punishing — the policy reads the tactic in its
observation to predict where the bump is, and keeps full freedom over how to get
there. Tactics are randomized per episode during training
(`--randomize-tactics`), which is domain randomization over the coach.

**Keeping the 78M steps.** Adding seven inputs changes the policy's input layer,
so a trained checkpoint cannot simply be loaded into the new environment:
per-agent observations go 78 → 85, and the team vector 390 → 425.
`python/transplant_tactic.py` builds a fresh 425-input policy with the same
`[256, 256]` architecture and copies every matching weight across. The detail
that makes it work: the observation is **agent-major**, so the seven new columns
sit *inside* each agent's block, not appended to the end of the vector. Old
column `a*78 + f` maps to new column `a*85 + f`; a naive "copy the first 390,
zero the last 35" corrupts every agent after the first. The new columns start at
zero, which makes the transplanted policy behaviourally *identical* to the
scorer until training teaches it to modulate. All the motor skills survive; only
the tactical modulation is learned fresh, warm-started with
`--warm-start --roster-start-full --randomize-tactics`.

### Evaluating them

Two evals, because "can it score" and "does the coaching do anything" are
different questions:

```bash
PYTHONPATH=python python eval.py             # goals across rosters x opponent difficulty,
                                             # stochastic and deterministic
PYTHONPATH=python python tactic_fidelity.py  # does behaviour actually change per tactic
```

`tactic_fidelity.py` sets each shipped preset on Orange, rolls out the
deterministic policy at full 5v5 against a full-strength opponent, and checks
that three team statistics move in the direction the tactic promises: mean
advancement up the pitch (High Press > Balanced > Low Block), lateral spread
(Wing Play highest), and mean distance to the ball (High Press lowest). All
three are decoded straight out of the observation vector, so the eval needs no
extra bindings.

`python/self_play.py` keeps a pool of past checkpoints to play against, as an
alternative to the scripted opponent.

## The generative pipeline (MonkeyForge)

`tools/monkeyforge` turns a drawing into something the game engine will accept.
Five models do the creative work — **Qwen2-VL** to read the drawing, **SDXL**
with **ControlNet** to generate the visuals, **CLIP** to classify, and
**TRELLIS.2** for image-to-3D — and deterministic code does everything that is
not allowed to fail.

### Drawing → dressed 3D character

```text
sketch + description
  -> read the drawing        scripts/read_sketch_vlm.py     Qwen2-VL-2B     GX10
  -> resolve the garment     src/monkeyforge/garments.py                    local
  -> generate every sprite   scripts/gen_outfit.py          SDXL            GX10
  -> accessory -> mesh       scripts/image_to_glb.py        TRELLIS.2-4B    GX10
  -> project onto the body   scripts/project_planar.py                      local
  -> recover relief          scripts/garment_height.py                      local
  -> restyle the base        scripts/restyle_features.py                    local
  -> assemble, shell, socket, export   scripts/blender_assemble_character.py
  => character.glb + manifest.json + metrics.json
```

`scripts/forge_character.py` drives the whole thing. The result is a bundle, not
a raw mesh: hard validation rules (`validation.py`) plus a trainable pairwise
style ranker (`ranking.py`) stand between the model output and the game.

The body itself is hand-authored and animation-safe — the pipeline generates
only *bounded* customization: morphs, palette, hats, backpacks, held props, with
attachment points fixed by name (`SOCKET_HEAD_TOP`, `SOCKET_HAND_LEFT`,
`SOCKET_BACK`, …). `scripts/blender_build_original_base.py` builds team-owned
balanced / runner / defender / goalkeeper bodies.

### Drawing → superpower badge

The superpower takes the short path, because the game only has four powers and
mapping a sketch onto one of four known labels is **zero-shot**:

```text
src/monkeyforge/powers/registry.py    the four powers as data, pinned to the Rust enum
src/monkeyforge/powers/classify.py    CLIP zero-shot with prompt ensembling
src/monkeyforge/powers/sketchmask.py  drawing -> ControlNet line art, and -> subject alpha
src/monkeyforge/powers/beautify.py    SDXL + ControlNet subject generator (and a no-GPU stand-in)
src/monkeyforge/powers/badge.py       procedurally drawn badge frame and compositor
```

CLIP ViT-B/32 is small enough to run on a laptop CPU in a couple of seconds, so
`server/forge.ts` shells out to it directly and the game gets back a power plus
a confidence and a runner-up. `registry.py` asserts the power order at import,
because that order is written into the RL observation's one-hot — reordering it
would silently invalidate every PPO checkpoint. All four badge PNGs are
committed, so the HUD works offline and the round trip stays a classification
rather than an image build.

### Where the models run

The heavy models never touched a laptop. An **ASUS Ascent GX10 — NVIDIA GB10
Grace Blackwell**, 128 GB unified memory, 85.8 TFLOP/s fp16 — hosts SDXL,
ControlNet, Qwen2-VL and TRELLIS.2-4B behind a FastAPI worker
(`tools/monkeyforge/worker/`). The laptop talks to it over an HTTP contract
(`docs/gpu-worker-contract.md`), which is the seam that lets the CUDA
dependencies stay in their own image:

```dotenv
MONKEYFORGE_PROVIDER=http
MONKEYFORGE_GPU_ENDPOINT=https://your-worker.example/v1/image-to-3d
MONKEYFORGE_GPU_TOKEN=replace-me
```

Warm, an icon is ~16 s at 1024², 28 steps, SDXL + ControlNet. The default
development mode is fully local with a procedural stand-in, so a fresh clone
runs with no GPU at all.

The measured constraint on that box is **bandwidth, not capacity**: 240 GB/s
against an H100's 3.35 TB/s. 128 GB means a 4B model *fits*; it does not make it
fast.

### What the models taught us

- **Bigger models are not better pipelines.** Qwen2-VL got markedly more
  reliable when asked one narrow question about one part of a drawing at a time
  instead of being asked for a single large structured output.
- **Ask the model what it is good at.** On sleeve length, measured: CLIP split
  36/34 on a drawn t-shirt, and Qwen answered "long" for both a t-shirt *and* a
  suit. Both name the garment correctly — so sleeve length is now derived from
  the garment's name through a lookup table, not from looking at the arms.
- **Diffusion models cannot spell.** "MIT" came back as `3I`, `XJZ`, `VU`. Text
  and numbers are rendered with a font and composited, never prompted for.
- **Prompt structure and length matter more than prompt content.** A clause
  moved to the end of an SDXL prompt made it build a collage; CLIP silently
  truncates at 77 tokens, so anything at the end of a long prompt is simply
  gone. Both are now enforced by tests.
- **A prompt cannot supply identity the model never learned.** SD 1.5 does not
  know what a BTD6-style monkey is and produced a generic CGI mouse; that is an
  SDXL-or-LoRA problem, not a wording problem.
- **Inpainting preserves, text-to-image replaces.** If part of an image must
  survive untouched, inpainting copies everything outside the mask by
  construction.

The design principle the whole tool is built on: **models handle interpretation
and creativity; deterministic code enforces the constraints that cannot be
allowed to fail** — scale, mesh integrity, triangle budget, socket names,
collider contract. A beautiful mesh at the wrong scale is not an asset. The
game's `CubePlayerBundle` keeps its own collider, mass and locked axes; the
generated model is parented to it as appearance only, so no amount of
generative weirdness can change how a player physically behaves.

## Live voice coaching

During gameplay, hold **Hold to shout**, say "Player 4, get back!", and release
to send. Name a player by number (1–5, digits or words) and they answer; name
nobody and whoever looks up does. Your team's numbers float above their heads;
the player who answers is highlighted while their private, voiced reply plays.

Shouts are cosmetic — they never change tactics or movement. Recordings stop at
15 seconds. Losing window focus or leaving gameplay cancels capture, requests
and playback. A reply the model returns but that is unusable — empty, overlong,
malformed — falls back to "On it, coach!", while a provider that is actually
down still surfaces as an error, so a bad key stays visible instead of being
masked by a canned line.

`OPENAI_SHOUT_MODEL` optionally overrides `OPENAI_MODEL` for replies. Deepgram
provides English transcription and the five fixed Aura 2 voices. No shout
history is stored or broadcast. The server uses the
[OpenAI Responses API](https://developers.openai.com/api/docs/guides/text) and
[Deepgram raw linear16 output](https://developers.deepgram.com/docs/tts-media-output-settings).

A playable shout preview, skipping coaching:

```bash
npm run dev:shout
```

`SHOUT_PREVIEW_TEAM=blue` checks joiner numbering. `TACTIC_LAB_CAPTURE=/tmp/shout.png`
saves a gameplay screenshot and exits — leave it unset when playing.

## Multiplayer

Host/solo coaches Orange; a joiner on the LAN coaches Blue through the host's
service. `server/lobby.ts` holds each side's artifact bundle — session log,
tactical output, drawings — and releases the match when both coaches are ready.
Drawings are optional so a skipped creation flow can't deadlock a match.

## Verification

```bash
npm run typecheck
npm test
npm run build
cargo test --manifest-path native/coaching/Cargo.toml --lib -p cube-soccer -p tactic-lab-native
cargo build --manifest-path native/coaching/Cargo.toml --bin native-coaching
cd tools/monkeyforge && python -m pytest && python -m ruff check .
```

The normal suite uses mocks and loopback servers, with no provider requests.
Opt-in integration tests against the real OpenAI and Deepgram services:

```bash
npm run test:live
```

Interpretation failures show their reason with **Retry interpretation** and
**Continue anyway (Balanced)**; Balanced is never silently substituted.

Gameplay examples for looking at things in isolation:

```bash
cargo run --manifest-path native/cube-soccer/Cargo.toml --example ai_vs_ai
cargo run --manifest-path native/cube-soccer/Cargo.toml --example human_vs_ai
cargo run --manifest-path native/cube-soccer/Cargo.toml --example benchmark
```

## Who built what

- **Harry** — the reinforcement learning system: environment, reward design,
  training curriculum, and the steerable tactic-conditioned agents.
- **Caellum** — the generative pipeline turning drawings into polished
  characters, abilities and 3D game assets.
- **Jeremy** — frontend, visual design, the jungle environment, the tactical
  coaching interface, and the overall gameplay experience.
- **Chloe** — backend and integration: transcription, tactic interpretation,
  multiplayer, APIs, and the wiring between model services.

## What's next

Generalizing the loop past soccer: create a mechanic, generate its assets, and
immediately test it against a roster of AI player personas — eventually
stress-testing new mechanics for balance issues, exploits and unexpected
strategies before they ever reach real players.

## Credits

The original environment, game and RL bindings come from
[Aijo24's Cube Soccer 3D](https://github.com/Aijo24/Cube-soccer-3D).
