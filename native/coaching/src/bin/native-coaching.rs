use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use bevy_egui::EguiPlugin;
use tactic_lab_native::{game::GamePlugin, network::LobbyPlugin, phase::AppPhase, CoachingPlugin};
use tactic_lab_player_creation::state::ContinueToCoaching;
use tactic_lab_player_creation::PlayerCreationPlugin;

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
        .add_plugins(LobbyPlugin)
        .add_plugins(PlayerCreationPlugin::default().yielding_to_coaching())
        .add_systems(Update, handle_continue_to_coaching)
        .add_plugins(CoachingPlugin)
        .add_plugins(GamePlugin)
        .run();
}

fn handle_continue_to_coaching(
    mut finished: EventReader<ContinueToCoaching>,
    mut next_phase: ResMut<NextState<AppPhase>>,
) {
    if finished.read().next().is_some() {
        next_phase.set(AppPhase::Coaching);
    }
}
