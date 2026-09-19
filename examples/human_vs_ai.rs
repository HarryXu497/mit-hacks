//! Human vs AI gameplay example
//!
//! Controls:
//! - Orange player (Human): WASD + Space
//! - Blue player (AI): Controlled by simple AI
//!
//! Run with: cargo run --release --example human_vs_ai

use bevy::prelude::*;
use cube_soccer::game::Team;
use cube_soccer::entities::{CubePlayer, PlayerInput, Ball};
use cube_soccer::CubeSoccerPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Cube Soccer 3D - Human vs AI".to_string(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CubeSoccerPlugin)
        .add_systems(Update, simple_ai_controller)
        .run();
}

/// Simple AI controller for the Blue player
/// Moves towards the ball and tries to push it towards the Orange goal
fn simple_ai_controller(
    ball_query: Query<&Transform, With<Ball>>,
    mut player_query: Query<(&mut PlayerInput, &Transform, &CubePlayer)>,
) {
    let Ok(ball_transform) = ball_query.get_single() else {
        return;
    };

    let ball_pos = ball_transform.translation;

    for (mut input, player_transform, player) in player_query.iter_mut() {
        // Only control the Blue player (AI)
        if player.team != Team::Blue {
            continue;
        }

        let player_pos = player_transform.translation;

        // Calculate direction to ball
        let to_ball = ball_pos - player_pos;
        let dist_to_ball = to_ball.length();

        // Move towards the ball
        if dist_to_ball > 0.1 {
            input.movement = Vec2::new(
                to_ball.x.clamp(-1.0, 1.0),
                to_ball.z.clamp(-1.0, 1.0),
            ).normalize_or_zero();
        } else {
            // If close to ball, push towards opponent's goal (negative X)
            input.movement = Vec2::new(-1.0, 0.0);
        }

        // Jump occasionally when close to ball
        input.jump = dist_to_ball < 3.0 && ball_pos.y > player_pos.y + 1.0;
    }
}
