use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use bevy_egui::EguiPlugin;
use tactic_lab_native::{
    game::GamePlugin, network::LobbyPlugin, phase::AppPhase, world::WorldPlugin, CoachingPlugin,
};

fn main() {
    App::new()
        // Only ever seen for the frame before the world is built; the jungle covers it after that.
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
                // Nearest-neighbour is for the board's crisp 2D sprites. It does not suit the
                // generated character models or the painted canvases, which are sampled at an
                // angle -- those ask for `default()` once the 2D board is gone.
                .set(ImagePlugin::default_nearest()),
        )
        .init_state::<AppPhase>()
        .add_plugins(EguiPlugin)
        // The lobby menu comes first and is the only screen with no world behind it.
        .add_plugins(LobbyPlugin)
        // The world, and the two screens that stand in it: the painter's easel and the tactics
        // table. Built once at startup, because both stand on an island the landscape defines.
        .add_plugins(WorldPlugin)
        // Recording, speech, interpretation and the handoff into a match.
        .add_plugins(CoachingPlugin)
        .add_plugins(GamePlugin)
        .run();
}
