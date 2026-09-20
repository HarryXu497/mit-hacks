//! Visual QA: cargo run --manifest-path native/coaching/Cargo.toml --example shout-preview
//! SHOUT_PREVIEW_TEAM=blue tests joiner badges; TACTIC_LAB_CAPTURE saves a screenshot.
use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use cube_soccer::game::{
    config::{MATCH_DURATION_SECS, ROUND_DURATION_SECS},
    state::GameState,
};
use tactic_lab_native::{
    game::GamePlugin,
    network::{NetworkEndpoint, NetworkRole},
    phase::AppPhase,
    shout::ShoutPlugin,
    world::WorldPlugin,
    CoachingPlugin,
};
fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                // Bevy resolves `assets/` against the manifest or the executable, never the
                // working directory, so this has to be said explicitly or nothing loads.
                .set(cube_soccer::assets::asset_plugin())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Shout visual QA".into(),
                        resolution: if std::env::var("SHOUT_PREVIEW_SMALL").is_ok() {
                            (800_f32, 600_f32).into()
                        } else {
                            (1100_f32, 760_f32).into()
                        },
                        ..default()
                    }),
                    ..default()
                }),
        )
        .insert_state(AppPhase::Waiting)
        .add_plugins(EguiPlugin)
        .init_resource::<NetworkEndpoint>()
        // WorldPlugin schedules `capture_the_drawings`, which writes this; only NetworkPlugin
        // registers it, and this harness deliberately runs without the lobby.
        .init_resource::<tactic_lab_native::network::CreationArtifacts>()
        .insert_resource(
            if std::env::var("SHOUT_PREVIEW_TEAM").as_deref() == Ok("blue") {
                NetworkRole::Joiner
            } else {
                NetworkRole::Solo
            },
        )
        .add_plugins((WorldPlugin, CoachingPlugin, GamePlugin, ShoutPlugin))
        .add_systems(Startup, preview_skin)
        .add_systems(
            Update,
            keep_the_match_running.run_if(in_state(AppPhase::Game)),
        )
        .add_systems(Update, capture)
        .run();
}
/// A solo match hands the coach back to the tactics table every time the 15s round timer expires
/// (`world::back_to_the_table`), and ends outright when the 5 minute match clock runs out. This
/// preview exists to exercise shouts *during* gameplay, so it keeps both clocks wound and stays on
/// the pitch. The scoreboard therefore sits at 15 here, which is the intended tell.
fn keep_the_match_running(mut state: ResMut<GameState>) {
    state.round_timer = ROUND_DURATION_SECS;
    state.time_remaining = MATCH_DURATION_SECS;
}

fn capture(
    mut frames: Local<u32>,
    mut manager: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    windows: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut exit: EventWriter<bevy::app::AppExit>,
    mut phase: ResMut<NextState<AppPhase>>,
    finished: Local<Arc<AtomicBool>>,
) {
    *frames += 1;
    if *frames == 3 {
        phase.set(AppPhase::Game);
    }
    let Ok(path) = std::env::var("TACTIC_LAB_CAPTURE") else {
        return;
    };
    if *frames == 240 {
        let finished = finished.clone();
        manager
            .take_screenshot(windows.single(), move |image| {
                image.try_into_dynamic().unwrap().save(&path).unwrap();
                finished.store(true, Ordering::SeqCst);
            })
            .unwrap();
    }
    if finished.load(Ordering::SeqCst) {
        exit.send(bevy::app::AppExit);
    }
}

fn preview_skin(mut worn: ResMut<cube_soccer::entities::WornCharacters>) {
    if let Ok(path) = std::env::var("SHOUT_PREVIEW_SKIN") {
        worn.orange = Some(path.clone());
        worn.blue = Some(path);
    }
}
