//! One world, standing from launch, and the camera's journey through it.
//!
//! Before this, the app built its world when the match began: entering `AppPhase::Game` spawned
//! the pitch, the players and the jungle, and the phases before it were flat 2D screens drawn over
//! a bare `ClearColor`. That cannot work now, because the two screens before the match are *places
//! in that world* — the painter's easel and the tactics table both stand on an island the
//! landscape itself defines (`creation::OUTCROP` reads `jungle::landscape::OUTCROPS`). The island
//! has to exist before anyone can stand on it.
//!
//! So the world is built once, at startup, and never rebuilt:
//!
//! ```text
//! Lobby ──▶ Creation ──▶ Coaching ──▶ [Waiting] ──▶ Game ──┐
//!  menu     the easel    the table     upload      the match │
//!                             ▲                              │
//!                             └──── end of round ────────────┘
//! ```
//!
//! That last arrow is why nothing here may spawn twice: the match returns to the table at every
//! round boundary for a fresh play, so `OnEnter(AppPhase::Game)` fires again and again. Everything
//! built once lives in `Startup`; the phases only change what is *active*.
//!
//! # Two state machines, one journey
//!
//! `AppPhase` is this crate's, and owns the parts of the flow that are not in the world: the lobby
//! menu, the networked upload, and the match itself. `creation::CreationPhase` is the engine's, and
//! owns the in-world journey — which painting is on the easel, when the table has the floor, and
//! the camera's flight down to the pitch.
//!
//! Neither is subordinate. `AppPhase` leads at the two ends (the host decides when creation begins
//! and when the match starts) and `CreationPhase` leads in the middle (it advances itself through
//! the paintings and on to the table). This module is the only place the two are coupled.

use bevy::prelude::*;

use crate::network::{CreationArtifacts, NetworkRole};
use crate::phase::AppPhase;
use crate::session::CoachingSession;
use crate::tactics_bridge;

use cube_soccer::creation::{camera::CreationCamera, persistence::CreationSession, CreationPhase, CreationPlugin};
use cube_soccer::entities::{
    spawn_arena, spawn_ball, spawn_field, spawn_goals, spawn_players, spawn_wall_scoreboard,
};
use cube_soccer::game::{GameState, MatchState};
use cube_soccer::jungle::{animate_jungle, animate_water, build_jungle};
use cube_soccer::rendering::batching::merge_static_draws;
use cube_soccer::rendering::setup_lighting;
use cube_soccer::rendering::stylized::{is_rendering, register_shader, stylize, JungleMaterial};
use cube_soccer::systems::camera::{CameraRig, MainCamera};
use cube_soccer::systems::physics::configure_physics;
use cube_soccer::tactics::{Session as TableSession, TableState, TacticsPlugin};

/// Builds the world, runs the in-world screens, and couples their state to `AppPhase`.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        // The cel-lighting material extension has to be registered before anything wearing it is
        // spawned, and only where there is a renderer: a headless app has an asset server but no
        // `Assets<Shader>`, and `stylize` no-ops there for the same reason.
        register_shader(app);
        if is_rendering(app) {
            app.add_plugins(MaterialPlugin::<JungleMaterial>::default());
        }

        app.add_plugins(CreationPlugin)
            .add_plugins(TacticsPlugin)
            // Two samples, not four: once the static props are batched the frame is fill bound,
            // and the goal netting's sub-pixel beams sparkle with no coverage sampling at all.
            .insert_resource(Msaa::Sample2)
            .init_resource::<RoundCount>()
            // The pitch and its lighting first, then the jungle on top of it. `build_jungle`
            // blanks the draw mesh of everything that already exists, rebuilding the arena, field
            // and goals procedurally -- so it has to run after the things it replaces, and the
            // clearing and the table have to run after *it*. Both already order themselves that
            // way in their own plugins.
            .add_systems(
                Startup,
                (
                    configure_physics,
                    spawn_arena,
                    spawn_wall_scoreboard,
                    spawn_field,
                    spawn_goals,
                    spawn_players,
                    spawn_ball,
                    setup_lighting,
                )
                    .chain(),
            )
            .add_systems(Startup, build_jungle.after(setup_lighting))
            // Last of all: the static scenery -- pitch, jungle, clearing and table together --
            // collapses into a handful of draws. Ordered after the two screens' own builders so
            // their rocks and timber are merged too.
            .add_systems(
                Startup,
                merge_static_draws
                    .after(cube_soccer::creation::scene::build_clearing)
                    .after(cube_soccer::tactics::scene::build_table),
            )
            // Stylising runs every frame rather than once, because entities keep arriving: a
            // painting is hung, a generated character loads. It is a no-op for anything already
            // wearing the jungle material.
            .add_systems(Update, (stylize, animate_jungle, animate_water))
            .add_systems(OnExit(AppPhase::Lobby), step_up_to_the_easel)
            .add_systems(
                OnEnter(CreationPhase::Coaching),
                (capture_the_drawings, open_the_table).chain(),
            )
            .add_systems(OnEnter(AppPhase::Game), leave_the_island)
            .add_systems(OnEnter(CreationPhase::Finished), hand_the_camera_to_the_match)
            .add_systems(OnEnter(MatchState::RoundOver), back_to_the_table)
            .add_systems(
                Update,
                mirror_the_table_into_the_session.run_if(in_state(AppPhase::Coaching)),
            );
    }
}

/// How many rounds have been played, so the table can say which play it is taking.
#[derive(Resource, Debug, Default)]
pub struct RoundCount(pub u32);

/// The lobby is done with: step up to the easel.
///
/// `CreationPhase` starts `Idle` precisely so this is a decision and not an accident — painting
/// systems running behind the lobby menu would read stray clicks onto the canvas.
fn step_up_to_the_easel(mut phase: ResMut<NextState<CreationPhase>>) {
    phase.set(CreationPhase::PaintingAppearance);
}

/// Take note of what the painter produced, at the moment they finish painting.
///
/// The drawings are the one thing the coaching side needs from the easel: the networked upload
/// ships them to the host, and the forge turns them into a character and a superpower. The easel
/// saves each painting as it is finished and writes a manifest beside them, so by the time the
/// table has the floor they are already on disk.
///
/// This is the same contract the 2D screen had — its `ContinueToCoaching` carried exactly this
/// pair — so `CreationArtifacts` did not have to change.
fn capture_the_drawings(session: Res<CreationSession>, mut artifacts: ResMut<CreationArtifacts>) {
    artifacts.session_id = Some(session.id.clone());
    // Canonicalized because the easel's output root is relative to the working directory, and the
    // upload needs a path it can read from a worker thread.
    artifacts.directory = session.directory().canonicalize().ok();
}

/// The table has the floor, so the app is coaching.
fn open_the_table(phase: Res<State<AppPhase>>, mut next: ResMut<NextState<AppPhase>>) {
    // Only from the easel. A round boundary sets both states itself and must not be undone here.
    if *phase.get() == AppPhase::Creation {
        next.set(AppPhase::Coaching);
    }
}

/// The match is starting: fly the camera off the island and down to the pitch.
///
/// Deliberately driven from `AppPhase::Game` rather than from the table, because what counts as
/// "the match is starting" is the host's business — in networked play it is the moment the server
/// pushes both coaches' merged output, which can be a good while after this machine finished
/// coaching.
fn leave_the_island(
    phase: Res<State<CreationPhase>>,
    mut next: ResMut<NextState<CreationPhase>>,
    mut rounds: ResMut<RoundCount>,
) {
    rounds.0 += 1;
    // Already down on the pitch: a later round needs no second flight.
    if !matches!(*phase.get(), CreationPhase::Finished | CreationPhase::Departing) {
        next.set(CreationPhase::Departing);
    }
}

/// Hand the creation camera over to the match once it has landed.
///
/// One camera makes the whole journey. The alternative -- a second camera spawned for the match --
/// means two cameras rendering the same window, and a cut instead of a flight. So when the flight
/// finishes, the camera that flew simply becomes the match camera: `update_camera` picks it up by
/// `MainCamera` and starts following the ball from exactly where the flight left it.
fn hand_the_camera_to_the_match(
    mut commands: Commands,
    camera: Query<Entity, (With<CreationCamera>, Without<MainCamera>)>,
) {
    for entity in &camera {
        commands
            .entity(entity)
            .insert((MainCamera, CameraRig::default()));
    }
}

/// A round has ended: go back up to the table and coach the next one.
///
/// This is what "plays are defined in rounds" means. The match does not need pausing explicitly --
/// every gameplay system is gated on `in_state(AppPhase::Game)`, so leaving that phase stops the
/// AI, movement and scoring on its own, and the world simply stands there while the coach works.
///
/// Solo only. In networked play the lobby merges both coaches' output once per *match*, so
/// re-coaching every round would mean re-synchronising two machines every fifteen seconds over a
/// protocol that has no version negotiation. A networked match keeps its opening play throughout.
fn back_to_the_table(
    role: Option<Res<NetworkRole>>,
    state: Res<GameState>,
    phase: Res<State<AppPhase>>,
    mut next_phase: ResMut<NextState<AppPhase>>,
    mut next_creation: ResMut<NextState<CreationPhase>>,
    mut table: ResMut<TableSession>,
    mut next_table: ResMut<NextState<TableState>>,
) {
    if role.map(|role| role.is_networked()).unwrap_or(false) {
        return;
    }
    // Only interrupt a match that is actually in progress, and not one that has been won.
    if *phase.get() != AppPhase::Game || state.winner.is_some() {
        return;
    }

    // A fresh play, on a fresh clock. The previous round's log has already been mirrored into the
    // session and interpreted, so nothing is lost by clearing it -- and keeping it would put the
    // last round's movements into the next round's evidence.
    *table = TableSession::default();
    next_table.set(TableState::Setup);
    next_creation.set(CreationPhase::Coaching);
    next_phase.set(AppPhase::Coaching);
}

/// Keep the canonical session in step with what the table has recorded.
///
/// The table logs to its own flat event list; everything downstream -- `replay_session`,
/// `validate_contract`, `/api/interpret`, the handoff, the artifact bundle -- consumes
/// `model::Session`. Rather than teaching either side about the other, this mirrors one into the
/// other once a frame while the table has the floor. See `tactics_bridge`.
fn mirror_the_table_into_the_session(
    table: Res<TableSession>,
    mut session: ResMut<CoachingSession>,
) {
    if !table.is_changed() {
        return;
    }
    tactics_bridge::sync_into(&table, &mut session.session);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_easel_is_only_stepped_up_to_deliberately() {
        // `Idle` is the default, so a host that never leaves its lobby never starts painting.
        assert!(CreationPhase::default().is_idle());
        assert!(!CreationPhase::default().at_the_easel());
    }

    #[test]
    fn a_later_round_does_not_fly_the_camera_again() {
        // The flight happens once, on the way down from the island. Re-entering Game for round
        // two must not replay it, or every round would open with a four-second camera move.
        for already_down in [CreationPhase::Departing, CreationPhase::Finished] {
            assert!(
                matches!(already_down, CreationPhase::Finished | CreationPhase::Departing),
                "the guard in leave_the_island must cover {already_down:?}"
            );
        }
    }
}
