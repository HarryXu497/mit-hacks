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
                .set(ImagePlugin::default_nearest())
                // Bevy resolves `assets/` against the manifest or the executable, never the
                // working directory, so this has to be said explicitly or nothing loads -- and a
                // missing glTF fails silently.
                .set(cube_soccer::assets::asset_plugin()),
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
        // Live, cosmetic shouts to your own players during a match.
        .add_plugins(tactic_lab_native::shout::ShoutPlugin)
        // Turns the painted superpower into one of the game's four, and arms the coached side.
        .add_plugins(ForgePlugin)
        .add_systems(Update, (capture, autoplay, autoplay_report, bench_start, bench_dump))
        .run();
}

/// Set `TACTIC_LAB_AUTOPLAY` to walk the whole flow without touching the mouse.
///
/// A smoke test for the phase transitions themselves. Every phase change spawns, despawns and
/// re-parents things, and a unit test cannot see a panic that only happens with a renderer
/// attached -- which is exactly the class of bug that reaches a player and never reaches CI.
fn autoplay(
    mut frames: Local<u32>,
    phase: Res<State<AppPhase>>,
    mut next_phase: ResMut<NextState<AppPhase>>,
    mut next_creation: ResMut<NextState<cube_soccer::creation::CreationPhase>>,
) {
    if std::env::var("TACTIC_LAB_AUTOPLAY").is_err() {
        return;
    }
    *frames += 1;
    // Generous gaps: the world batches on the first frames and a glTF character lands later.
    let step = |at: u32| *frames == at;
    use cube_soccer::creation::CreationPhase;

    if step(60) {
        info!("autoplay: lobby -> the easel");
        next_creation.set(CreationPhase::PaintingAppearance);
        next_phase.set(AppPhase::Creation);
    } else if step(150) {
        info!("autoplay: the easel -> the table  (phase now {:?})", phase.get());
        next_creation.set(CreationPhase::Coaching);
    } else if step(260) {
        // Only the in-world step, exactly as the table's Enter key does it. This used to set
        // `AppPhase::Game` directly as well -- which bypassed the very link that was missing, so
        // the harness reported a working match while the real game had no match running at all.
        // A smoke test must never take a shortcut the player cannot take.
        info!("autoplay: leaving the island  (phase now {:?})", phase.get());
        next_creation.set(CreationPhase::Departing);
        let _ = &mut next_phase;
    } else if *frames % 60 == 0 && *frames >= 300 {
        info!("autoplay: still alive at {:?}", phase.get());
    }
}

/// Print what the app actually believes, rather than what it was asked to do.
// A diagnostic that reports on everything at once, which is what makes it useful and also what
// makes it wide. Not a shipped system.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn autoplay_report(
    mut frames: Local<u32>,
    phase: Res<State<AppPhase>>,
    creation: Res<State<cube_soccer::creation::CreationPhase>>,
    table: Res<State<cube_soccer::tactics::TableState>>,
    match_state: Res<State<cube_soccer::game::MatchState>>,
    lifecycle: Res<tactic_lab_native::CoachingLifecycle>,
    marked: Query<(Entity, &Transform, Option<&Camera>), With<cube_soccer::systems::camera::MainCamera>>,
    every_camera: Query<(
        Entity,
        &Camera,
        &Transform,
        &GlobalTransform,
        Option<&Parent>,
        Option<&cube_soccer::systems::camera::MainCamera>,
        Option<&cube_soccer::creation::camera::CreationCamera>,
        Option<&cube_soccer::systems::camera::CameraRig>,
    )>,
    players: Query<(&cube_soccer::entities::CubePlayer, &Transform, &bevy_rapier3d::prelude::Velocity)>,
    ball: Query<(&Transform, &bevy_rapier3d::prelude::Velocity), With<cube_soccer::entities::Ball>>,
    possession: Res<cube_soccer::systems::possession::Possession>,
    rapier: Res<bevy_rapier3d::prelude::RapierContext>,
    bodies: Query<Entity, With<cube_soccer::entities::CubePlayer>>,
    groups: Query<&bevy_rapier3d::prelude::CollisionGroups, With<cube_soccer::entities::CubePlayer>>,
) {
    if std::env::var("TACTIC_LAB_AUTOPLAY").is_err() {
        return;
    }
    *frames += 1;
    if *frames % 120 != 0 {
        return;
    }
    let all_cameras: Vec<_> = every_camera
        .iter()
        .map(|(e, c, t, g, p, m, cr, rig)| {
            (e, c.clone(), t.translation, g.translation(), p.map(|p| p.get()), m.is_some(), cr.is_some(), rig.is_some())
        })
        .collect();
    for (entity, transform, camera) in &marked {
        info!(
            "  MAIN-MARKED {entity:?}: is_camera={} at={:?}",
            camera.is_some(),
            transform.translation
        );
    }
    let main = marked.iter().next().map(|(_, t, _)| t.translation);
    for (entity, camera, local, global, parent, is_main, is_creation, has_rig) in &all_cameras {
        info!(
            "  camera {entity:?}: order={} window={} main={is_main} creation={is_creation} parent={parent:?} rig={has_rig} local={local:?} global={global:?}",
            camera.order,
            matches!(camera.target, bevy::render::camera::RenderTarget::Window(_)),
        );
    }
    let moving = players.iter().filter(|(_, _, v)| v.linvel.length() > 0.2).count();
    // Does anyone ever actually touch the ball? A ball that is never struck stays at rest at the
    // centre spot, which is precisely what continuous collision detection being off looked like.
    if *phase.get() == AppPhase::Game {
        // The decisive measurement: does Rapier report any contact at all between players?
        let touching = bodies
            .iter()
            .map(|e| rapier.contact_pairs_with(e).count())
            .sum::<usize>();
        let first_groups = groups.iter().next().map(|g| (g.memberships.bits(), g.filters.bits()));
        info!("  player contact pairs={touching}  collision_groups={first_groups:?}");
        if let Ok((ball_t, ball_v)) = ball.get_single() {
            let nearest = players
                .iter()
                .map(|(_, t, _)| {
                    Vec2::new(t.translation.x, t.translation.z)
                        .distance(Vec2::new(ball_t.translation.x, ball_t.translation.z))
                })
                .fold(f32::INFINITY, f32::min);
            info!(
                "  ball at ({:.1},{:.1}) speed={:.2} nearest_player={:.2} holder={:?}",
                ball_t.translation.x,
                ball_t.translation.z,
                ball_v.linvel.length(),
                nearest,
                possession.holder.is_some(),
            );
        }
    }
    if *phase.get() == AppPhase::Game {
        let mut rows: Vec<String> = players
            .iter()
            .map(|(p, t, v)| {
                format!(
                    "{:?}#{} at ({:.1},{:.1}) v={:.1}  expected ({:.1},{:.1})",
                    p.team,
                    p.index,
                    t.translation.x,
                    t.translation.z,
                    v.linvel.length(),
                    cube_soccer::entities::get_spawn_position(p.team, p.index).x,
                    cube_soccer::entities::get_spawn_position(p.team, p.index).z,
                )
            })
            .collect();
        rows.sort();
        for row in rows {
            info!("  {row}");
        }
    }
    let lowest = players
        .iter()
        .map(|(_, t, _)| t.translation.y)
        .fold(f32::INFINITY, f32::min);
    info!(
        "state: app={:?} creation={:?} table={:?} match={:?} coaching_active={} players={} moving={} lowest_y={:.1} main_camera={:?}",
        phase.get(),
        creation.get(),
        table.get(),
        match_state.get(),
        lifecycle.active,
        players.iter().count(),
        moving,
        lowest,
        main,
    );
}

/// Set `TACTIC_LAB_CAPTURE` to a PNG path for a reproducible screenshot of the running app.
///
/// The same affordance `cube-soccer`'s own preview binary has, and for the same reason: a screen
/// is the only honest way to check a screen, and "it looked right on my machine" is not a record.
/// `TACTIC_LAB_CAPTURE_FRAME` picks the frame, which matters because the world takes a moment to
/// batch and any glTF character arrives a little after that.
fn capture(
    mut frames: Local<u32>,
    mut shot: Local<u32>,
    mut screenshots: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut exit: EventWriter<bevy::app::AppExit>,
) {
    let Ok(dir) = std::env::var("TACTIC_LAB_CAPTURE") else {
        return;
    };
    let Ok(window) = window.get_single() else {
        return;
    };
    *frames += 1;

    // A burst rather than one frame: a still cannot show movement, and movement is the thing
    // under suspicion. `TACTIC_LAB_CAPTURE` is a directory; frames land in it numbered.
    let first = std::env::var("TACTIC_LAB_CAPTURE_FRAME")
        .ok()
        .and_then(|f| f.parse::<u32>().ok())
        .unwrap_or(420);
    let every = std::env::var("TACTIC_LAB_CAPTURE_EVERY")
        .ok()
        .and_then(|f| f.parse::<u32>().ok())
        .unwrap_or(24)
        .max(1);
    let count = std::env::var("TACTIC_LAB_CAPTURE_COUNT")
        .ok()
        .and_then(|f| f.parse::<u32>().ok())
        .unwrap_or(8);

    if *frames >= first && *shot < count && (*frames - first) % every == 0 {
        let path = format!("{dir}/frame-{:02}.png", *shot);
        let _ = screenshots.save_screenshot_to_disk(window, path);
        *shot += 1;
    }
    if *shot >= count && *frames > first + count * every + 90 {
        exit.send(bevy::app::AppExit);
    }
}

/// The same dump the baseline worktree prints, so the two builds can be compared line for line.
///
/// Identical format on purpose: the question is not "does it look wrong" but "where do the two
/// diverge", and that is a diff, not an opinion.
fn bench_dump(
    mut frames: Local<u32>,
    phase: Res<State<AppPhase>>,
    players: Query<(&cube_soccer::entities::CubePlayer, &Transform, &bevy_rapier3d::prelude::Velocity)>,
    ball: Query<(&Transform, &bevy_rapier3d::prelude::Velocity), With<cube_soccer::entities::Ball>>,
) {
    *frames += 1;
    if *phase.get() != AppPhase::Game || *frames % 60 != 0 {
        return;
    }
    let mut rows: Vec<String> = players
        .iter()
        .map(|(p, t, v)| {
            format!(
                "POS f={:04} {:?}#{} x={:+07.2} z={:+07.2} v={:05.2}",
                *frames, p.team, p.index, t.translation.x, t.translation.z, v.linvel.length()
            )
        })
        .collect();
    rows.sort();
    for row in rows {
        println!("{row}");
    }
    if let Ok((t, v)) = ball.get_single() {
        println!(
            "POS f={:04} BALL      x={:+07.2} z={:+07.2} v={:05.2}",
            *frames, t.translation.x, t.translation.z, v.linvel.length()
        );
    }
}

/// The baseline worktree's entry into the match, replicated exactly.
///
/// Straight to `AppPhase::Game` at frame 90 -- no lobby, no easel, no stone table. That is the
/// experiment: if the soccer is broken from here too, the engine is at fault; if it plays, then
/// whatever the creation and coaching path does to the world is.
fn bench_start(mut frames: Local<u32>, mut next: ResMut<NextState<AppPhase>>) {
    if std::env::var("TACTIC_LAB_BENCH").is_err() {
        return;
    }
    *frames += 1;
    if *frames == 90 {
        next.set(AppPhase::Game);
    }
}
