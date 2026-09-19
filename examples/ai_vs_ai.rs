//! AI vs AI gameplay example
//!
//! Watch two simple AI agents play against each other
//!
//! Run with: cargo run --release --example ai_vs_ai

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use cube_soccer::game::{Team, FIELD_WIDTH};
use cube_soccer::entities::{CubePlayer, PlayerInput, Ball};
use cube_soccer::CubeSoccerPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Cube Soccer 3D - AI vs AI".to_string(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CubeSoccerPlugin)
        .add_systems(Update, dual_ai_controller)
        .run();
}

/// AI controller for both players
/// Each AI moves towards the ball and tries to push it towards the opponent's goal
fn dual_ai_controller(
    ball_query: Query<(&Transform, &Velocity), With<Ball>>,
    mut player_query: Query<(&mut PlayerInput, &Transform, &CubePlayer)>,
) {
    let Ok((ball_transform, ball_velocity)) = ball_query.get_single() else {
        return;
    };

    let ball_pos = ball_transform.translation;
    let ball_vel = ball_velocity.linvel;

    for (mut input, player_transform, player) in player_query.iter_mut() {
        let player_pos = player_transform.translation;

        // Determine target goal (opponent's goal)
        let target_goal_x = match player.team {
            Team::Orange => FIELD_WIDTH / 2.0,   // Blue's goal (positive X)
            Team::Blue => -FIELD_WIDTH / 2.0,    // Orange's goal (negative X)
        };

        // Calculate direction to ball
        let to_ball = ball_pos - player_pos;
        let dist_to_ball = to_ball.length();

        // Predict where ball will be
        let prediction_time = 0.5;
        let predicted_ball_pos = ball_pos + ball_vel * prediction_time;
        let to_predicted_ball = predicted_ball_pos - player_pos;

        // Strategy: Position behind ball relative to target goal
        let ball_to_goal = Vec3::new(target_goal_x - ball_pos.x, 0.0, -ball_pos.z);
        let behind_ball_offset = ball_to_goal.normalize_or_zero() * -2.0;
        let target_pos = ball_pos + behind_ball_offset;

        let to_target = target_pos - player_pos;
        let dist_to_target = to_target.length();

        // Movement logic
        if dist_to_ball < 2.5 {
            // Close to ball: push towards goal
            let push_dir = Vec3::new(target_goal_x - player_pos.x, 0.0, -ball_pos.z);
            input.movement = Vec2::new(
                push_dir.x.signum(),
                push_dir.z.clamp(-0.5, 0.5),
            );
        } else if dist_to_target > 1.0 {
            // Move to intercept position
            input.movement = Vec2::new(
                to_predicted_ball.x.clamp(-1.0, 1.0),
                to_predicted_ball.z.clamp(-1.0, 1.0),
            ).normalize_or_zero();
        } else {
            // At position, move towards ball
            input.movement = Vec2::new(
                to_ball.x.clamp(-1.0, 1.0),
                to_ball.z.clamp(-1.0, 1.0),
            ).normalize_or_zero();
        }

        // Jump when ball is above and close
        input.jump = dist_to_ball < 3.0 && ball_pos.y > player_pos.y + 0.5;
    }
}
