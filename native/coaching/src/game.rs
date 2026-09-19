use bevy::prelude::*;
use bevy::transform::TransformSystem;
use bevy_rapier3d::prelude::*;

use cube_soccer::entities::{spawn_arena, spawn_ball, spawn_field, spawn_goals, spawn_players, spawn_wall_scoreboard};
use cube_soccer::game::{BallTouchedEvent, GameOverEvent, GameState, GoalScoredEvent, MatchState, ResetGameEvent};
use cube_soccer::input::keyboard_input_system;
use cube_soccer::jungle::{animate_jungle, build_jungle};
use cube_soccer::rendering::setup_lighting;
use cube_soccer::systems::{
    animate_fragments, animate_googly_eyes, animate_trail_particles, apply_player_movement, check_reset_timer,
    configure_physics, detect_goals, handle_goal_scored, reset_after_goal, reset_after_round, setup_camera,
    spawn_trail_particles, update_camera, update_timers, update_wall_scoreboard, ResetTimer, TrailSpawnTimer,
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
                )
                    .chain(),
            )
            .add_systems(OnEnter(AppPhase::Game), build_jungle.after(setup_ui))
            .add_systems(Update, animate_jungle.run_if(in_state(AppPhase::Game)))
            .add_systems(
                Update,
                (
                    keyboard_input_system,
                    apply_player_movement,
                    detect_goals,
                    handle_goal_scored,
                    update_timers,
                    update_ui,
                    update_wall_scoreboard,
                )
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
                (animate_fragments, animate_googly_eyes, spawn_trail_particles, animate_trail_particles)
                    .run_if(in_state(AppPhase::Game)),
            )
            .add_systems(
                PostUpdate,
                update_camera
                    .run_if(in_state(AppPhase::Game))
                    .before(TransformSystem::TransformPropagate),
            )
            .add_systems(OnEnter(MatchState::GoalScored), reset_after_goal)
            .add_systems(OnEnter(MatchState::RoundOver), reset_after_round);
    }
}
