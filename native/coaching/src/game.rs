use bevy::prelude::*;
use bevy::transform::TransformSystem;
use bevy_rapier3d::prelude::*;

use crate::game_handoff::GameHandoff;
use cube_soccer::entities::CubePlayer;
use cube_soccer::entities::{
    spawn_arena, spawn_ball, spawn_field, spawn_goals, spawn_players, spawn_wall_scoreboard,
};
use cube_soccer::game::{
    BallTouchedEvent, GameOverEvent, GameState, GoalScoredEvent, MatchState, ResetGameEvent,
};
use cube_soccer::jungle::{animate_jungle, build_jungle};
use cube_soccer::rendering::setup_lighting;
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
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            .insert_state(MatchState::Playing)
            .init_resource::<GameState>()
            .init_resource::<ResetTimer>()
            .init_resource::<TrailSpawnTimer>()
            .init_resource::<TeamTactics>()
            .init_resource::<Possession>()
            .add_event::<ImpulseEvent>()
            .add_event::<GoalScoredEvent>()
            .add_event::<GameOverEvent>()
            .add_event::<ResetGameEvent>()
            .add_event::<BallTouchedEvent>()
            .add_systems(
                OnEnter(AppPhase::Game),
                (
                    configure_physics,
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
                )
                    .chain(),
            )
            .add_systems(OnEnter(AppPhase::Game), build_jungle.after(setup_ui))
            .add_systems(Update, animate_jungle.run_if(in_state(AppPhase::Game)))
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
                    update_ui,
                    update_wall_scoreboard,
                )
                    .chain()
                    .run_if(in_state(AppPhase::Game))
                    .run_if(in_state(MatchState::Playing)),
            )
            .add_systems(
                Update,
                check_reset_timer
                    .run_if(in_state(AppPhase::Game))
                    .run_if(in_state(MatchState::GoalScored)),
            )
            .add_systems(
                Update,
                (
                    animate_fragments,
                    animate_googly_eyes,
                    spawn_trail_particles,
                    animate_trail_particles,
                )
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

fn spawn_tactic_hud(mut commands: Commands, handoff: Res<GameHandoff>) {
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
