//! Human vs Human gameplay example
//!
//! Controls:
//! - Orange player (index 0): WASD + Space
//! - Blue player (index 0): Arrow keys + Enter
//! - Teammates (index >= 1) are AI-controlled.
//!
//! Run with: cargo run --release --example human_vs_human

use bevy::prelude::*;
use cube_soccer::entities::CubePlayer;
use cube_soccer::input::keyboard::keyboard_input_system;
use cube_soccer::systems::heuristic_ai::{apply_heuristic_ai, AiControlled};
use cube_soccer::CubeSoccerPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Cube Soccer 3D - Human vs Human".to_string(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CubeSoccerPlugin)
        .add_systems(PostStartup, tag_ai_teammates)
        .add_systems(Update, apply_heuristic_ai.after(keyboard_input_system))
        .run();
}

/// Tag every non-human cube (index >= 1 on both teams) as AI-controlled.
fn tag_ai_teammates(mut commands: Commands, query: Query<(Entity, &CubePlayer)>) {
    for (entity, player) in query.iter() {
        if player.index != 0 {
            commands.entity(entity).insert(AiControlled);
        }
    }
}
