use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResizeConstraints, WindowResolution};
use bevy_egui::EguiPlugin;
use tactic_lab_player_creation::state::ContinueToCoaching;
use tactic_lab_player_creation::PlayerCreationPlugin;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.063, 0.094, 0.133)))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Tactic Lab · Player creation".to_owned(),
                        resolution: WindowResolution::new(1440.0, 900.0),
                        resize_constraints: WindowResizeConstraints {
                            min_width: 960.0,
                            min_height: 680.0,
                            ..default()
                        },
                        resizable: true,
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(EguiPlugin)
        .add_plugins(PlayerCreationPlugin::default())
        .add_systems(Update, report_handoff)
        .run();
}

/// Standalone build has no coaching screen to hand off to, so the boundary is
/// logged. The combined app replaces this with `SetCoachingActive(true)`.
fn report_handoff(mut events: EventReader<ContinueToCoaching>) {
    for event in events.read() {
        match &event.manifest_path {
            Some(path) => info!(
                "Player creation complete for session {}; manifest at {}",
                event.session_id,
                path.display()
            ),
            None => info!("Player creation complete for session {}", event.session_id),
        }
    }
}
