# Agent Guide: Tactic Lab Repo Structure

This document is written for an AI agent (working on this branch, or on a
different branch/fork that needs to integrate with this one) who hasn't
seen this codebase before. It explains what the pieces are, how data
flows between them, and where the current seams and gaps are — so you can
figure out where your own work plugs in without re-deriving all of this
from source first.

For the hard rules and constraints this repo operates under (fixed 5v5
roster, normalized 0–1 coordinates, architectural boundaries, what counts
as "done"), see `AGENTS.md` at the repo root. This document doesn't repeat
those — it's purely descriptive: what exists today, and how it fits
together.

## The pipeline, at a glance

The native desktop app (`native/coaching/src/bin/native-coaching.rs`) is a
single Bevy application that moves through three phases, tracked by one
state enum:

```rust
// native/coaching/src/phase.rs
pub enum AppPhase {
    Creation,  // draw a player
    Coaching,  // record a tactic on the board
    Game,      // play the resulting minigame
}
```

Roughly:

```
Player Creation  →  Coaching  →  Interpretation  →  Game
 (draw a player)   (record a     (raw session      (3D soccer
                     tactic)      → tactical JSON)   minigame)
```

The coaching → interpretation → game handoff is wired through a validated
controller selection. Drawing artifacts are still saved but are not consumed
by the game.

## 1. Player drawing (`native/player-creation/`)

One user draws exactly one player: an "Appearance" canvas, then a
"Superpower" canvas (both 1024×1024, freehand strokes), then a review
screen, then clicks "Continue to coaching". There's no roster or
multi-player flow here — that was removed; the crate is scoped to a
single player per session.

- **Inputs**: pointer drag/click events and keyboard shortcuts via
  `bevy_egui`, translated into stroke commands in `src/input.rs`
  (`canvas_point`/`clamped_canvas_point` produce a normalized 0.0–1.0
  point *local to the drawing canvas*). This normalization is
  independent of the coaching board's — same 0–1 convention, but no
  shared type, so don't assume a canvas point can be passed directly into
  coaching's `Point` type.
- **Outputs**: two PNGs and a manifest, written atomically to
  `output/player-creations/<session_id>/{appearance.png,superpower.png,manifest.json}`
  (`src/persistence.rs`), plus a `ContinueToCoaching { session_id,
  manifest_path }` Bevy event fired from `src/ui.rs` when the user
  finishes.
- **Handoff**: `bin/native-coaching.rs` wires `PlayerCreationPlugin`
  before `CoachingPlugin` and reacts to `ContinueToCoaching` by moving
  `AppPhase` to `Coaching`. The plugin itself never touches `AppPhase` —
  the binary is what bridges the two.
- **Current gap**: nothing in `native/coaching` reads the manifest or the
  PNGs. The hand-drawn appearance/superpower art isn't consumed by the
  board or the game yet — if your work is meant to use that art (e.g. as
  a texture, or as input to a model), that read path doesn't exist and
  you'd be adding it.

## 2. Coaching session (`native/coaching/`)

A Bevy + `bevy_egui` app. The core idea: everything the coach does —
moving players/ball, drawing arrows, speaking — gets appended as one
flat, timestamped, ordered event log, so it can be replayed, scrubbed,
and later interpreted as a whole.

- **Domain model** (`src/model.rs`): `RawSessionEvent` is a tagged enum —
  `RecordingStarted/Resumed/Stopped`, `EntityMoved{entity, from, to,
  path, started_at_ms}`, `AnnotationAdded`/`AnnotationRemoved`, `Undo`,
  `TranscriptAdded`/`TranscriptEdited` — every variant carries its own
  `timestamp_ms` on one shared clock. `Point{x,y}` is normalized 0..1 and
  validated (`is_normalized`); `Session::validate_contract()` rejects
  out-of-range values.
- **Fixed roster**: red players are IDs 1–5, yellow 6–10, always — not
  configurable. `BoardState::default()` hardcodes starting positions for
  all ten plus the ball.
- **Recording** (`src/session.rs`, `src/board.rs`): `CoachingSession`
  owns the live `Session` and its clock; board drags and annotation
  strokes call `session.append(...)`. `src/replay.rs::replay_session`
  folds the event log (respecting `Undo`) into a `BoardState` + transcript
  list for both playback and interpretation.
- **Speech** (`src/speech.rs`): captures mic audio via `cpal`, streams
  24kHz mono PCM16 over a WebSocket to a local Node service, which
  proxies it to Deepgram's streaming transcription API and relays
  `partial`/`final` transcript events back. Final segments land as
  `TranscriptAdded` events on the same session timeline as movement and
  annotations — this is the "board actions and speech as one synchronized
  session" that `AGENTS.md` describes.
- **Local service** (`server/`): an Express + `ws` process run alongside
  the native app (`npm run dev:native` starts both; `npm run dev:api`
  runs just the service). Surface: `GET /api/health`, `POST
  /api/interpret` (session → tactical JSON), `WS /api/transcribe`
  (mic audio → Deepgram). Env vars: `DEEPGRAM_API_KEY`/`DEEPGRAM_MODEL`
  for transcription, `OPENAI_API_KEY`/`OPENAI_MODEL` for interpretation.
  Credentials live only in this Node process, never in the native binary.

## 3. Tactical JSON payload

The canonical external contract — what `AGENTS.md` means by "treat the
tactical JSON as an external interface" — is `tacticalOutputSchema` in
`src/domain/interpret.ts` (Zod; exported TS type `TacticalOutput`). This
is the shape any downstream consumer (game, RL policy, analytics, etc.)
should target, not the internal `RawSessionEvent` log.

Shape, roughly:

```
{
  schemaVersion: "2.0", taxonomyVersion: "tactics-v2", interpretationMode,
  session: { id, title, durationMs },
  teams: [{ id: "red"|"yellow", playerIds }],
  classification, summary: { name, objective },
  steps: [{ id, startMs, endMs, primaryTactic, instruction, objective,
            actors, movements, annotationIds,
            evidence: { eventIds, transcriptSegmentIds } }],
  finalState: { players, ball, annotations },
  rlSelection: { schemaVersion, taxonomyVersion, sessionId,
                 teamId: "red", primaryTactic, downstreamValue,
                 selectionReason, evidenceStrength,
                 playerOverrides: [{ playerId, tactic, evidence }] }
}
```

- **How it's produced**: the native client POSTs the raw `Session` JSON
  to the local `/api/interpret` route. The server (`server/app.ts` →
  `server/openaiInterpretation.ts`) constrains an LLM to emit only
  *semantic* judgments (tactic classification, instructions, objectives —
  never coordinates or IDs), then `server/normalize.ts` grounds that
  output against the real session data (rejecting any invented event or
  transcript ID) and assembles the final `tacticalOutputSchema` object.
  Failures leave the native client in `Failed` with no output or game transition.
  The failure panel displays the error code, HTTP/provider reason and request ID
  when available, plus **Retry interpretation** and **Continue anyway (Balanced)**.
  Only that explicit continue action creates a deterministic Balanced payload
  and enters the game. Deterministic generators remain available for previews/tests.
- **`rlSelection`** is the game handoff. `primaryTactic` and `downstreamValue`
  use the same canonical label: `balanced`, `highpress`, `gegenpress`,
  `lowblock`, `parkthebus`, `counterattack`, `possession`, `wingplay`,
  `narrowmidblock`, or `alloutattack`. The server interprets the red team's
  intended behavior. Optional, unique overrides refer only to red IDs 1–5,
  select another named preset, and cite real board/transcript evidence.
  The model must choose the closest supported preset with `best_match`, even
  for imperfect or mixed evidence. Balanced is allowed only when neutral shape
  actually fits, not as an uncertainty escape hatch. Weak evidence is retained
  as a visible low-confidence notice. Empty coaching produces a specific error.
  User-approved fallback selects Balanced with no overrides.
- **Versions:** new tactical output and selections use `2.0` / `tactics-v2`;
  raw sessions and `/api/interpret` request envelopes remain `1.0`. The native
  handoff accepts legacy v1 selections (`Balanced`, `HighPress`, `LowBlock`,
  `Wide`), translating `Wide` to `wingplay`. Unknown labels/versions fail
  validation instead of silently changing the tactic.
- A round-trip fixture lives at `fixtures/session-v1.json`, used by both
  the Rust and TypeScript test suites — useful as a concrete example
  payload if you want to test against the contract without running the
  full app.

## 4. Game portion (`native/coaching/src/game.rs`)

`GamePlugin` integrates the tracked `native/cube-soccer` crate into the Bevy
`AppPhase` lifecycle. The dependency no longer relies on a `.claude` worktree.
The crate combines main's gameplay foundation with the existing jungle,
monkey-player presentation, field decoration, lighting, and broadcast camera.

- **Handoff:** the results UI fires `EnterGame`. The handler requires a ready,
  current interpretation and validates its selection through
  `game_handoff.rs::GameHandoff`. Invalid or stale data leaves the user in
  coaching with an explanation. The same output is saved/exported and used
  to construct the game directive.
- **Controller:** `Tactic::from_name()` → `Tactic::params()` →
  `TeamDirective::uniform()` / `set_player()` → `TeamTactics` →
  `apply_heuristic_ai`. Each update uses live player and ball state to compute
  movement. Tactic presets affect support shape, pressing, depth, width,
  spacing, line height, and attacking commitment; they do not replay paths.
- **Identity:** fixed 5-v-5; red IDs 1–5 map to Orange indices 0–4, yellow
  IDs 6–10 to Blue indices 0–4. All ten players receive `AiControlled`.
  Orange uses the coached directive, Blue uses Balanced. No keyboard player
  movement system runs in this integrated spectator match.
- **Lifecycle:** normal starting positions, tactics retained through goal and
  round resets, possession cleared on resets. AI runs before movement/power
  activation. Results and the game HUD show the coached preset and overrides.
- **Architecture:** recording and interpretation remain separate from the game
  adapter. The external JSON uses coaching IDs and labels, never Bevy entities
  or controller internals. Normalized board coordinates remain in the report;
  this version does not use them to place players in the game.

## 5. Integration from main: implemented and remaining

Source: `origin/main` commit `f0026d8e68bd1cbec675b4751b4c1ecbe31416ec`.
This is a source integration into `native/cube-soccer`, not a claim that the
entire main branch was merged into the coaching branch.

| Functionality | Status in the combined app |
|---|---|
| Ten named tactical presets and per-player directives | Wired to coaching JSON and active during gameplay. |
| Indexed players and team identity | Integrated; main's 1-v-1 constant changed to fixed 5-v-5. |
| Heuristic movement, team support roles, pressing and spacing | Active for both teams; red/Orange is coached and yellow/Blue stays Balanced. |
| Possession, movement, velocity limits, scoring and resets | Integrated into the native game lifecycle. |
| Superpower activation, cooldowns and status-effect systems | Registered, but no drawing-to-power assignment exists; players without a `Superpower` component have no ability. |
| Jungle visuals and camera | Preserved from the previous local game, including its larger arena/goal dimensions and ball CCD. Main's movement/gravity remain the gameplay basis. |
| RL observations/actions, headless simulation and Python bindings | Source included in the game crate; not used as the controller for the combined app. |
| PPO training/evaluation Python scripts | Included as upstream source, not launched by the app. No trained checkpoint or native policy-inference bridge is supplied. These scripts are not verified as part of the coaching demo. |
| Tactic blends and arbitrary numeric parameter APIs | Available in upstream controller code; not exposed in coaching JSON for this phase. |
| Drawing appearance/superpower artifacts | Still saved; not consumed to generate game appearance or assign powers. |
| Board-position initialization, timed phase execution, coaching both teams | Not implemented in this phase. |

**Terminology:** the running controller is heuristic tactical AI, not a trained
PPO policy. The `rlSelection` field name is retained for continuity. Main's
PPO policy interface consumes numerical observations and emits actions; it
has no coaching-label input. Source inclusion does not mean that learned
inference is wired up. A separately trained model can be integrated later
using the handoff notes below.
With this build's five players per team, the observation layout is 78 floats
per player (390 per team) and actions are four floats per player (20 per team).
A checkpoint trained with main's one-player-per-team dimensions would not be
compatible without additional work.

Launch with `npm run dev:native`; it starts the local API and the native app.
No external worktree or Python process is needed for the coaching/game flow.

### Adding the separately trained model later

The user reports that another teammate may be training a model separately.
No trained checkpoint is committed in the inspected main revision; this does
**not** mean no model exists elsewhere. Main ignores `models/`, `checkpoints/`,
and several model-file extensions, so obtain the artifact from its owner.
The current app remains fully usable with heuristic tactical AI until a
compatible learned controller is explicitly connected.

**Request from the model owner:**

- The checkpoint (for example, an SB3 PPO `.zip`), loading/inference example,
  required Python/library versions, and any observation-normalization statistics.
- The training commit and environment configuration: team size, whether the
  policy controls one player or a whole team, observation feature order/scaling,
  agent ordering, action format, decision frequency/action repeat, and physics/
  superpower settings. Confirm compatibility with this app's five-player teams;
  do not assume the model being trained uses main's one-player default.
- Whether/how the policy accepts coaching: tactic labels, numeric conditioning,
  or separate checkpoints per tactic. The current upstream PPO observation has
  no tactic input. Merely loading a model will not make it obey `rlSelection`.

**Integration points (future work, not implemented switches):**

1. Keep the coaching JSON and `native/coaching/src/game_handoff.rs` as the
   interpretation boundary. Add a policy controller at the game layer in
   `native/coaching/src/game.rs`; do not send raw coaching events to the model
   or replace recording/interpretation with inference.
2. Build live observations using `native/cube-soccer/src/rl/observation.rs`
   (`get_observations` / `compute_observations`). Match the training preprocessing
   exactly, including team-relative coordinates and any saved normalization.
   This build exposes 78 floats per player, or 390 for the Orange team policy;
   confirm the supplied checkpoint's actual input shape before loading it.
3. Run the supplied policy at its trained decision cadence, then adapt its
   outputs through `AIActions` / `PlayerAction` in
   `native/cube-soccer/src/input/ai_controller.rs`, before movement. Each player
   has four actions: `move_x`, `move_z`, `jump`, `fire`; the Orange team output
   is 20 floats. Preserve red IDs 1–5 → Orange indices 0–4.
4. Give each player exactly one controller. Keep Blue on the Balanced heuristic
   if only Orange is learned. The existing `apply_ai_actions` writes to every
   indexed player, so scope the new adapter to policy-controlled players;
   exclude those players from `apply_heuristic_ai` to avoid overwriting actions.
5. Wire coaching into the policy only according to the training contract agreed
   with its owner. If it was not conditioned on tactics, decide that integration
   explicitly before replacing the heuristic; otherwise coaching could stop
   affecting behavior. Keep the heuristic as an explicit selectable fallback
   and report missing/incompatible checkpoints visibly.

Before enabling the learned controller, verify a known observation/action
example against the owner's inference script, validate shapes and finite action
values, and smoke-test movement, game resets, and the intended coaching effect.
The checkpoint, inference runtime/bridge, controller selection, and coaching
conditioning connection are all still to be added; these notes document the
existing connection points rather than claiming a drop-in switch exists.

## Where to look next

| Portion | Entry point | Key files |
|---|---|---|
| Player drawing | `native/player-creation/src/bin/native-player-creation.rs` | `state.rs` (flow states, `ContinueToCoaching`), `persistence.rs` (output paths), `input.rs` (coordinate mapping) |
| Coaching session | `native/coaching/src/bin/native-coaching.rs` | `model.rs` (event/domain types), `session.rs` (recording), `replay.rs` (log → board state), `speech.rs` (transcription) |
| Tactical JSON | — (library code, both languages) | `src/domain/interpret.ts` (`tacticalOutputSchema`, canonical contract), `server/openaiInterpretation.ts` + `server/normalize.ts` (production), `native/coaching/src/interpretation.rs` (client + Rust fallback) |
| Game | `native/coaching/src/game.rs` | `game_handoff.rs` (JSON → directives), tracked `native/cube-soccer`, `AppPhase::Game` transition in `src/lib.rs` |
| Local service | `server/index.ts` | `app.ts` (`/api/interpret`, `/api/health`), `transcription.ts` (`/api/transcribe` → Deepgram) |

## Verification of this integration

- TypeScript build and 29 deterministic tests cover output labels and override grounding.
- Two live model tests cover High Press and Low Block through the updated schema/API.
- 111 Rust tests cover the game and coaching crates, including the production
  game plugin starting ten AI players from a synthetic interpretation, player
  movement, goal/round resets, and rejecting stale/invalid handoffs.
- The native app build and the game crate's examples/all-targets check pass.
- `native/coaching/examples/coached_match.rs` renders a synthetic coached match
  through the production adapter/plugin without touching saved sessions or
  calling external services. Run with `-- highpress` (or another canonical label).
- Rendered native creation/coaching screens and the synthetic 5-v-5 jungle match
  were visually checked; the match HUD showed High Press, Balanced opposition,
  and player 2’s Low Block override. Saved coaching data was left unchanged.

### Demo-flow cleanup TODO

Once integration is stable, remove the verbose developer error/low-confidence
warnings from the demo presentation (including codes and request IDs). Keep
full diagnostics in server logs and preserve clear failure/retry behavior and
an explicit **Continue anyway (Balanced)** choice. Do not reintroduce a silent
Balanced fallback. For now the detailed warnings intentionally remain visible.

The server makes one 20-second model attempt; the native request allows 30
seconds so the server's detailed timeout can arrive. Provider authentication,
access/model errors, quota/rate limits, connectivity, incomplete/refused output,
and schema/grounding failures have distinct actionable diagnostics. API key
material is redacted from provider messages before display/logging.
