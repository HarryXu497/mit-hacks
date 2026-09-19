//! Human vs Human gameplay example
//!
//! Controls:
//! - Orange player: WASD + Space
//! - Blue player: Arrow keys + Enter
//!
//! Run with: cargo run --release --example human_vs_human

use bevy::prelude::*;
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
        .run();
}
