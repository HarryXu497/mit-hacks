//! Cube Soccer 3D - Standalone game executable
//!
//! Run with: cargo run --bin cube-soccer

use bevy::prelude::*;
use cube_soccer::CubeSoccerPlugin;

fn main() {
    let mut app = App::new();
    app
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Canopy Clash | Jungle Soccer".to_string(),
                resolution: (1280.0_f32, 720.0_f32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CubeSoccerPlugin);
    // Set CANOPY_DIAG to print frame time, fps and entity count every two
    // seconds. Frame cost here is dominated by how many separate objects reach
    // the GPU, so the entity count is the number to watch alongside the fps.
    if std::env::var("CANOPY_DIAG").is_ok() {
        app.add_plugins((
            bevy::diagnostic::FrameTimeDiagnosticsPlugin,
            bevy::diagnostic::EntityCountDiagnosticsPlugin,
            bevy::diagnostic::LogDiagnosticsPlugin {
                wait_duration: std::time::Duration::from_secs(2),
                ..Default::default()
            },
        ));
    }
    app.add_systems(Update, capture_preview).run();
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
    let capture_frame = std::env::var("CANOPY_CAPTURE_FRAME")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(90)
        .clamp(1, 3600);
    if *frames == capture_frame {
        screenshots
            .save_screenshot_to_disk(window.single(), path)
            .unwrap();
    }
    if *frames == capture_frame + 40 {
        exit.send(bevy::app::AppExit);
    }
}
