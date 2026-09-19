use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use bevy_egui::EguiPlugin;
use tactic_lab_native::{game::GamePlugin, phase::AppPhase, CoachingPlugin};

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.063, 0.094, 0.133)))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Tactic Lab".to_owned(),
                        resolution: WindowResolution::new(1440.0, 900.0),
                        resizable: true,
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .init_state::<AppPhase>()
        .add_plugins(EguiPlugin)
        .add_plugins(CoachingPlugin)
        .add_plugins(GamePlugin)
        .run();
}
