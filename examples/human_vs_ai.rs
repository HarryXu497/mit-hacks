//! Human vs AI gameplay example
//!
//! Controls:
//! - Orange player index 0 (Human): WASD + Space
//! - All Blue players and Orange teammates: built-in heuristic AI
//!
//! Run with: cargo run --release --example human_vs_ai

use bevy::prelude::*;
use cube_soccer::entities::CubePlayer;
use cube_soccer::game::Team;
use cube_soccer::input::keyboard::keyboard_input_system;
use cube_soccer::systems::heuristic_ai::{apply_heuristic_ai, AiControlled};
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
        .add_systems(PostStartup, tag_ai_players)
        .add_systems(Update, apply_heuristic_ai.after(keyboard_input_system))
        .run();
}

/// Human controls Orange index 0. Everything else (all Blue, Orange teammates)
/// is AI-controlled.
fn tag_ai_players(mut commands: Commands, query: Query<(Entity, &CubePlayer)>) {
    for (entity, player) in query.iter() {
        let is_human = player.team == Team::Orange && player.index == 0;
        if !is_human {
            commands.entity(entity).insert(AiControlled);
        }
    }
}
