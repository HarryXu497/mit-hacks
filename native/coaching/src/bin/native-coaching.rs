use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use bevy_egui::EguiPlugin;
use tactic_lab_native::{
    forge::ForgePlugin, game::GamePlugin, network::LobbyPlugin, phase::AppPhase,
    world::WorldPlugin, CoachingPlugin,
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
        // Turns the painted superpower into one of the game's four, and arms the coached side.
        .add_plugins(ForgePlugin)
        .add_systems(Update, capture)
        .run();
}

/// Set `TACTIC_LAB_CAPTURE` to a PNG path for a reproducible screenshot of the running app.
///
/// The same affordance `cube-soccer`'s own preview binary has, and for the same reason: a screen
/// is the only honest way to check a screen, and "it looked right on my machine" is not a record.
/// `TACTIC_LAB_CAPTURE_FRAME` picks the frame, which matters because the world takes a moment to
/// batch and any glTF character arrives a little after that.
fn capture(
    mut frames: Local<u32>,
    mut screenshots: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut exit: EventWriter<bevy::app::AppExit>,
) {
    let Ok(path) = std::env::var("TACTIC_LAB_CAPTURE") else {
        return;
    };
    let Ok(window) = window.get_single() else {
        return;
    };
    *frames += 1;
    let at = std::env::var("TACTIC_LAB_CAPTURE_FRAME")
        .ok()
        .and_then(|frame| frame.parse::<u32>().ok())
        .unwrap_or(150)
        .clamp(1, 3600);
    if *frames == at {
        let _ = screenshots.save_screenshot_to_disk(window, path);
    }
    if *frames == at + 90 {
        exit.send(bevy::app::AppExit);
    }
}
