//! Standalone preview of the painting clearing.
//!
//! Runs just the creation phase plus the jungle world behind it, so the scene
//! can be looked at and painted on without building the whole match.

use bevy::prelude::*;
use cube_soccer::creation::CreationPlugin;
use cube_soccer::jungle::{animate_jungle, build_jungle};
use cube_soccer::rendering::{setup_lighting, stylized};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Canopy Clash | The Sitting".to_string(),
            resolution: (1440.0_f32, 900.0_f32).into(),
            ..default()
        }),
        ..default()
    }));
    // CANOPY_MSAA=0|2|4 to compare anti-aliasing cost on a given machine.
    if let Ok(samples) = std::env::var("CANOPY_MSAA") {
        app.insert_resource(match samples.as_str() {
            "0" => Msaa::Off,
            "2" => Msaa::Sample2,
            _ => Msaa::Sample4,
        });
    }
    else {
        // Same setting as the match: two samples hold thin edges together for
        // close to the price of none once the scene is batched.
        app.insert_resource(Msaa::Sample2);
    }
    // Collapse the static scenery into a few draws, as the match does. The
    // clearing is built first so its rocks and easel timber are merged too.
    // CANOPY_NOBATCH skips it, for back-to-back frame-rate comparisons.
    if std::env::var("CANOPY_NOBATCH").is_err() {
        app.add_systems(
            Startup,
            cube_soccer::rendering::batching::merge_static_draws
                .after(cube_soccer::creation::scene::build_clearing),
        );
    }
    stylized::register_shader(&mut app);
    app.add_plugins(MaterialPlugin::<stylized::JungleMaterial>::default())
        .add_plugins(CreationPlugin)
        .add_systems(Startup, redirect_output)
        .add_systems(Startup, (setup_lighting, build_jungle))
        .add_systems(Update, (animate_jungle, stylized::stylize))
        .add_systems(Update, capture)
        // Frame rate once a second on stderr, so "is it laggy" is a number.
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin)
        .add_plugins(bevy::diagnostic::LogDiagnosticsPlugin::default())
        .add_systems(PreUpdate, self_test.after(bevy::input::InputSystem))
        .run();
}

/// Set CANOPY_CAPTURE to a PNG path for a reproducible screenshot.
fn capture(
    mut frames: Local<u32>,
    mut screenshots: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut exit: EventWriter<bevy::app::AppExit>,
) {
    let Ok(path) = std::env::var("CANOPY_CAPTURE") else {
        return;
    };
    *frames += 1;
    let at = std::env::var("CANOPY_CAPTURE_FRAME")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(90)
        .clamp(1, 3600);
    if *frames == at {
        screenshots.save_screenshot_to_disk(window.single(), path).unwrap();
    }
    if *frames == at + 20 {
        exit.send(bevy::app::AppExit);
    }
}

/// Set CANOPY_SITTING_SELFTEST to a folder to have the app run its own flow.
///
/// It drives the real input path — the window's cursor position, the mouse
/// button and the keyboard state — rather than calling the systems directly,
/// so a pass here means a person doing the same thing gets the same result.
/// It paints the appearance in two colours, saves, paints the superpower,
/// saves, reviews, continues, and screenshots each step into that folder.
#[allow(clippy::too_many_arguments)]
fn self_test(
    mut frame: Local<u32>,
    mut windows: Query<(Entity, &mut Window)>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut shots: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    mut exit: EventWriter<bevy::app::AppExit>,
    phase: Res<State<cube_soccer::creation::CreationPhase>>,
    cameras: Query<(&Camera, &GlobalTransform), With<cube_soccer::creation::camera::CreationCamera>>,
    canvases: Query<&GlobalTransform, With<cube_soccer::creation::scene::CanvasSurface>>,
    dishes: Query<(&GlobalTransform, &cube_soccer::creation::paint::PigmentDish)>,
) {
    let Ok(dir) = std::env::var("CANOPY_SITTING_SELFTEST") else {
        return;
    };
    *frame += 1;
    let f = *frame;
    if phase.is_changed() {
        info!("SELFTEST frame {f}: phase is {:?}", phase.get());
    }
    let (Ok((window_entity, mut window)), Ok((camera, camera_tf)), Ok(canvas)) =
        (windows.get_single_mut(), cameras.get_single(), canvases.get_single())
    else {
        return;
    };
    // A point on the canvas face, in its own -0.5..0.5 space, as a screen position.
    let on_canvas = |x: f32, y: f32| {
        camera.world_to_viewport(camera_tf, canvas.transform_point(Vec3::new(x, y, 0.5)))
    };
    let dish = |index: usize| {
        dishes
            .iter()
            .find(|(_, d)| d.0 == index)
            .and_then(|(tf, _)| camera.world_to_viewport(camera_tf, tf.translation()))
    };
    let along = |range: &std::ops::Range<u32>| (f - range.start) as f32 / (range.end - range.start) as f32;

    let ink = 300..360;
    let orange = 380..440;
    let blue = 540..600;
    let target = if ink.contains(&f) {
        let t = along(&ink);
        on_canvas(-0.35 + t * 0.7, 0.35 - t * 0.7)
    } else if f == 370 {
        dish(2)
    } else if orange.contains(&f) {
        let t = along(&orange);
        on_canvas(0.35 - t * 0.7, 0.35 - t * 0.7)
    } else if f == 530 {
        dish(3)
    } else if blue.contains(&f) {
        // A zigzag, so the superpower is recognisably a different painting.
        let t = along(&blue);
        on_canvas(-0.3 + t * 0.6, ((t * 6.0).fract() - 0.5) * 0.5)
    } else {
        None
    };
    match target {
        Some(position) => {
            window.set_cursor_position(Some(position));
            buttons.press(MouseButton::Left);
        }
        None => buttons.release(MouseButton::Left),
    }

    // Enter: save appearance, save superpower, continue from review.
    if [470, 630, 760].contains(&f) {
        keys.press(KeyCode::Enter);
    } else {
        keys.release(KeyCode::Enter);
    }
    let shot = match f {
        460 => Some("st-1-appearance"),
        620 => Some("st-2-superpower"),
        750 => Some("st-3-review"),
        1120 => Some("st-4-landed"),
        _ => None,
    };
    if let Some(name) = shot {
        let _ = shots.save_screenshot_to_disk(window_entity, format!("{dir}/{name}.png"));
    }
    if f == 1150 {
        exit.send(bevy::app::AppExit);
    }
}

/// CANOPY_OUTPUT_ROOT sends the saved paintings somewhere other than ./output.
fn redirect_output(mut session: ResMut<cube_soccer::creation::persistence::CreationSession>) {
    if let Ok(root) = std::env::var("CANOPY_OUTPUT_ROOT") {
        session.root = root.into();
    }
}
