//! Cube Soccer 3D - Standalone game executable
//!
//! Run with: cargo run --bin cube-soccer

use bevy::prelude::*;
use cube_soccer::CubeSoccerPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Canopy Clash | Jungle Soccer".to_string(),
                resolution: (1280.0_f32, 720.0_f32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CubeSoccerPlugin)
        .add_systems(Update, capture_preview)
        .run();
}

/// Set CANOPY_CAPTURE to a PNG path for a reproducible camera capture.
fn capture_preview(
    mut frames: Local<u32>,
    mut screenshots: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut exit: EventWriter<bevy::app::AppExit>,
) {
    let Ok(path) = std::env::var("CANOPY_CAPTURE") else {
        return;
    };
    *frames += 1;
    if *frames == 90 {
        screenshots
            .save_screenshot_to_disk(window.single(), path)
            .unwrap();
    }
    if *frames == 130 {
        exit.send(bevy::app::AppExit);
    }
}
