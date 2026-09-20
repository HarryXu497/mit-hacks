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
    animate_player_visual, reveal_loaded_characters, spawn_arena, spawn_ball, spawn_field,
    spawn_goals, spawn_players, spawn_wall_scoreboard, wear_characters, WornCharacters,
};
use cube_soccer::game::{
    BallTouchedEvent, GameOverEvent, GameState, GoalScoredEvent, MatchState, ResetGameEvent,
};
use cube_soccer::jungle::{animate_jungle, animate_water, build_jungle};
use cube_soccer::rendering::batching::merge_static_draws;
use cube_soccer::rendering::setup_lighting;
use cube_soccer::rendering::stylized::{is_rendering, register_shader, stylize, JungleMaterial};
use cube_soccer::systems::{
    activate_superpowers, apply_heuristic_ai, apply_status_forces, clamp_velocities,
    clear_possession, tick_cooldowns, tick_status_effects, tick_superpower_cooldowns,
    update_possession, AiControlled, ImpulseEvent, Possession, TeamTactics,
};
use cube_soccer::systems::{
    animate_fragments, animate_googly_eyes, animate_trail_particles, apply_player_movement,
    check_reset_timer, configure_physics, detect_goals, handle_goal_scored, reset_after_goal,
    reset_after_round, setup_camera, spawn_trail_particles, update_camera, update_timers,
    update_wall_scoreboard, ResetTimer, TrailSpawnTimer,
};
use cube_soccer::ui::{setup_ui, update_ui};

use crate::phase::AppPhase;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // The jungle's cel lighting and animated water are a material extension, so the
        // shader has to be registered on the app before anything that uses it is spawned.
        register_shader(app);
        if is_rendering(app) {
            // Only where there is a renderer to use it. The integration tests build a headless
            // app with `AssetPlugin` but no `RenderPlugin`, so there is no `Assets<Shader>` and
            // no material store; `stylize` no-ops there for the same reason.
            app.add_plugins(MaterialPlugin::<JungleMaterial>::default());
        }

        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            // Two samples, not four: once the static props are batched the frame is fill
            // bound, and the goal netting's sub-pixel beams sparkle with no coverage
            // sampling at all. See the note in cube-soccer's own CubeSoccerPlugin.
            .insert_resource(Msaa::Sample2)
            .insert_state(MatchState::Playing)
            .init_resource::<GameState>()
            .init_resource::<ResetTimer>()
            .init_resource::<TrailSpawnTimer>()
            .init_resource::<TeamTactics>()
            .init_resource::<Possession>()
            .init_resource::<WornCharacters>()
            .init_resource::<SnapshotTimer>()
            .add_event::<ImpulseEvent>()
            .add_event::<GoalScoredEvent>()
            .add_event::<GameOverEvent>()
            .add_event::<ResetGameEvent>()
            .add_event::<BallTouchedEvent>()
            .add_systems(
                OnEnter(AppPhase::Game),
                (
                    configure_physics,
                    configure_network_physics,
                    spawn_arena,
                    spawn_wall_scoreboard,
                    spawn_field,
                    spawn_goals,
                    spawn_players,
                    spawn_ball,
                    setup_camera,
                    setup_lighting,
                    setup_ui,
                    tag_players_ai,
                    spawn_tactic_hud,
                    start_game_stream,
                )
                    .chain(),
            )
            // Batching has to see every prop and stylising has to see the batches, so these
            // three are chained. Without the last two the scene renders unlit and unmerged:
            // this branch previously scheduled `build_jungle` alone, back when the jungle was
            // a single file with no landscape, cel material or batching pass.
            .add_systems(
                OnEnter(AppPhase::Game),
                (build_jungle, merge_static_draws, stylize)
                    .chain()
                    .after(setup_ui),
            )
            .add_systems(
                Update,
                (animate_jungle, animate_water).run_if(in_state(AppPhase::Game)),
            )
            .add_systems(
                Update,
                (
                    apply_heuristic_ai,
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
            );
    }
}

fn tag_players_ai(mut commands: Commands, players: Query<Entity, With<CubePlayer>>) {
    for entity in &players {
        commands.entity(entity).insert(AiControlled);
    }
}

/// The joiner never steps physics locally — it renders positions straight
/// from the host's stream — so disable Rapier's pipeline there entirely.
/// This is what makes host-authoritative streaming safe against cross-machine
/// simulation drift: the joiner has no local simulation to drift from.
fn configure_network_physics(role: Option<Res<NetworkRole>>, mut config: ResMut<RapierConfiguration>) {
    config.physics_pipeline_active = !role.map(|role| role.is_joiner()).unwrap_or(false);
}

fn spawn_tactic_hud(mut commands: Commands, handoff: Option<Res<MatchHandoff>>) {
    // Optional on purpose: a missing handoff used to panic mid-`OnEnter`, which
    // silently skipped `start_game_stream` and left a joiner with no stream.
    let Some(handoff) = handoff else { return };
    commands.spawn(TextBundle {
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
    });
}
