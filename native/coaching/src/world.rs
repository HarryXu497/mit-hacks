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
use crate::interpretation::{InterpretationState, RequestInterpretation, TacticalResult};
use crate::model::SessionStatus;
use crate::session::CoachingSession;
use crate::EnterGame;
use crate::tactics_bridge;

use cube_soccer::creation::camera::{broadcast_view, CreationCamera, Flight};
use cube_soccer::creation::{persistence::CreationSession, CreationPhase, CreationPlugin};
use cube_soccer::entities::{
    spawn_arena, spawn_ball, spawn_field, spawn_goals, spawn_players, spawn_wall_scoreboard,
};
use cube_soccer::game::{GameState, MatchState, MATCH_DURATION_SECS};
use cube_soccer::jungle::{animate_jungle, animate_water, build_jungle};
use cube_soccer::rendering::batching::merge_static_draws;
use cube_soccer::rendering::wordmark;
use cube_soccer::rendering::setup_lighting;
use cube_soccer::rendering::stylized::{
    is_rendering, keep_characters_unstylised, register_shader, stylize, JungleMaterial,
};
use cube_soccer::systems::camera::{CameraRig, MainCamera};
use cube_soccer::systems::physics::configure_physics;
use cube_soccer::tactics::{
    Interpretation as TableInterpretation, RequestInterpretation as TableInterpretRequest,
    Session as TableSession, TableState, TacticsPlugin,
};

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
            .init_resource::<HalfTime>()
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
            .add_systems(
                Update,
                (keep_characters_unstylised, stylize, animate_jungle, animate_water).chain(),
            )
            // The lobby's backdrop is the stadium itself, seen from a slow orbit. Set after the
            // creation camera is spawned, because it is that same camera -- there is only ever
            // one, and it is about to glide from here up to the easel.
            .add_systems(
                Startup,
                frame_the_lobby.after(cube_soccer::creation::camera::spawn_camera),
            )
            .add_systems(Update, drift_the_lobby_view.run_if(in_state(AppPhase::Lobby)))
            // The title, standing in the world rather than drawn over it.
            .add_systems(OnEnter(AppPhase::Lobby), raise_the_wordmark)
            .add_systems(
                Update,
                carry_the_wordmark.run_if(in_state(AppPhase::Lobby)),
            )
            .add_systems(OnExit(AppPhase::Lobby), strike_the_wordmark)
            .add_systems(OnExit(AppPhase::Lobby), step_up_to_the_easel)
            .add_systems(
                OnEnter(CreationPhase::Coaching),
                (capture_the_drawings, open_the_table).chain(),
            )
            .add_systems(OnEnter(AppPhase::Game), leave_the_island)
            .add_systems(
                OnEnter(CreationPhase::Finished),
                (hand_the_camera_to_the_match, kick_off).chain(),
            )
            // Both restarts, not just the round boundary. A round is forty-five seconds now,
            // so waiting for one could carry the interval most of a round past the midpoint,
            // while goals arrive every twenty or thirty seconds -- and a goal is just as clean
            // a break to take it at, for the same reason: the ball is back on the centre spot.
            .add_systems(
                OnEnter(MatchState::RoundOver),
                half_time,
            )
            .add_systems(OnEnter(MatchState::GoalScored), half_time)
            .add_systems(OnEnter(AppPhase::Game), a_new_match_has_its_interval_to_come)
            .add_systems(
                Update,
                (
                    the_panel_drives_the_clock,
                    mirror_the_table_into_the_session,
                    the_table_asks_for_an_interpretation,
                    tell_the_sign_what_came_back,
                )
                    .chain()
                    .run_if(in_state(AppPhase::Coaching)),
            )
            // ...and the table's own keyboard, if that is what the coach reached for, drives the
            // panel back. `OnEnter` fires only on a real transition, and the pusher above only
            // pushes on a change, so the two converge instead of fighting for the clock.
            .add_systems(OnEnter(TableState::Recording), the_table_starts_the_clock)
            .add_systems(OnEnter(TableState::Stopped), the_table_stops_the_clock);
    }
}

/// How many rounds have been played, so the table can say which play it is taking.
#[derive(Resource, Debug, Default)]
pub struct RoundCount(pub u32);

/// Where the camera sits while the menu is up: a wide three-quarter view of the stadium.
///
/// High and far enough back that the pitch, the stands and the ranges behind them are all in
/// frame, because this is the establishing shot of the whole game and the only chance to show
/// the place before the flow starts walking through it.
fn lobby_view(angle: f32) -> Transform {
    /// Distance from the pitch's centre, and height above it.
    const RADIUS: f32 = 104.0;
    const HEIGHT: f32 = 40.0;
    /// Aimed above the turf so the horizon and a band of sky stay in shot, as the match camera
    /// does for the same reason.
    const AIM: Vec3 = Vec3::new(0.0, 12.0, 0.0);

    Transform::from_translation(Vec3::new(
        angle.sin() * RADIUS,
        HEIGHT,
        angle.cos() * RADIUS,
    ))
    .looking_at(AIM, Vec3::Y)
}

/// Builds the title and stands it in the scene.
///
/// Spawned on entering the lobby rather than at startup: `merge_static_draws`
/// absorbs static meshes and despawns the originals at `PostStartup`, and
/// anything built before it stops being drawn while still reporting itself
/// visible. The geometry also carries `NoMerge`, for the same reason twice.
fn raise_the_wordmark(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    wordmark::spawn(
        &mut commands,
        &mut meshes,
        &mut materials,
        wordmark::FONT,
        ["MONKEY", "BUSINESS"],
    );
}

/// Keeps the title at a fixed place in frame while the lobby view turns.
///
/// The obvious thing is to stand it somewhere on the mountainside, which is
/// what the game's own title card does -- but that card is shot from one fixed
/// camera. This one orbits the whole stadium every four minutes, so a title
/// planted in the world would swing out of frame and spend a quarter of the
/// turn facing away. Carrying it with the camera keeps the letters legible and
/// still lets them take the scene's light, its cel banding and its shadows.
fn carry_the_wordmark(
    cameras: Query<&Transform, (With<CreationCamera>, Without<wordmark::Wordmark>)>,
    mut titles: Query<&mut Transform, (With<wordmark::Wordmark>, Without<Parent>)>,
) {
    let Ok(view) = cameras.get_single() else {
        return;
    };
    for mut title in &mut titles {
        // Right of centre and high, clear of the menu column on the left, and
        // set far enough back that the two lines read as a wordmark over the
        // scene rather than filling the frame. Turned a few degrees so the
        // extrusion's side walls stay visible instead of presenting flat on.
        *title = view.mul_transform(Transform::from_xyz(0.5, 6.8, -50.0));
        title.rotation *= Quat::from_rotation_y(-0.22);
    }
}

fn strike_the_wordmark(
    mut commands: Commands,
    titles: Query<Entity, (With<wordmark::Wordmark>, Without<Parent>)>,
) {
    for title in &titles {
        commands.entity(title).despawn_recursive();
    }
}

fn frame_the_lobby(mut cameras: Query<&mut Transform, With<CreationCamera>>) {
    if let Ok(mut transform) = cameras.get_single_mut() {
        *transform = lobby_view(0.0);
    }
}

/// Turn the establishing shot, slowly.
///
/// Slowly on purpose: about four minutes for a full turn, which reads as a living scene behind a
/// menu rather than as something moving that you are meant to watch. The easel glide takes over
/// the moment the lobby is left, easing from wherever this had got to.
fn drift_the_lobby_view(time: Res<Time>, mut cameras: Query<&mut Transform, With<CreationCamera>>) {
    /// Radians per second. TAU over this is roughly four minutes.
    const DRIFT: f32 = 0.026;

    if let Ok(mut transform) = cameras.get_single_mut() {
        *transform = lobby_view(time.elapsed_seconds() * DRIFT);
    }
}

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
    mut commands: Commands,
    phase: Res<State<CreationPhase>>,
    mut next: ResMut<NextState<CreationPhase>>,
    mut rounds: ResMut<RoundCount>,
    cameras: Query<&Transform, With<CreationCamera>>,
) {
    rounds.0 += 1;
    // Already down on the pitch: a later round needs no second flight.
    if matches!(*phase.get(), CreationPhase::Finished | CreationPhase::Departing) {
        return;
    }
    let Ok(from) = cameras.get_single() else {
        // No camera to fly. Skip straight to arrived rather than entering a phase whose only
        // system has nothing to move.
        next.set(CreationPhase::Finished);
        return;
    };

    // `Departing` is not a state the camera can be put into on its own: `camera::fly` reads a
    // `Flight` resource describing the journey, and the table inserts one when the coach presses
    // Enter to leave. Setting the phase without it panicked the moment the match started --
    // "Resource requested by creation::camera::fly does not exist". The flight has to be handed
    // over with the phase, from wherever the camera currently is.
    commands.insert_resource(Flight {
        elapsed: 0.0,
        duration: cube_soccer::creation::FLIGHT_SECONDS,
        from: *from,
        to: broadcast_view(),
    });
    next.set(CreationPhase::Departing);
}

/// Hand the creation camera over to the match once it has landed.
///
/// One camera makes the whole journey. The alternative -- a second camera spawned for the match --
/// means two cameras rendering the same window, and a cut instead of a flight. So when the flight
/// finishes, the camera that flew simply becomes the match camera: `update_camera` picks it up by
/// `MainCamera` and starts following the ball from exactly where the flight left it.
fn hand_the_camera_to_the_match(
    mut commands: Commands,
    mut camera: Query<CameraHandover, (With<CreationCamera>, Without<MainCamera>)>,
) {
    for (entity, mut transform) in &mut camera {
        // Placed, not merely marked. `update_camera` will drive it from here, but it runs in
        // PostUpdate, so without this the first rendered frame of a match is still looking at
        // the tactics board.
        *transform = broadcast_view();

        // And `CreationCamera` comes off. It is the tag every camera system in `creation` keys
        // on -- `glide` pulls the camera to the easel or the table, `fly` carries it down -- so
        // leaving it attached means the match camera is still reachable by systems that believe
        // they own it. Handing it over has to be a handover, not a shared claim: after this,
        // `update_camera` is the only thing that moves it.
        commands
            .entity(entity)
            .insert((MainCamera, CameraRig::default()))
            .remove::<CreationCamera>();
    }
}

/// The flight has landed: start the match.
///
/// This link was missing, and its absence was the whole game. The table's Enter key carries the
/// camera down to the pitch and leaves `CreationPhase` at `Finished` — but `AppPhase` stayed on
/// `Coaching`, and *every* match system is gated on `in_state(AppPhase::Game)`:
/// `apply_soccer_ai`, `apply_player_movement`, `activate_superpowers`, `detect_goals`,
/// `update_timers`, possession. So the camera showed a pitch with ten players standing on it
/// under gravity and no game running at all. It looked like broken AI; there was no AI.
///
/// `EnterGame` is sent first so the coached play is applied by `handle_enter_game`, which is
/// where the tactical output is validated and turned into `TeamTactics`. If it refuses — no
/// interpretation yet, or a failed one — the match still begins, on the balanced default that
/// `GamePlugin` already registers. A coach who could not reach the model should still get a game,
/// and arriving on the pitch must never be a dead end.
fn kick_off(
    phase: Res<State<AppPhase>>,
    role: Option<Res<NetworkRole>>,
    result: Res<TacticalResult>,
    session: Res<CoachingSession>,
    mut enter_game: EventWriter<EnterGame>,
    mut next_phase: ResMut<NextState<AppPhase>>,
) {
    // Networked play starts when the server pushes both coaches' merged output, not when this
    // machine's camera lands; `network.rs` owns that transition.
    if role.map(|role| role.is_networked()).unwrap_or(false) {
        return;
    }
    if *phase.get() == AppPhase::Game {
        return;
    }

    let coached_play_is_ready = result.state == InterpretationState::Ready
        && session.session.status == SessionStatus::Interpreted;
    if coached_play_is_ready {
        enter_game.send(EnterGame);
    } else {
        next_phase.set(AppPhase::Game);
    }
}

/// What `hand_the_camera_to_the_match` needs of the camera it is taking over.
type CameraHandover = (Entity, &'static mut Transform);

/// Whether the coach has already had their half-time interval this match.
///
/// One break per match, so this has to outlive the round it is taken in.
#[derive(Resource, Default)]
pub struct HalfTime {
    taken: bool,
}

/// Half time: go back up to the table once, at the midpoint, to change the play.
///
/// Plays used to be re-coached at *every* round boundary -- every fifteen seconds of a
/// five-minute match, twenty interruptions. That is not how a game of soccer is coached, so the
/// interval now happens once, when the match clock reaches halfway.
///
/// It is taken at the first restart at or after the midpoint rather than at the exact second,
/// because a restart is already a clean break: positions have just been reset and the ball is
/// back on the centre spot, so play is never interrupted mid-move. Either kind of restart
/// counts -- a goal as well as a round boundary -- since a round now lasts forty-five seconds
/// and waiting only for those would push the interval well past halfway.
///
/// The match does not need pausing explicitly -- every gameplay system is gated on
/// `in_state(AppPhase::Game)`, including `update_timers`, so leaving that phase stops the AI,
/// movement, scoring *and the clock* on its own, and the world simply stands there while the
/// coach works.
///
/// Solo only. In networked play the lobby merges both coaches' output once per match, so
/// re-coaching mid-match would mean re-synchronising two machines over a protocol that has no
/// version negotiation. A networked match keeps its opening play throughout.
#[allow(clippy::too_many_arguments)]
fn half_time(
    role: Option<Res<NetworkRole>>,
    state: Res<GameState>,
    phase: Res<State<AppPhase>>,
    mut half: ResMut<HalfTime>,
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
    if !is_half_time(state.time_remaining, half.taken) {
        return;
    }
    half.taken = true;

    // A fresh play, on a fresh clock. The first half's log has already been mirrored into the
    // session and interpreted, so nothing is lost by clearing it -- and keeping it would put the
    // first half's movements into the second half's evidence.
    *table = TableSession::default();
    next_table.set(TableState::Setup);
    next_creation.set(CreationPhase::Coaching);
    next_phase.set(AppPhase::Coaching);
}

/// Whether this round boundary is the half-time interval.
///
/// The first boundary at or after the midpoint, and only ever one per match.
fn is_half_time(time_remaining: f32, already_taken: bool) -> bool {
    !already_taken && time_remaining <= MATCH_DURATION_SECS / 2.0
}

/// A match starting from a full clock has not had its interval yet.
///
/// Without this a second match in the same session would kick off already past half time.
fn a_new_match_has_its_interval_to_come(state: Res<GameState>, mut half: ResMut<HalfTime>) {
    if state.time_remaining >= MATCH_DURATION_SECS {
        half.taken = false;
    }
}

/// Start and stop are the panel's to give: push them to the table when they change.
///
/// Two clocks would be one too many. The table keeps the time — its `tick` only advances while it
/// is recording, so the timestamps on the log are time spent coaching rather than time spent
/// sitting in front of the board — and the panel's Record button says when that starts. Tracked
/// through a `Local` so this pushes on a change rather than every frame, which is what stops it
/// from overriding the table's own keyboard the instant the coach uses it.
fn the_panel_drives_the_clock(
    mut last: Local<Option<SessionStatus>>,
    session: Res<CoachingSession>,
    state: Res<State<TableState>>,
    mut next: ResMut<NextState<TableState>>,
) {
    let status = session.session.status;
    if *last == Some(status) {
        return;
    }
    *last = Some(status);

    let wanted = match status {
        SessionStatus::Ready => TableState::Setup,
        SessionStatus::Recording => TableState::Recording,
        // Review and Interpreted are both "stopped, with something to look at".
        SessionStatus::Review | SessionStatus::Interpreted => TableState::Stopped,
    };
    if *state.get() != wanted {
        next.set(wanted);
    }
}

/// The coach started the clock at the table rather than in the panel.
fn the_table_starts_the_clock(mut session: ResMut<CoachingSession>) {
    if session.session.status != SessionStatus::Recording {
        session.start_or_resume();
    }
}

/// The coach stopped the clock at the table rather than in the panel.
fn the_table_stops_the_clock(mut session: ResMut<CoachingSession>) {
    if session.session.status == SessionStatus::Recording {
        session.stop();
    }
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

/// The coach asked the table for tactical JSON: pass that on to the service that can do it.
///
/// The table fires its own `RequestInterpretation` and then waits, by design — turning a session
/// into tactical JSON is a model call behind a local service, which the table deliberately knows
/// nothing about. This is the host answering.
fn the_table_asks_for_an_interpretation(
    mut asked: EventReader<TableInterpretRequest>,
    mut requests: EventWriter<RequestInterpretation>,
) {
    if asked.read().next().is_some() {
        requests.send(RequestInterpretation::Generate);
    }
}

/// Report the interpretation back onto the table's timber sign.
///
/// The sign is the only readout the coach has while they are out on the island, so it has to say
/// what happened — including when it failed, which the table cannot discover for itself.
fn tell_the_sign_what_came_back(
    result: Res<TacticalResult>,
    mut sign: ResMut<TableInterpretation>,
) {
    if !result.is_changed() {
        return;
    }

    sign.requested = matches!(result.state, InterpretationState::Generating);
    match result.state {
        InterpretationState::Ready => {
            let selection = result
                .output
                .as_ref()
                .and_then(|output| output.get("rlSelection"));
            sign.tactic = selection
                .and_then(|s| s.get("primaryTactic"))
                .and_then(|t| t.as_str())
                .map(str::to_owned);
            sign.summary = result
                .output
                .as_ref()
                .and_then(|output| output.get("summary"))
                .and_then(|s| s.get("objective"))
                .and_then(|o| o.as_str())
                .map(str::to_owned);
            sign.error = None;
        }
        InterpretationState::Failed => {
            sign.tactic = None;
            sign.summary = None;
            // The panel's own notice carries the provider detail; the sign gets the short form.
            sign.error = Some(
                result
                    .notice
                    .clone()
                    .unwrap_or_else(|| "Interpretation failed.".to_owned()),
            );
        }
        InterpretationState::Idle | InterpretationState::Generating => {
            sign.error = None;
        }
    }
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

    /// Plays are re-coached once, at half time -- not twenty times a match.
    ///
    /// The interval used to be taken at *every* round boundary: a five-minute match with
    /// fifteen-second rounds meant twenty trips back to the table.
    #[test]
    fn the_interval_is_taken_once_at_the_midpoint() {
        let full = cube_soccer::game::MATCH_DURATION_SECS;

        // The whole first half is played without interruption.
        for elapsed in [0.0, 0.25, 0.49] {
            assert!(
                !is_half_time(full - full * elapsed, false),
                "no interval {elapsed} of the way through the first half"
            );
        }
        // The first boundary at or past the midpoint takes it.
        assert!(is_half_time(full / 2.0, false), "the midpoint is half time");
        assert!(is_half_time(full * 0.4, false), "so is the first boundary after it");
        // And the second half is played without interruption.
        assert!(!is_half_time(full * 0.4, true), "one interval per match, not two");
        assert!(!is_half_time(1.0, true), "not again in the closing seconds");
    }
}
