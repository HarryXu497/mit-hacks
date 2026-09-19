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

Each arrow below is currently a real seam, and two of them are places
where data does *not* yet flow through — worth knowing before you assume
something is wired up.

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
  schemaVersion, taxonomyVersion, interpretationMode,
  session: { id, title, durationMs },
  teams: [{ id: "red"|"yellow", playerIds }],
  classification, summary: { name, objective },
  steps: [{ id, startMs, endMs, primaryTactic, instruction, objective,
            actors, movements, annotationIds,
            evidence: { eventIds, transcriptSegmentIds } }],
  finalState: { players, ball, annotations },
  rlSelection: { schemaVersion, taxonomyVersion, sessionId,
                 primaryTactic, downstreamValue: "Balanced"|"HighPress"|
                 "LowBlock"|"Wide", selectionReason, evidenceStrength }
}
```

- **How it's produced**: the native client POSTs the raw `Session` JSON
  to the local `/api/interpret` route. The server (`server/app.ts` →
  `server/openaiInterpretation.ts`) constrains an LLM to emit only
  *semantic* judgments (tactic classification, instructions, objectives —
  never coordinates or IDs), then `server/normalize.ts` grounds that
  output against the real session data (rejecting any invented event or
  transcript ID) and assembles the final `tacticalOutputSchema` object.
  If the service is unavailable, both the TS side (`interpretSession()`
  in `src/domain/interpret.ts`) and the Rust side
  (`native/coaching/src/interpretation.rs`) have a deterministic
  fallback that produces a structurally-compatible payload without an
  LLM.
- **`rlSelection`**: worth flagging specifically — this field looks like
  the intended hook for a reinforcement-learning consumer (a compact
  `downstreamValue` enum plus the reasoning behind it), but today it's
  always populated with fallback values (`primaryTactic: "balanced"`,
  `selectionReason: "system_fallback"`), never genuinely model- or
  RL-derived. If you're building the RL portion, this is a plausible
  place to plug in, but it isn't live yet.
- A round-trip fixture lives at `fixtures/session-v1.json`, used by both
  the Rust and TypeScript test suites — useful as a concrete example
  payload if you want to test against the contract without running the
  full app.

## 4. Game portion (`native/coaching/src/game.rs`)

`GamePlugin` is a thin Bevy-plugin wrapper: almost all of its systems
(`spawn_arena`, `spawn_players`, `spawn_ball`, movement, scoring, camera,
UI) are imported from a separate `cube_soccer` crate, not written in this
crate. `native/coaching/Cargo.toml` currently points that dependency at
a path (`../../.claude/worktrees/jungle-soccer`) — a local worktree, not
something published or version-pinned.

- **Entering the phase**: `EnterGame` is a plain Bevy event, fired only
  from a UI button after tactical interpretation results are shown
  (`src/ui.rs::result_view`). `handle_enter_game` in `src/lib.rs` reacts
  by deactivating coaching and setting `AppPhase::Game`.
- **Current gap**: this transition is a bare state-machine switch.
  **No tactical/interpreted data is passed into the game phase.** The
  `TacticalResult` resource (the interpreted JSON) stays scoped to the
  coaching module; `GamePlugin`/`cube_soccer` never reads it. The game
  starts with the same hardcoded arena/setup regardless of what was
  coached. If your work is about tactics or RL behavior actually
  affecting gameplay, this is the gap to close — there's currently no
  code moving `TacticalOutput` (or `rlSelection` specifically) into the
  game phase at all.

## Where to look next

| Portion | Entry point | Key files |
|---|---|---|
| Player drawing | `native/player-creation/src/bin/native-player-creation.rs` | `state.rs` (flow states, `ContinueToCoaching`), `persistence.rs` (output paths), `input.rs` (coordinate mapping) |
| Coaching session | `native/coaching/src/bin/native-coaching.rs` | `model.rs` (event/domain types), `session.rs` (recording), `replay.rs` (log → board state), `speech.rs` (transcription) |
| Tactical JSON | — (library code, both languages) | `src/domain/interpret.ts` (`tacticalOutputSchema`, canonical contract), `server/openaiInterpretation.ts` + `server/normalize.ts` (production), `native/coaching/src/interpretation.rs` (client + Rust fallback) |
| Game | `native/coaching/src/game.rs` | `Cargo.toml` (`cube-soccer` path dependency), `AppPhase::Game` transition in `src/lib.rs` |
| Local service | `server/index.ts` | `app.ts` (`/api/interpret`, `/api/health`), `transcription.ts` (`/api/transcribe` → Deepgram) |
