# Native player creation

Player creation is a standalone Bevy 0.13 crate at `native/player-creation`
(`tactic-lab-player-creation`). It runs on its own through the
`native-player-creation` binary, and exposes a `PlayerCreationPlugin` that drops
into the coaching application when the two screens are combined.

The flow is: **appearance drawing → superpower drawing → player review →
continue to coaching**, for each of the ten fixed players (red `1–5`, yellow
`6–10`).

## Prerequisites

- Rust 1.85 or newer with Cargo

Bevy 0.13's current dependency closure contains crates published with the
2024 edition, which Cargo only understands from 1.85 onwards. The crate
declares `rust-version = "1.85"` and CI pins the same version.

On macOS, install the Xcode command-line tools:

```sh
xcode-select --install
```

On Windows, install the Rust MSVC toolchain and the Visual Studio 2022 C++ build
tools.

Player creation needs no API key, no Node service, and no network access, so it
runs directly from Cargo rather than through `npm run dev:native`.

## Run

```sh
cargo run --manifest-path native/player-creation/Cargo.toml --bin native-player-creation
```

### Controls

| Action | Mouse | Keyboard |
| --- | --- | --- |
| Draw | Drag on the canvas | — |
| Brush / eraser | Tool strip | `B` / `E` |
| Brush size | Tool strip slider | `[` / `]` |
| Colour | Tool strip swatches | — |
| Undo | Undo button | `Cmd`/`Ctrl` + `Z` |
| Clear canvas | Clear button | — |
| Save and advance | Save button | `Enter` |
| Change player | Roster rail, Prev/Next | `←` / `→` |

Shortcuts are ignored while a text field has focus.

## Architecture

Five modules, kept apart so each can be changed without disturbing the others:

| Module | Owns | Depends on |
| --- | --- | --- |
| `state.rs` | Flow states, roster, per-player entries | — |
| `drawing.rs` | Strokes and rasterization | — (no Bevy, no egui) |
| `input.rs` | Pointer and keyboard → intent | egui types only |
| `persistence.rs` | Artifact writes behind a trait | `state`, `drawing` |
| `ui.rs` | Layout and presentation | all of the above |

Three invariants the tests pin down:

- **Strokes are the source of truth.** The RGBA raster is a derived cache and
  the PNG is an artifact produced from it. Raw strokes are never conflated with
  the exported image.
- **Canvases are owned by the player entry** and indexed by player ID. There is
  no floating "current drawing", so navigating between players cannot attach a
  drawing to the wrong one.
- **Coordinates are normalized `0..1`,** matching the field-coordinate
  convention used by the coaching board, so resizing the window never distorts
  or rescales a saved drawing.

### Reset scope

"Clear" empties the active canvas only. "Reset this player" clears both of the
selected player's drawings behind a confirmation dialog. There is no
session-wide destructive control, so one player's reset cannot wipe the session.

### Error handling

A failed write keeps the user on the current screen and shows an actionable
message; the flow only advances after a successful save. If the manifest write
fails after the PNG was written, the in-memory saved flag is rolled back so the
UI and the manifest cannot disagree.

## Storage

Artifacts are written under an ignored local output directory:

```text
output/player-creations/<session-id>/player-01-appearance.png
output/player-creations/<session-id>/player-01-superpower.png
output/player-creations/<session-id>/manifest.json
```

Session IDs and player IDs are stable, and drawing file names are derived from
them, so re-saving a slot overwrites in place rather than accumulating files.
Writes go through a temporary file and an atomic rename, with a backup-and-
restore step on Windows, where renaming onto an existing file fails. PNGs are
written before the manifest, so a crash never leaves the manifest pointing at a
file that does not exist.

`manifest.json` carries the session ID, timestamps, canvas dimensions, and one
entry per player ID — always ten entries, keyed by ID, so repeated saves update
in place and never duplicate:

```json
{
  "schemaVersion": 1,
  "sessionId": "session-2f1c…",
  "createdAt": "2026-09-19T20:10:56Z",
  "updatedAt": "2026-09-19T20:12:04Z",
  "canvas": { "width": 1024, "height": 1024 },
  "players": [
    {
      "playerId": 2,
      "team": "red",
      "appearancePath": "player-02-appearance.png",
      "superpowerPath": "player-02-superpower.png",
      "appearanceSavedAt": "2026-09-19T20:11:31Z",
      "superpowerSavedAt": "2026-09-19T20:12:04Z",
      "strokeCounts": { "appearance": 12, "superpower": 5 }
    }
  ]
}
```

Drawing paths are relative to the manifest, so the session directory can be
moved or uploaded as a unit. A player with nothing saved has no path fields.

## Handing drawings to a model later

Nothing is sent to a model yet, and there is no interpretation layer. The seam
for adding one is the `CreationStore` trait in `persistence.rs`:

```rust
pub trait CreationStore: Send + Sync {
    fn save_drawing(&self, session_id: &str, player: PlayerId, slot: DrawingSlot, png: &[u8])
        -> Result<PathBuf>;
    fn write_manifest(&self, manifest: &CreationManifest) -> Result<PathBuf>;
}
```

`PlayerCreationPlugin::with_store(Arc::new(MyStore))` swaps the backend without
touching the drawing UI, the data model, or the flow. A remote implementation
should post to the local service that already holds the OpenAI credentials, as
the coaching client does for transcription and interpretation — **no API keys
belong in the native client.**

Consuming the artifacts as they stand needs only the manifest: it names every
PNG, its player, and its team, and PNG is directly accepted as image input by
the model APIs.

## Continuing to coaching

Leaving review emits a `ContinueToCoaching { session_id, manifest_path }` event.
In the standalone binary this is logged, since there is no coaching screen in
the process. In the combined application, the coaching plugin listens for it.

`native/coaching` is being modified by other work and is deliberately left
untouched by this crate. To combine the two screens, add the dependency:

```toml
# native/coaching/Cargo.toml
tactic-lab-player-creation = { path = "../player-creation" }
```

and wire the handoff in `native/coaching/src/bin/native-coaching.rs`, after
`add_plugins(CoachingPlugin)`:

```rust
use tactic_lab_native::SetCoachingActive;
use tactic_lab_player_creation::state::ContinueToCoaching;
use tactic_lab_player_creation::{full_flow_requested, PlayerCreationPlugin};

// …after .add_plugins(CoachingPlugin):
if full_flow_requested() {
    app.add_plugins(PlayerCreationPlugin::default().yielding_to_coaching())
        .add_systems(Startup, suspend_coaching_until_creation_finishes)
        .add_systems(Update, start_coaching_after_creation);
}

/// `CoachingPlugin` spawns its board at startup and defaults to active, so the
/// combined app switches it off once, after that startup work has run.
fn suspend_coaching_until_creation_finishes(mut activate: EventWriter<SetCoachingActive>) {
    activate.send(SetCoachingActive(false));
}

fn start_coaching_after_creation(
    mut finished: EventReader<ContinueToCoaching>,
    mut activate: EventWriter<SetCoachingActive>,
) {
    for _ in finished.read() {
        activate.send(SetCoachingActive(true));
    }
}
```

Two details make this work rather than merely compile:

- `update_lifecycle` ignores an event that matches the current state, so
  `CoachingLifecycle` is left at its default (`active: true`) and switched off
  with an event. Inserting `CoachingLifecycle { active: false }` instead would
  make the later `SetCoachingActive(true)` a no-op and the board would never
  appear. Sending `false` also despawns the board entities that `spawn_board`
  creates at startup; `SetCoachingActive(true)` respawns them.
- `.yielding_to_coaching()` stops this crate's UI after the handoff, so the two
  egui screens never draw over each other. Without it both would paint into the
  same window.

With that in place, `TACTIC_LAB_FLOW=full` starts in player creation and hands
off to the coaching board; unset, `native-coaching` behaves exactly as it does
today. `full_flow_requested()` and `yielding_to_coaching()` already ship in this
crate, so the snippet is the whole change.

This snippet is written against `native/coaching` as of this commit but has not
been compiled, because that crate is being changed by other work and is left
untouched here.

The jungle soccer game does not consume these drawings yet, and nothing here
depends on it.

## Verification

```sh
cargo fmt    --manifest-path native/player-creation/Cargo.toml --check
cargo clippy --manifest-path native/player-creation/Cargo.toml --all-targets -- -D warnings
cargo test   --manifest-path native/player-creation/Cargo.toml
cargo run    --manifest-path native/player-creation/Cargo.toml --bin native-player-creation
```

Automated tests cover player ID and team mapping, appearance/superpower
separation, navigation between players, undo and reset behaviour, manifest
serialization, save-path generation, overwrite-instead-of-duplicate on repeated
saves, and failed writes surfacing as errors. The integration tests in
`tests/creation_flow.rs` write real files to a temporary directory and read the
PNGs back.

For acceptance, exercise by hand: brush, eraser, colour, and size; undo and
clear; save and continue from appearance to superpower; the review screen; the
roster rail and Prev/Next; resetting one player and confirming another player's
drawings survive; window resizing and display scaling; and a failed write by
making `output/` read-only.

Interactive and rendered-window checks must be run on real desktops. This
implementation was verified on macOS (Apple silicon, Metal). **Windows was not
testable in this environment**; run the four commands above in a Developer
Command Prompt with the MSVC toolchain installed, and confirm in particular that
re-saving a drawing succeeds, since replacing an existing file takes the
backup-and-restore path there.
