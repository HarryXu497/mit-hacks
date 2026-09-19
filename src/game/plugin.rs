use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use super::events::{BallTouchedEvent, GameOverEvent, GoalScoredEvent, ResetGameEvent};
use super::state::{GameState, MatchState};
use crate::entities::{spawn_arena, spawn_ball, spawn_field, spawn_goals, spawn_wall_scoreboard};
use crate::input::keyboard::keyboard_input_system;
use crate::rendering::lighting::setup_lighting;
use crate::systems::{
    camera::setup_camera,
    display::update_wall_scoreboard,
    effects::animate_fragments,
    eyes::animate_googly_eyes,
    movement::apply_player_movement,
    physics::configure_physics,
    reset::{check_reset_timer, reset_after_goal, reset_after_round, ResetTimer},
    scoring::{detect_goals, handle_goal_scored, update_timers},
    trail::{animate_trail_particles, spawn_trail_particles, TrailSpawnTimer},
};
use crate::ui::hud::setup_ui;
use crate::ui::scoreboard::update_ui;

pub struct CubeSoccerPlugin;

impl Plugin for CubeSoccerPlugin {
    fn build(&self, app: &mut App) {
        crate::rendering::stylized::register_shader(app);
        app.add_plugins(MaterialPlugin::<crate::rendering::stylized::JungleMaterial>::default())
            // States
            .insert_state(MatchState::Playing)
            // Resources
            .init_resource::<GameState>()
            .init_resource::<ResetTimer>()
            .init_resource::<TrailSpawnTimer>()
            // Events
            .add_event::<GoalScoredEvent>()
            .add_event::<GameOverEvent>()
            .add_event::<ResetGameEvent>()
            .add_event::<BallTouchedEvent>()
            // Physics
            .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            // Debug render disabled for cleaner visuals
            // .add_plugins(RapierDebugRenderPlugin::default())
            // Startup systems
            .add_systems(
                Startup,
                (
                    configure_physics,
                    spawn_arena,
                    spawn_wall_scoreboard,
                    spawn_field,
                    spawn_goals,
                    crate::entities::roster::spawn_rosters,
                    spawn_ball,
                    setup_camera,
                    setup_lighting,
                    setup_ui,
                ),
            )
            .add_systems(
                PostStartup,
                (
                    crate::jungle::build_jungle,
                    crate::rendering::stylized::stylize,
                )
                    .chain(),
            )
            .add_systems(Update, crate::jungle::animate_jungle)
            .add_systems(Update, crate::jungle::animate_water)
            .add_systems(
                PostUpdate,
                crate::systems::camera::update_camera
                    .before(bevy::transform::TransformSystem::TransformPropagate),
            )
            // Update systems during playing
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
                    .run_if(in_state(MatchState::Playing)),
            )
            // Reset after goal (with 1 second delay)
            .add_systems(OnEnter(MatchState::GoalScored), reset_after_goal)
            .add_systems(
                Update,
                check_reset_timer.run_if(in_state(MatchState::GoalScored)),
            )
            // Reset after round timeout (immediate)
            .add_systems(OnEnter(MatchState::RoundOver), reset_after_round)
            // Animate effects (always running)
            .add_systems(
                Update,
                (
                    animate_fragments,
                    animate_googly_eyes,
                    spawn_trail_particles,
                    animate_trail_particles,
                ),
            );
    }
}
