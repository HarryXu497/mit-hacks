use bevy::prelude::*;
use bevy::transform::TransformSystem;
use bevy_rapier3d::prelude::*;

use crate::game_handoff::MatchHandoff;
use crate::game_stream::{
    apply_network_snapshot, publish_game_snapshot, start_game_stream, SnapshotTimer,
};
use crate::network::{is_not_spectator, is_spectator, NetworkRole};
use cube_soccer::entities::CubePlayer;
use cube_soccer::entities::{
    animate_player_visual, reveal_loaded_characters, wear_characters, WornCharacters,
};
use cube_soccer::game::{
    BallTouchedEvent, GameOverEvent, GameState, GoalScoredEvent, MatchState, ResetGameEvent,
};
use cube_soccer::input::{apply_ai_actions, AIActions};
use cube_soccer::rl::{apply_policy_actions, PolicyController, PolicyNet, PolicyTeams};
use cube_soccer::systems::{
    activate_superpowers, apply_status_forces, clamp_velocities,
    clear_possession, tick_cooldowns, tick_status_effects, tick_superpower_cooldowns,
    update_possession, AiControlled, ImpulseEvent, Possession, TeamTactics,
};
use cube_soccer::systems::{
    animate_fragments, animate_googly_eyes, animate_trail_particles, apply_player_movement,
    check_reset_timer, detect_goals, handle_goal_scored, reset_after_goal, reset_after_round,
    spawn_trail_particles, update_camera, update_timers, CubeFragment,
    update_wall_scoreboard, ResetTimer, TrailSpawnTimer,
};
use cube_soccer::ui::{setup_ui, update_ui};

use crate::phase::AppPhase;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // The jungle's material and sample count are set up by `world::WorldPlugin`, which owns
        // the scene they belong to.
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            .insert_state(MatchState::Playing)
            .init_resource::<GameState>()
            .init_resource::<ResetTimer>()
            .init_resource::<TrailSpawnTimer>()
            .init_resource::<TeamTactics>()
            .init_resource::<AIActions>()
            .insert_resource(PolicyTeams::default())
            .insert_resource(load_policy_controller())
            .init_resource::<Possession>()
            .init_resource::<WornCharacters>()
            .init_resource::<MatchFurnished>()
            .init_resource::<SnapshotTimer>()
            .add_event::<ImpulseEvent>()
            .add_event::<GoalScoredEvent>()
            .add_event::<GameOverEvent>()
            .add_event::<ResetGameEvent>()
            .add_event::<BallTouchedEvent>()
            // The world -- pitch, players, jungle, and the two screens standing on the island --
            // is built once at startup by `world::WorldPlugin`, not here. This phase is entered
            // again at every round boundary, so anything spawned here would be spawned again.
            //
            // What is left is what genuinely belongs to starting a match: whose players the AI
            // drives, the play showing on the HUD, and the stream to a spectating joiner.
            .add_systems(
                OnEnter(AppPhase::Game),
                (configure_network_physics, tag_players_ai, show_the_play).chain(),
            )
            // Furnishings that must exist exactly once, however many rounds are played. The
            // stream in particular binds a socket: starting it twice fails to bind and leaves a
            // joiner with no game.
            .add_systems(
                OnEnter(AppPhase::Game),
                (setup_ui, start_game_stream, mark_furnished)
                    .chain()
                    .run_if(not_yet_furnished),
            )
            .add_systems(
                Update,
                (
                    // The coach's tactics reach the players through the RL policy:
                    // `TeamTactics` (set from the coaching handoff) feed each agent's
                    // observation, the policy reads them, and `apply_ai_actions` turns
                    // its output into `PlayerInput`. This replaces the heuristic AI.
                    apply_policy_actions,
                    apply_ai_actions,
                    tick_superpower_cooldowns,
                    activate_superpowers,
                    tick_status_effects,
                    apply_player_movement,
                    apply_status_forces,
                    clamp_velocities,
                    tick_cooldowns,
                    update_possession,
                    detect_goals,
                    handle_goal_scored,
                    update_timers,
                )
                    .chain()
                    .run_if(in_state(AppPhase::Game))
                    .run_if(in_state(MatchState::Playing))
                    .run_if(is_not_spectator),
            )
            // Display-only systems: they read `GameState` and write text and
            // materials, so the spectator needs them too — gated off, the
            // joiner's scoreboard never reflected the streamed score.
            .add_systems(
                Update,
                (update_ui, update_wall_scoreboard)
                    .chain()
                    .after(apply_network_snapshot)
                    .run_if(in_state(AppPhase::Game)),
            )
            .add_systems(
                Update,
                publish_game_snapshot.run_if(in_state(AppPhase::Game)),
            )
            .add_systems(
                Update,
                apply_network_snapshot
                    .run_if(in_state(AppPhase::Game))
                    .run_if(is_spectator),
            )
            .add_systems(
                Update,
                check_reset_timer
                    .run_if(in_state(AppPhase::Game))
                    .run_if(in_state(MatchState::GoalScored))
                    .run_if(is_not_spectator),
            )
            .add_systems(
                Update,
                (
                    animate_fragments,
                    animate_googly_eyes,
                    // Display-only, and deliberately not gated on `is_not_spectator`: the joiner
                    // has to animate too, which is why `apply_network_snapshot` recovers
                    // `Velocity` from the streamed positions rather than leaving it at zero.
                    animate_player_visual,
                    spawn_trail_particles,
                    animate_trail_particles,
                )
                    .run_if(in_state(AppPhase::Game)),
            )
            // Which model each side wears, and showing it once it has genuinely loaded. This is
            // how a character forged from a player's drawing reaches the pitch: the forge writes
            // the GLB and points `WornCharacters` at it, and the whole team changes.
            .add_systems(
                Update,
                (wear_characters, reveal_loaded_characters)
                    .chain()
                    .run_if(in_state(AppPhase::Game)),
            )
            .add_systems(
                PostUpdate,
                update_camera
                    .run_if(in_state(AppPhase::Game))
                    .before(TransformSystem::TransformPropagate),
            )
            .add_systems(
                OnEnter(MatchState::GoalScored),
                (reset_after_goal, clear_possession)
                    .chain()
                    .run_if(in_state(AppPhase::Game)),
            )
            .add_systems(
                OnEnter(MatchState::RoundOver),
                (reset_after_round, clear_possession)
                    .chain()
                    .run_if(in_state(AppPhase::Game)),
            )
            // The countdown-driven round reset bursts the players into cube
            // fragments, but the same transition leaves `AppPhase::Game` for the
            // table — which stops `animate_fragments`, the system that clears them.
            // Sweep them up on arrival at the table so they don't hang on the pitch.
            .add_systems(OnEnter(AppPhase::Coaching), clear_reset_debris);
    }
}

/// Despawn the decomposition fragments left by a round-ending reset. See the
/// `OnEnter(AppPhase::Coaching)` registration above for why they linger.
fn clear_reset_debris(mut commands: Commands, fragments: Query<Entity, With<CubeFragment>>) {
    for entity in &fragments {
        commands.entity(entity).despawn_recursive();
    }
}

/// Whether the one-off furnishings of a match have been put in place.
///
/// `AppPhase::Game` is entered once per round, not once per match, so the systems that spawn the
/// score HUD and open the game stream are gated on this.
#[derive(Resource, Default)]
struct MatchFurnished(bool);

fn not_yet_furnished(furnished: Res<MatchFurnished>) -> bool {
    !furnished.0
}

fn mark_furnished(mut furnished: ResMut<MatchFurnished>) {
    furnished.0 = true;
}

/// Marks the panel naming the play currently being run, so it can be replaced when the play does.
#[derive(Component)]
struct PlayReadout;

fn tag_players_ai(mut commands: Commands, players: Query<Entity, With<CubePlayer>>) {
    for entity in &players {
        commands.entity(entity).insert(AiControlled);
    }
}

/// Load the trained PPO policy that drives both coached teams. The weights live
/// with the `cube-soccer` crate (`assets/policy.json`); override the path with
/// `COACH_POLICY_JSON`. The diffuse checkpoints make the deterministic mean look
/// passive, so we sample by default — set `COACH_POLICY_DETERMINISTIC` to force
/// the mean.
fn load_policy_controller() -> PolicyController {
    let path = std::env::var("COACH_POLICY_JSON")
        .unwrap_or_else(|_| PolicyNet::default_asset_path().to_string());
    let net = PolicyNet::load(&path)
        .unwrap_or_else(|e| panic!("coaching: failed to load RL policy from {path}: {e}"));
    let sample = std::env::var("COACH_POLICY_DETERMINISTIC").is_err();
    PolicyController::new(net, sample, 15.0)
}

/// The joiner never steps physics locally — it renders positions straight
/// from the host's stream — so disable Rapier's pipeline there entirely.
/// This is what makes host-authoritative streaming safe against cross-machine
/// simulation drift: the joiner has no local simulation to drift from.
fn configure_network_physics(role: Option<Res<NetworkRole>>, mut config: ResMut<RapierConfiguration>) {
    config.physics_pipeline_active = !role.map(|role| role.is_joiner()).unwrap_or(false);
}

/// Put the play being run on screen, replacing whatever the previous round showed.
///
/// Re-entrant by necessity: a new play is coached at every round boundary, and without the
/// despawn each round's readout would be stacked on top of the last one's.
fn show_the_play(
    mut commands: Commands,
    handoff: Option<Res<MatchHandoff>>,
    previous: Query<Entity, With<PlayReadout>>,
) {
    for entity in &previous {
        commands.entity(entity).despawn_recursive();
    }
    // Optional on purpose: a missing handoff used to panic mid-`OnEnter`, which
    // silently skipped `start_game_stream` and left a joiner with no stream.
    let Some(handoff) = handoff else { return };
    commands.spawn((
        PlayReadout,
        TextBundle {
            text: Text::from_section(
                handoff.summary(),
                TextStyle {
                    font_size: 18.0,
                    color: Color::WHITE,
                    ..default()
                },
            ),
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Px(18.0),
                bottom: Val::Px(18.0),
                ..default()
            },
            background_color: BackgroundColor(Color::rgba(0.02, 0.06, 0.04, 0.85)),
            ..default()
        },
    ));
}
