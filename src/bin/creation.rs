//! Standalone preview of the painting clearing.
//!
//! Runs just the creation phase plus the jungle world behind it, so the scene
//! can be looked at and painted on without building the whole match.

use bevy::prelude::*;
use cube_soccer::creation::CreationPlugin;
use cube_soccer::tactics::TacticsPlugin;
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
        .add_plugins(TacticsPlugin)
        .add_systems(Startup, redirect_output)
        .add_systems(Startup, (setup_lighting, build_jungle))
        .add_systems(Update, (animate_jungle, stylized::stylize))
        .add_systems(Update, capture)
        // Frame rate once a second on stderr, so "is it laggy" is a number.
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin)
        .add_plugins(bevy::diagnostic::LogDiagnosticsPlugin::default())
        .add_systems(PreUpdate, (self_test, self_test_table).chain().after(bevy::input::InputSystem))
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

/// The frame the easel's driver hands the mouse to the table's driver.
const EASEL_UNTIL: u32 = 800;

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
    // Only while the easel has the floor: after that the table's driver owns
    // the mouse, and two drivers releasing each other's clicks paints nothing.
    if f < EASEL_UNTIL {
        match target {
            Some(position) => {
                window.set_cursor_position(Some(position));
                buttons.press(MouseButton::Left);
            }
            None => buttons.release(MouseButton::Left),
        }
    }

    // One key at a time, released on every other frame. Holding a key down
    // means the next press of it is not a fresh press, which silently skipped
    // the stop and left the session recording.
    let wanted = match f {
        470 | 630 | 760 => Some(KeyCode::Enter), // save, save, continue to the table
        820 => Some(KeyCode::KeyR),              // start the clock
        980 => Some(KeyCode::KeyA),              // arrow in hand
        1180 => Some(KeyCode::KeyR),             // stop the clock
        1260 => Some(KeyCode::Enter),            // leave for the match
        _ => None,
    };
    for key in [KeyCode::Enter, KeyCode::KeyR, KeyCode::KeyA] {
        if wanted == Some(key) {
            keys.press(key);
        } else {
            keys.release(key);
        }
    }

    let shot = match f {
        460 => Some("st-1-appearance"),
        620 => Some("st-2-superpower"),
        750 => Some("st-3-review"),
        1210 => Some("st-4-table"),
        1560 => Some("st-5-landed"),
        _ => None,
    };
    if let Some(name) = shot {
        let _ = shots.save_screenshot_to_disk(window_entity, format!("{dir}/{name}.png"));
    }
    if f == 1600 {
        exit.send(bevy::app::AppExit);
    }
}

/// Drives the board itself once the camera has settled at the table: drags a
/// token across the halfway line, then draws an arrow behind it.
fn self_test_table(
    mut frame: Local<u32>,
    mut windows: Query<&mut Window>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    cameras: Query<(&Camera, &GlobalTransform), With<cube_soccer::creation::camera::CreationCamera>>,
    plane: Option<Res<cube_soccer::tactics::board::BoardPlane>>,
) {
    if std::env::var("CANOPY_SITTING_SELFTEST").is_err() {
        return;
    }
    *frame += 1;
    let f = *frame;
    if f < EASEL_UNTIL {
        return;
    }
    let (Ok(mut window), Ok((camera, camera_tf)), Some(plane)) =
        (windows.get_single_mut(), cameras.get_single(), plane)
    else {
        return;
    };
    let on_board = |at: Vec2| camera.world_to_viewport(camera_tf, plane.world(at) + plane.normal() * 0.12);

    let drag = 860..950;
    let arrow = 1010..1110;
    let target = if drag.contains(&f) {
        // Player 7 pushed up out of the near line.
        let t = (f - drag.start) as f32 / (drag.end - drag.start) as f32;
        on_board(Vec2::new(0.64, 0.64).lerp(Vec2::new(0.58, 0.44), t))
    } else if arrow.contains(&f) {
        let t = (f - arrow.start) as f32 / (arrow.end - arrow.start) as f32;
        on_board(Vec2::new(0.50, 0.40).lerp(Vec2::new(0.32, 0.18), t))
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
}

/// CANOPY_OUTPUT_ROOT sends the saved paintings somewhere other than ./output.
fn redirect_output(mut session: ResMut<cube_soccer::creation::persistence::CreationSession>) {
    if let Ok(root) = std::env::var("CANOPY_OUTPUT_ROOT") {
        session.root = root.into();
    }
}
