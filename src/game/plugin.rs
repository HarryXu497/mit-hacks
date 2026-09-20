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
};
use crate::ui::hud::setup_ui;
use crate::ui::scoreboard::update_ui;

pub struct CubeSoccerPlugin;

impl Plugin for CubeSoccerPlugin {
    fn build(&self, app: &mut App) {
        crate::rendering::stylized::register_shader(app);
        app.add_plugins(MaterialPlugin::<crate::rendering::stylized::JungleMaterial>::default())
            // Four-sample anti-aliasing shades every pixel four times over.
            // Once the static props are batched the frame becomes fill bound,
            // and on integrated graphics that setting costs about a fifth of it
            // (measured 39 fps at 4x against 46 with it off).
            //
            // Off is too far: the goal netting is built from 0.035-wide beams
            // that fall below a pixel at broadcast distance, and with no
            // coverage sampling the mesh breaks into sparkle. Two samples hold
            // the net and the touchlines together for roughly the price of
            // none, so that is where this sits.
            .insert_resource(Msaa::Sample2)
            // States
            .insert_state(MatchState::Playing)
            // Resources
            .init_resource::<GameState>()
            .init_resource::<ResetTimer>()
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
                    crate::rendering::batching::merge_static_draws,
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
            // Speed trails are deliberately not scheduled: the effect is not
            // wanted, and it cost a freshly allocated mesh and material for
            // every particle, thirty-three times a second per moving player.
            // `systems::trail` is still built because other binaries in the
            // workspace schedule it themselves.
            // Animate effects (always running)
            .add_systems(Update, (animate_fragments, animate_googly_eyes));
    }
}
