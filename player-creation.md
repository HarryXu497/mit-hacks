# Player Creation — handoff notes

Written for an agent integrating this crate into a different branch/checkout
that already has **coaching → game** working, to complete the intended
**creation → coaching → game** pipeline. This file is the single entry point;
the fuller reference (data model, controls, invariants) lives at
[docs/native-player-creation.md](./docs/native-player-creation.md) — read that
before wiring anything, this file is the map to it.

## What this is

A standalone Bevy 0.13 crate, `native/player-creation`
(package `tactic-lab-player-creation`, lib name `tactic_lab_player_creation`),
independent of `native/coaching`. It has its own binary
(`native-player-creation`) and its own window; nothing about it assumes the
coaching or game crates exist.

Flow: **appearance drawing → superpower drawing → player review → continue to
coaching**, repeated implicitly for all ten fixed players (red `1–5`, yellow
`6–10` — same fixed 5-v-5 convention as the rest of Tactic Lab). No API key,
no Node service, no network access.

## Run it standalone

```sh
cargo run --manifest-path native/player-creation/Cargo.toml --bin native-player-creation
```

Needs Rust 1.85+ (Bevy 0.13's dependency closure requires it). See
[docs/native-player-creation.md](./docs/native-player-creation.md) for
controls and platform prerequisites.

## What it produces

Per session, under `output/player-creations/<session-id>/`:

- `player-01-appearance.png`, `player-01-superpower.png`, … one pair per
  player, 1024×1024 PNG with transparency, normalized `0..1` drawing
  coordinates (same convention the coaching board uses)
- `manifest.json` — ten player entries (always ten, keyed by player ID),
  each with team, both PNG paths (relative to the manifest), per-slot saved
  timestamps, and stroke counts. Paths are relative so the session directory
  is portable as a unit.

Full schema and example in
[docs/native-player-creation.md § Storage](./docs/native-player-creation.md#storage).
**The manifest is the only thing a consumer needs** — it names every PNG,
its player, and its team, and PNG is directly accepted as image input by
model APIs.

## The integration seam (how creation hands off to coaching)

This crate never imports or depends on `native/coaching`. Integration is
one-directional and event-based:

- `PlayerCreationPlugin` (in `lib.rs`) is a normal Bevy plugin — add it to any
  app that already installs `EguiPlugin`.
- On leaving the review screen it fires a `ContinueToCoaching { session_id,
  manifest_path }` event (`state.rs`). The standalone binary just logs it;
  a combined app should listen for it and take over the window.
- `PlayerCreationPlugin::default().yielding_to_coaching()` stops this crate's
  UI system from running once `CreationFlow` reaches `ContinueToCoaching`, so
  it releases the window instead of drawing a summary screen forever. Without
  `.yielding_to_coaching()`, this crate keeps drawing (`HandoffBehavior::ShowSummary`,
  the default) — correct for standalone use, wrong for a combined app.
- `full_flow_requested()` reads `TACTIC_LAB_FLOW=full` from the environment.
  It's a suggested convention for a combined binary to decide whether to
  start in creation or skip straight to coaching — use it or replace it, the
  crate doesn't enforce it.

The exact wiring snippet this repo wrote for combining with `native/coaching`
(add the path dependency, suspend the coaching board at startup, resume it on
`ContinueToCoaching`) is in
[docs/native-player-creation.md § Continuing to coaching](./docs/native-player-creation.md#continuing-to-coaching).
**That snippet was written but never compiled** — `native/coaching` was being
changed by other work in this repo and deliberately left untouched. Treat it
as a correct-shape starting point, not a tested patch. The two details that
make it work rather than just compile (both explained in that section) are
worth reading closely:
1. Suspend the coaching board via a `SetCoachingActive(false)` event, not by
   inserting a disabled `CoachingLifecycle` resource — the board's `Startup`
   system already spawns as active, so a resource-only approach makes the
   later "turn it on" event a no-op.
2. `.yielding_to_coaching()` is required, or both screens' egui UIs paint into
   the same window after handoff.

If the target branch's coaching→game integration doesn't look like
`native/coaching` here, the shape of the two details above still applies:
whatever owns the board/game needs to (a) start suspended or be told to
suspend, and (b) resume on `ContinueToCoaching`, and this crate's own UI needs
to stop running once that event fires.

## Extending storage (e.g., to feed a model, or a different consumer)

Swap the storage backend without touching the drawing UI, data model, or
flow, via the `CreationStore` trait (`persistence.rs`):

```rust
pub trait CreationStore: Send + Sync {
    fn save_drawing(&self, session_id: &str, player: PlayerId, slot: DrawingSlot, png: &[u8])
        -> Result<PathBuf>;
    fn write_manifest(&self, manifest: &CreationManifest) -> Result<PathBuf>;
}
```

`PlayerCreationPlugin::with_store(Arc::new(MyStore))` installs a different
backend. Nothing currently sends drawings to a model — this trait is the
seam, not a working integration. If a remote store is added, credentials must
live in the local service the coaching client already uses for
transcription/interpretation, never in this native client.

## Architecture at a glance

| Module | Owns | Depends on |
| --- | --- | --- |
| `state.rs` | `CreationFlow` states, `PlayerId`/`Team`, `PlayerCreationSession`/`PlayerEntry`, `ContinueToCoaching` event | — |
| `drawing.rs` | Strokes, rasterization (`Canvas`) | — (no Bevy, no egui) |
| `input.rs` | Pointer/keyboard → intent, tool settings, palette | egui types only |
| `persistence.rs` | `CreationStore` trait, `LocalFileStore`, `CreationManifest` | `state`, `drawing` |
| `ui.rs` | Layout/presentation only | all of the above |

Invariants a future change should preserve (tests in the crate pin these
down — see `cargo test` below):
- Strokes are the source of truth; the raster/PNG is a derived, disposable
  cache.
- Canvases are owned per-player, indexed by `PlayerId` — there's no
  free-floating "current drawing" that navigation could misattach.
- Coordinates are normalized `0..1`, independent of window size.
- A failed write never advances the flow or updates the manifest; a PNG is
  always written before the manifest references it.

## Verification

```sh
cargo fmt    --manifest-path native/player-creation/Cargo.toml --check
cargo clippy --manifest-path native/player-creation/Cargo.toml --all-targets -- -D warnings
cargo test   --manifest-path native/player-creation/Cargo.toml
cargo run    --manifest-path native/player-creation/Cargo.toml --bin native-player-creation
```

## Known gaps / things not to assume

- **Not wired into any game.** "The jungle soccer game does not consume
  these drawings yet, and nothing here depends on it" — quoted from the
  detailed doc, still true.
- **No model/interpretation call exists yet.** The manifest + PNGs are the
  full output; nothing here calls out to OpenAI or any other model.
- **The coaching-combination snippet is unverified** (see above) — compile
  and exercise it in the target branch before trusting it.
- **Windows path (backup-and-restore rename) was not tested** in this
  environment — only verified on macOS/Apple silicon/Metal. Re-run the
  verification commands on Windows before shipping there.
- This crate pins exact dependency versions (`=x.y.z` throughout its
  `Cargo.toml`) to match what the rest of Tactic Lab's native crates use.
  Keep them in lockstep if the target branch's `native/coaching`-equivalent
  uses different pinned versions of `bevy`/`bevy_egui`.
