# Handoff: Next button into unmodified jungle scene

Read this before changing anything. You do not need prior chat history.

## What this repo is

Hackathon app: a coach moves 10 tokens + a ball on a 2D board while speaking. The session becomes tactical JSON.

- **This checkout:** `/Users/chloehouvardas/Documents/CODE/hack-mit-2026`
- **Git remote:** whatever `origin` is (GitHub). Working branch: **`coaching`**
- **Primary app:** Bevy 0.13 native client at `native/coaching` (`cargo run --bin native-coaching`)
- **Local API:** Node/Express in `server/` — transcription + `POST /api/interpret`
- **Run both:** `source "$HOME/.cargo/env"` then `npm run dev:native` (script is `scripts/dev-native.mjs`)
- **Env:** copy `.env.example` → `.env` with `OPENAI_API_KEY` and `OPENAI_MODEL`. Never commit `.env`.

Architecture rules are in `AGENTS.md`. Demo is a fixed 5-v-5 (red 1–5, yellow 6–10), normalized coords 0–1. Tactical JSON is an external interface; this repo does not own its consumers.

## Goal that is NOT finished

After Generate JSON, **Next** should hide the coaching board and, **in the same Tactic Lab window**, show the unmodified Canopy Clash jungle stadium from branch **`origin/codex/jungle-soccer-visuals`**.

```
coaching board → Generate JSON → Next → jungle scene (same window)
```

No tactic injection. No second process. No Cube Soccer 3D AI-cubes demo.

## What is already done

1. Native coaching board compiles and runs. First-frame wgpu crash is fixed: `update_board_camera` in `native/coaching/src/board.rs` clamps the camera viewport to the window physical size (egui’s first-frame screen rect is 10000×10000).
2. Egui `CentralPanel` is **transparent** so the Bevy pitch shows through (`native/coaching/src/ui.rs`).
3. Toolbar / Generate JSON / Add note layout is readable.
4. Wrong Cube Soccer 3D handoff was removed (`handoff.rs`, `scripts/play-tactic.mjs`, `npm run play`, smoke bin, “Kick off match as …” button). Do not bring those back.
5. After JSON, a green **Next** button sends `EnterGame` (`native/coaching/src/lib.rs` + `ui.rs`).
6. `SetCoachingActive(false)` already despawns 2D board entities (`CoachingOwned`), stops speech, and stops coaching UI systems.

## What is NOT done (your job)

`EnterGame` is **not handled**. Clicking Next does nothing visible except fire an unused event.

`native/coaching/src/bin/native-coaching.rs` still only starts `CoachingPlugin`. There is no `AppPhase`, no jungle crate dependency, no spawn of the 3D scene.

`CubeSoccerPlugin` on `origin/codex/jungle-soccer-visuals` spawns the whole world on **Startup**. If you `add_plugins(CubeSoccerPlugin)` at launch, the jungle appears under the coaching board. Keep jungle **visuals unmodified**. Wrap it: add Rapier + the same systems the plugin uses, but run spawn on **OnEnter(Game)**, then `jungle::build_jungle`.

Public jungle APIs you can call without editing jungle art:

- `cube_soccer::{CubeSoccerPlugin, GameState, MatchState, Team}`
- `cube_soccer::entities::{spawn_arena, spawn_wall_scoreboard, spawn_field, spawn_goals, spawn_players, spawn_ball}`
- `cube_soccer::systems::{configure_physics, setup_camera, update_camera, apply_player_movement, detect_goals, handle_goal_scored, update_timers, reset_after_goal, reset_after_round, check_reset_timer, ResetTimer, animate_fragments, animate_googly_eyes, spawn_trail_particles, animate_trail_particles, TrailSpawnTimer, update_wall_scoreboard}`
- `cube_soccer::input::keyboard_input_system`
- `cube_soccer::rendering::setup_lighting`
- `cube_soccer::ui::{setup_ui, update_ui}`
- `cube_soccer::jungle::{build_jungle, animate_jungle}`
- Events: `GoalScoredEvent`, `GameOverEvent`, `ResetGameEvent`, `BallTouchedEvent`

On Next:

1. `SetCoachingActive(false)` (despawn 2D camera/pitch/tokens, stop egui coaching UI)
2. Transition `AppPhase::Coaching` → `AppPhase::Game`
3. Run the jungle startup list (physics, arena, field, goals, players, ball, camera, lighting, HUD, `build_jungle`)
4. Pass **no** JSON, tactic names, or `CUBE_SOCCER_*` env vars into the scene

Also expand coaching `DefaultPlugins` / Bevy features with `bevy_pbr` and `tonemapping_luts` (jungle needs them). Coaching `Cargo.toml` currently lacks those and has no `cube-soccer` path dep.

## Branches and worktrees (easy to mix up)

| Location | Branch | What it is |
|---|---|---|
| This folder | `coaching` | Tactic Lab coaching app. **Work here.** |
| `origin/codex/jungle-soccer-visuals` | jungle visuals | Canopy Clash. **This is the game to show.** |
| `origin/main` | Cube Soccer 3D / RL | Heuristic AI cubes. **Out of scope.** |
| `.claude/worktrees/game` | `game-handoff` tracking `origin/main` | Leftover from the wrong handoff. Do not use for the Next scene. Safe to delete the worktree. |
| `.claude/worktrees/player-creation` | `native-player-creation` | Separate player-drawing flow. Not this task. |

`.claude/` is untracked. Do not commit worktrees.

To depend on jungle without editing it:

```sh
git worktree add .claude/worktrees/jungle-soccer origin/codex/jungle-soccer-visuals
```

Then in `native/coaching/Cargo.toml`:

```toml
cube-soccer = { path = "../../.claude/worktrees/jungle-soccer" }
```

(or copy/vendor into `native/jungle` if you want the path inside this branch).

Jungle crate name is **`cube-soccer`**. Bevy 0.13 + `bevy_rapier3d` 0.25. Plugin wiring to copy (do not run at Startup) is `src/game/plugin.rs` on that branch.

## How to run and verify

```sh
cd /Users/chloehouvardas/Documents/CODE/hack-mit-2026
source "$HOME/.cargo/env"          # Cargo may be missing from PATH
npm install
cp .env.example .env               # if needed
npm run dev:native
```

Check: app starts on the **2D coaching board**, not the jungle. Record or add a manual note, Stop, Generate JSON, click **Next**. Same window should become the jungle stadium (broadcast camera, blocky players, grass, timber goals). No coaching chrome.

`cargo build --manifest-path native/coaching/Cargo.toml`
`cargo test --manifest-path native/coaching/Cargo.toml`
`npm test`

macOS terminal often lacks Screen Recording permission; Bevy `ScreenshotManager` can capture from inside the app if you need a frame.

## Do not do

- Do not launch `examples/ai_vs_ai` or inject `rlSelection.downstreamValue`.
- Do not restore `scripts/play-tactic.mjs` / “Kick off match as HighPress”.
- Do not edit jungle art, cameras, or gameplay. Wiring only.
- Do not add `CubeSoccerPlugin` on app Startup.
- Do not commit `.env` or `.claude/worktrees/*`.

## Suggested implementation order

1. Worktree or vendor `origin/codex/jungle-soccer-visuals`; path-dep `cube-soccer`.
2. Add `AppPhase { Coaching, Game }` (binary or `lib.rs`). Start on Coaching.
3. Thin wrapper plugin: same systems as `CubeSoccerPlugin`, spawn on `OnEnter(Game)`, updates `run_if` in Game.
4. System that reads `EnterGame` → `SetCoachingActive(false)` → `AppPhase::Game`.
5. Bevy features: `bevy_pbr`, `tonemapping_luts`.
6. Smoke the full flow in one window.
