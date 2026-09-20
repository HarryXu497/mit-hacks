use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use bevy_egui::EguiPlugin;
use tactic_lab_native::{
    game::GamePlugin,
    network::{CreationArtifacts, LobbyPlugin},
    phase::AppPhase,
    CoachingPlugin,
};
use tactic_lab_player_creation::state::ContinueToCoaching;
use tactic_lab_player_creation::CreationEnabled;
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
        .add_systems(Update, (handle_continue_to_coaching, sync_creation_enabled))
        .add_plugins(CoachingPlugin)
        .add_plugins(GamePlugin)
        .run();
}

fn handle_continue_to_coaching(
    mut finished: EventReader<ContinueToCoaching>,
    mut artifacts: ResMut<CreationArtifacts>,
    mut next_phase: ResMut<NextState<AppPhase>>,
) {
    for event in finished.read() {
        // Captured, not discarded: this is the only link between the drawings
        // on disk and the coaching session that will be uploaded to the host.
        artifacts.session_id = Some(event.session_id.clone());
        // Canonicalized because the creation store's root is CWD-relative.
        artifacts.directory = event
            .manifest_path
            .as_ref()
            .and_then(|path| path.parent())
            .and_then(|dir| dir.canonicalize().ok());
        next_phase.set(AppPhase::Coaching);
    }
}

/// The creation plugin gates its UI only on its own flow state, so without
/// this it renders its drawing toolbar over the lobby menu.
fn sync_creation_enabled(phase: Res<State<AppPhase>>, mut enabled: ResMut<CreationEnabled>) {
    let should_draw = *phase.get() == AppPhase::Creation;
    if enabled.0 != should_draw {
        enabled.0 = should_draw;
    }
}
