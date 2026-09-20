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
single Bevy application that moves through four phases, tracked by one
state enum:

```rust
// native/coaching/src/phase.rs
pub enum AppPhase {
    Lobby,     // main menu: Host / Join / Play Solo
    Creation,  // draw a player
    Coaching,  // record a tactic on the board
    Waiting,   // networked only: uploaded, waiting for the other coach
    Game,      // play the resulting minigame
}
```

Roughly:

```
Lobby     →  Player Creation  →  Coaching  →  Interpretation  →  [Waiting] →  Game
(Host/Join/   (draw a player)   (record a     (raw session      (networked   (3D soccer
 Solo menu)                      tactic)       → tactical JSON)   only)        minigame)
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
  `narrowmidblock`, or `alloutattack`. `teamId` is `"red"` or `"yellow"` — the
  server interprets whichever team's session it was asked to interpret via the
  `/api/interpret` request's `teamId` field (defaults to `"red"` for backward
  compatibility). Optional, unique overrides refer only to that coached team's
  own player IDs (1–5 for red, 6–10 for yellow), select another named preset,
  and cite real board/transcript evidence. The model must choose the closest
  supported preset with `best_match`, even for imperfect or mixed evidence.
  Balanced is allowed only when neutral shape actually fits, not as an
  uncertainty escape hatch. Weak evidence is retained as a visible
  low-confidence notice. Empty coaching produces a specific error.
  User-approved fallback selects Balanced with no overrides.
  On the Rust side, `game_handoff::CoachedTeam::from_output_for_team` parses
  and grounds one team's output (parameterized by `TeamSide::Red`/`Yellow`);
  `game_handoff::MatchHandoff` merges a red and a yellow `CoachedTeam` into
  the `TeamTactics` the game controller consumes. Today only red is coached
  through the live UI — `handle_enter_game` in `src/lib.rs` builds yellow via
  `CoachedTeam::balanced_default` until a second machine's real yellow output
  is available (see the "Multiplayer" section below).
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
  `game_handoff.rs::CoachedTeam::from_output_for_team` (red), then merges it
  with a yellow `CoachedTeam` into a `game_handoff.rs::MatchHandoff`. Invalid
  or stale data leaves the user in coaching with an explanation. The same
  output is saved/exported and used to construct the game directive.
- **Controller:** `Tactic::from_name()` → `Tactic::params()` →
  `TeamDirective::uniform()` / `set_player()` → `TeamTactics` →
  `apply_heuristic_ai`. Each update uses live player and ball state to compute
  movement. Tactic presets affect support shape, pressing, depth, width,
  spacing, line height, and attacking commitment; they do not replay paths.
- **Identity:** fixed 5-v-5; red IDs 1–5 map to Orange indices 0–4, yellow
  IDs 6–10 to Blue indices 0–4. Solo play: Orange uses the coached directive,
  Blue uses `CoachedTeam::balanced_default`. Multiplayer: both are real
  coached directives merged from host+joiner (see "4a. Multiplayer"). All
  non-spectator players receive `AiControlled`; a joiner spectates via
  `game_stream.rs` instead of running AI. No keyboard player movement system
  runs in this integrated match.
- **Lifecycle:** normal starting positions, tactics retained through goal and
  round resets, possession cleared on resets. AI runs before movement/power
  activation. Results and the game HUD show the coached preset and overrides.
- **Architecture:** recording and interpretation remain separate from the game
  adapter. The external JSON uses coaching IDs and labels, never Bevy entities
  or controller internals. Normalized board coordinates remain in the report;
  this version does not use them to place players in the game.

## 4a. Multiplayer

LAN host/join multiplayer: two people on the same wifi each coach their own
team on their own machine (host = red, joiner = yellow, fixed), then watch
one shared AI-vs-AI match. There is no live shared-board editing — each side
runs its own full Lobby → Creation → Coaching pipeline independently — and
the Game phase is host-authoritative: the host runs the real physics/AI
simulation and streams state to the joiner, which renders it as a spectator
rather than running its own simulation (this is what avoids cross-machine
simulation drift). Built in three phases, all complete:

- **Phase A — merge plumbing:** `teamId` on `/api/interpret` and
  `rlSelection` (`src/domain/interpret.ts`, `server/normalize.ts`,
  `server/openaiInterpretation.ts`), and `game_handoff.rs::CoachedTeam`
  (parameterized by `TeamSide::Red`/`Yellow`) + `MatchHandoff` (merges both
  teams' directives) in Rust. Both teams can now be driven by real coached
  output instead of yellow being hardcoded to Balanced.
- **Phase B — LAN transport & lobby:** `server/index.ts` binds `0.0.0.0`
  with CORS; `server/lobby.ts` holds one in-memory match slot with
  `/api/lobby/join`, `/api/lobby/ready`, `/api/lobby/start`, and a
  `/api/lobby` WebSocket that pushes connection status and, once both sides
  have posted a ready tactical output, the merged `{red, yellow}` payload
  (auto-starts the moment the second side posts). On the Rust side,
  `native/coaching/src/network.rs` adds `AppPhase::Lobby` (now the app's
  default/first phase) with a main menu (Host / Join / Play Solo),
  `NetworkRole`/`NetworkEndpoint` resources, `LobbyRuntime` (transient
  blocking HTTP calls for join/ready + one persistent WS listener, same
  shape as `speech.rs`'s `SpeechRuntime`), and best-effort mDNS
  advertise/browse via `mdns-sd` with manual `ip:port` entry always
  available as the reliable fallback (multicast on shared wifi is not
  trustworthy enough to be load-bearing for a live demo). A joiner's
  `/api/interpret` and `/api/transcribe` calls are redirected to the host by
  setting `TACTIC_LAB_API_URL`/`TACTIC_LAB_WS_URL` at connect time — both are
  already read fresh per-call, so no other code needed to change. The
  results screen's "Next"/"Continue anyway" buttons now fire a `MatchReady`
  event instead of `EnterGame` directly; `network.rs::handle_match_ready`
  branches on `NetworkRole` — solo behaves exactly as before, networked play
  submits to the lobby and waits for the server's merged push instead.
- **Phase C — game streaming:** `native/coaching/src/game_stream.rs`
  defines `GameSnapshot` (10 players' position/yaw, ball position,
  score, time remaining) and `GameStreamRuntime`, which is host-mode (runs
  its own `tokio-tungstenite` WS *server* on port `9010`, broadcasting to
  any connected joiner — a direct Rust↔Rust socket, not relayed through
  Node) or joiner-mode (WS client with auto-reconnect, buffering the latest
  snapshot for `apply_network_snapshot` to consume once per frame).
  `game.rs::GamePlugin` splits its systems by role: visual spawn systems
  (arena, players, ball, camera, jungle) run identically on both; the
  physics/AI/goal-detection chain is gated `.run_if(is_not_spectator)`;
  `configure_network_physics` sets Rapier's `physics_pipeline_active = false`
  for the joiner so it never steps local physics at all — it only ever
  hard-sets `Transform`/`GameState` from the network stream. Both host and
  joiner build the same `MatchHandoff` from the server's `MatchStart` push
  (`network.rs::build_match_handoff`), so `spawn_tactic_hud` needs no
  role-specific handling.

### Match start, the waiting phase, and artifact collection

Three bugs made a real LAN match impossible to finish; all are fixed, and the
tests below are written specifically to keep them fixed:

1. **The merge could never succeed.** `build_match_handoff` used to read the
   session id from the *red* output and validate *both* teams against it. The
   two machines coach separate sessions with separate UUIDs, so the yellow side
   always failed with "Interpretation belongs to another session" — meaning
   every networked match silently refused to start.
   `CoachedTeam::from_output_for_team` now reads each payload's session id
   *from that payload* and enforces internal self-consistency
   (`session.id == rlSelection.sessionId`) instead of cross-payload equality.
   Every other grounding check (taxonomy, canonical labels, per-team override
   rosters, evidence) is unchanged. `MatchHandoff.match_id` — assigned by the
   host's lobby — is now the only thing correlating the two sessions.
   `CoachedTeam::from_local_session` keeps the stricter same-session check for
   the solo path, where a stale result really is a bug.
2. **The failure was invisible, and so was the wait.** Submitting used to fire
   `SetCoachingActive(false)` while leaving `AppPhase` on `Coaching`, which
   disabled every coaching UI system and despawned the board — with no other
   UI registered for that state, the window rendered nothing but `ClearColor`
   (the reported "black screen"), and the merge error had nowhere to display.
   There is now an `AppPhase::Waiting` with `network.rs::waiting_screen_ui`:
   upload progress, both players' ready state, the match id, errors, and
   **Retry upload** / **Back to coaching** on failure. (`bevy_egui` needs no
   camera — the Lobby screen already proved that — so this was never a camera
   problem, purely a missing UI system.)
3. **`start` was broadcast exactly once.** A client whose lobby WebSocket was
   mid-reconnect at that instant would hang forever with the identical
   symptom. The server now replays `start` to any client that connects after
   the match has begun.

**Artifact collection for later model runs.** Both machines upload their full
session to the *host* before the match is allowed to begin, so all training
data ends up in one place:

```
output/matches/<match-id>/
├── match.json          (ids, teams, per-player file lists, timestamps)
├── host/               appearance.png superpower.png manifest.json
│                       session.json tactical-output.json
└── joiner/             (same five files)
```

- `POST /api/lobby/artifacts` takes the bundle (base64 PNGs, the raw session
  event log, the tactical output, the creation manifest, and both session ids
  — the player-creation UUID and the coaching UUID are unrelated, so the
  bundle carries both). PNGs are validated by magic bytes; the write root is
  `TACTIC_LAB_OUTPUT_DIR` or the repo's `output/`, resolved from the server
  file's own location rather than CWD.
- **The upload blocks the match**: `/api/lobby/ready` returns
  `409 ARTIFACTS_REQUIRED` until that side's bundle is stored, and the server
  only broadcasts `start` once both bundles *and* both tactical outputs are
  in. Losing a player's drawings to a race would cost data that can't be
  recovered after the fact.
- `ContinueToCoaching { session_id, manifest_path }` was previously discarded
  by the binary; it is now captured into `network.rs::CreationArtifacts`,
  which is what lets the coaching side find the drawings belonging to its
  session. `express.json`'s limit went from 1mb to 25mb to fit the bundle
  (which also fixes a latent 413 risk on `/api/interpret` for long sessions).

**Spectator fixes:** `update_ui`/`update_wall_scoreboard` were gated behind
`is_not_spectator`, so the joiner's scoreboard never reflected the streamed
score; they are display-only readers of `GameState` and now run for both
roles. `GameSnapshot` also carries `round_timer`, since the joiner never runs
`update_timers` and its clock would otherwise freeze. `spawn_tactic_hud` now
takes an `Option<Res<MatchHandoff>>` — as a hard `Res` it panicked mid-`OnEnter`
when the handoff was missing, which also skipped `start_game_stream` and left
the joiner with no stream at all.

**Lobby polish:** `PlayerCreationPlugin` gated its UI only on its own
`CreationFlow`, so the drawing toolbar rendered on top of the Host/Join/Solo
menu. It now takes a `CreationEnabled` resource that the coaching app switches
on only during `AppPhase::Creation`.

**Tested over real sockets** (not just unit-level): `game_stream.rs`'s
`joiner_receives_snapshots_published_by_a_real_host_over_loopback` runs the
actual host WS server and joiner WS client against each other over a real
loopback TCP socket. `network.rs`'s
`host_and_joiner_reach_match_start_with_separate_sessions` spawns a real
`server/index.ts` process and drives two independent `LobbyRuntime`s (the
exact production networking code) through join → both-connected → upload →
both-ready → merged `MatchStart`, then builds a real `MatchHandoff` — using
**two different session ids**, which is what the earlier version of this test
got wrong: it shared one fixture id between both sides and therefore could
never have caught the bug that broke every real match.
`artifact_bundles_land_on_the_host_before_the_match_starts` asserts the
on-disk `output/matches/<id>/{host,joiner}/` tree, byte-for-byte PNG equality
(catching base64 corruption), and that `match.json` records both session ids;
`ready_without_artifacts_never_starts_the_match` proves the blocking gate.
All of this runs the same code path a genuine two-machine LAN session uses,
addressed at `127.0.0.1` instead of a routed wifi address.

**Confirmed by a real end-to-end run** (two `native-coaching` processes, both
driven by hand through drawing → coaching → interpretation → match): the host
coached red into **Wing Play** and the joiner coached yellow into **All-Out
Attack with two per-player overrides** (players 6 and 7 — correctly scoped to
the yellow 6–10 roster), both `model-backed`, under two different coaching
session ids. Both windows rendered the same match with the same match id in
the HUD, the host simulating and the joiner rendering the stream. All ten
artifacts landed on the host under one
`output/matches/match-<uuid>/{host,joiner}/` tree, with real 24–57KB PNGs and
a `match.json` recording both players' creation and coaching session ids.

What this does **not** cover: mDNS multicast behavior on real,
possibly-restrictive venue wifi (build around the manual IP fallback for a
live demo), and true cross-machine latency/packet loss — both processes in
this run were on one machine over loopback. Verify those on two physical
machines before relying on them live.

### Running a two-machine match

There is no packaged/distributable build yet — "joining" means running this
same native Bevy binary from source, not opening a URL or installer. Both
machines need the repo checked out at a commit with matching protocol code
(the lobby JSON shapes, `tacticalOutputSchema`, and `GameSnapshot` wire
format have no version negotiation — a mismatch fails validation rather than
degrading), a working Rust 1.85+ toolchain, and node/npm installed if using
`npm run dev:native`. Only the **host** needs `OPENAI_API_KEY`/
`DEEPGRAM_API_KEY` configured — the joiner's `/api/interpret` and
`/api/transcribe` calls get redirected to the host's server the moment they
connect (`network.rs::redirect_env_to_host`), so its own keys are never used.

**Host:**
1. `npm run dev:native` (starts the local Node service *and* the native app
   together; see `scripts/dev-native.mjs`). This binds the service to
   `0.0.0.0` and logs a LAN-reachable address (`server/index.ts`).
2. In the Lobby main menu, click **Host Match**. The screen shows the LAN
   address to share and waits for a joiner; it also announces over mDNS.

**Joiner:**
1. Skip `npm run dev:native` — it would start a redundant, unused local
   server. Run the native binary directly instead:
   ```
   cargo run --manifest-path native/coaching/Cargo.toml --bin native-coaching
   ```
2. In the Lobby main menu, click **Join Match**. If mDNS discovery finds the
   host it's listed as a clickable option; otherwise type the host's
   `ip:port` (as shown on the host's screen) into the manual field and click
   **Connect**.

Once connected, both sides proceed through their own independent
Creation → Coaching pipeline for their own team (host=red, joiner=yellow).
The match starts automatically for both the moment each side finishes and
submits its tactical output — see "4a. Multiplayer" above for what happens
after that.

## 5. Integration from main: implemented and remaining

Source: `origin/main` commit `f0026d8e68bd1cbec675b4751b4c1ecbe31416ec`.
This is a source integration into `native/cube-soccer`, not a claim that the
entire main branch was merged into the coaching branch.

| Functionality | Status in the combined app |
|---|---|
| Ten named tactical presets and per-player directives | Wired to coaching JSON and active during gameplay. |
| Indexed players and team identity | Integrated; main's 1-v-1 constant changed to fixed 5-v-5. |
| Heuristic movement, team support roles, pressing and spacing | Active for both teams. In solo play, red/Orange is coached and yellow/Blue stays Balanced; in LAN multiplayer, both are real coached directives (host=red, joiner=yellow). |
| Possession, movement, velocity limits, scoring and resets | Integrated into the native game lifecycle. |
| Superpower activation, cooldowns and status-effect systems | Registered, but no drawing-to-power assignment exists; players without a `Superpower` component have no ability. |
| Jungle visuals and camera | Preserved from the previous local game, including its larger arena/goal dimensions and ball CCD. Main's movement/gravity remain the gameplay basis. |
| RL observations/actions, headless simulation and Python bindings | Source included in the game crate; not used as the controller for the combined app. |
| PPO training/evaluation Python scripts | Included as upstream source, not launched by the app. No trained checkpoint or native policy-inference bridge is supplied. These scripts are not verified as part of the coaching demo. |
| Tactic blends and arbitrary numeric parameter APIs | Available in upstream controller code; not exposed in coaching JSON for this phase. |
| Drawing appearance/superpower artifacts | Still saved; not consumed to generate game appearance or assign powers. |
| Board-position initialization, timed phase execution | Not implemented in this phase. |
| Coaching both teams | Implemented via LAN multiplayer (see "4a. Multiplayer") rather than one person coaching both sides in a single session. |

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

### Quick handoff summary for the person working on main

We integrated main's gameplay code from commit `f0026d8` into the coaching
app. **What runs today is your heuristic tactical AI, not a trained PPO
model.** Coaching already affects player behavior through the ten supported
tactic presets and per-player overrides:

`Board actions + speech → coaching JSON → TeamTactics → heuristic player behavior`

**Currently wired in:** fixed 5-v-5 players, tactical AI, movement, possession,
scoring, and resets, with the jungle visuals preserved. Superpower systems
are included, but the user's drawing does not yet assign a power. In solo
play, red/Orange follows the coaching and yellow/Blue stays Balanced. The
subsequent LAN multiplayer work allows each side to supply coached directives;
the host runs the simulation and the joiner renders streamed state.

**Missing from the RL connection:**

- Observation/action code, the headless environment, Python bindings, and
  training/evaluation scripts are included, but the live game does not call a
  trained policy to choose its actions.
- No saved model checkpoint was committed in the inspected main revision;
  training code and logs were present. This says nothing about checkpoints,
  inference code, or newer changes you may have locally or elsewhere.
- The inspected PPO inputs do not include coaching/tactic labels. Loading a
  model alone therefore would not make it respond to the coaching JSON.
- That main revision defaults to 1-v-1; this app uses 5-v-5. We need your actual
  training configuration before assuming the checkpoint is compatible.

**Please confirm what you have locally:**

1. Do you have a trained checkpoint, or is training still in progress?
2. Does the policy control one player or a whole team, and was it trained for
   1-v-1 or 5-v-5?
3. Do you already have code running model predictions inside the rendered game?
4. Does the model accept tactical instructions, or are there separate models
   for different tactics?
5. Which loading script, dependencies, normalization files, and unpushed
   commits should accompany the checkpoint?

The main missing connection is **live game state → trained model → player
actions**, plus a defined way for **coaching to influence that model**.
The next section lists the concrete code boundaries for adding this later.

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
| Game | `native/coaching/src/game.rs` | `game_handoff.rs` (`CoachedTeam`/`MatchHandoff`: JSON → directives), `game_stream.rs` (host/joiner state streaming), tracked `native/cube-soccer`, `AppPhase::Game` transition in `src/lib.rs` |
| Multiplayer lobby | `native/coaching/src/network.rs` | `NetworkRole`/`NetworkEndpoint`/`LobbyRuntime`, `LobbyPlugin` (main menu UI), `server/lobby.ts` (join/ready/start + WS push) |
| Local service | `server/index.ts` | `app.ts` (`/api/interpret`, `/api/health`), `transcription.ts` (`/api/transcribe` → Deepgram), `lobby.ts` (multiplayer coordination) |

## Verification of this integration

- TypeScript build and 40 deterministic tests cover output labels, override grounding
  (including yellow-team assembly and roster-scoped override rejection), the lobby
  join/ready/auto-start protocol, the artifact upload (files written, PNG validation,
  stale match ids), and the `ARTIFACTS_REQUIRED` gate.
- Two live model tests cover High Press and Low Block through the updated schema/API.
- 35 Rust tests in the coaching crate (111+ across the game and coaching crates
  combined) cover the production game plugin starting ten AI players from a
  synthetic interpretation, player movement, goal/round resets, rejecting
  stale/invalid handoffs, merging two independently-sessioned teams, landing on
  the waiting phase instead of a blank screen, and — over real sockets, not
  mocks — a host and joiner completing the full lobby handshake plus artifact
  upload against a real spawned server, and a joiner receiving live game
  snapshots from a real host WS server.
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
