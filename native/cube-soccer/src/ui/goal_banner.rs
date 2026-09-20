//! The goal banner: a cartoon slab of celebration, carrying the side's own characters.
//!
//! The look is the one worked out on the `coaching` branch -- a jagged gold starburst, jungle
//! leaves, a ball, drifting confetti, and GOAL! set in heavy outlined gold that pops in and fades
//! out. What is different here is who is doing the celebrating.
//!
//! # Why the characters are genuinely the ones on the pitch
//!
//! That branch flanks the banner with monkeys built from primitives, which look the part but are
//! their own thing: re-dress a side and the banner does not follow. These are the real models.
//! [`WornCharacters`] is per *side*, not per player -- all five players on a team wear one path --
//! so "the scorer's model" is just `WornCharacters.get(scoring_team)`. Two stands per side are
//! kept dressed from that same resource by [`dress_the_podium`], which is
//! [`wear_characters`](crate::entities::wear_characters) in miniature, and an orthographic camera
//! photographs whichever pair just scored onto the banner's texture. Forge a monkey mid-match and
//! the next banner shows it, because it is reading the field the pitch reads.
//!
//! They are kept dressed *continuously* rather than on the goal, because a glTF load takes
//! several frames and a banner that is only up for two seconds would spend the first of them
//! showing an empty stage.
//!
//! # The three things that make this harder than it looks
//!
//! `RenderLayers` does not propagate to children in Bevy 0.13, and a glTF scene's meshes arrive
//! as descendants some frames after the scene is asked for. Nothing marks them for us, so
//! [`keep_the_podium_on_its_layer`] walks down and does it; without that the stage photographs
//! empty.
//!
//! Each stand carries a [`CharacterSkin`] of its own, which is not decoration. `stylize`
//! permanently swaps any unclaimed `StandardMaterial` for the cel-shaded jungle material, which
//! reads on textured art as bands of dark, and it decides by walking *up* from each surface
//! looking for exactly that component. Marking the surfaces from here would lose the race --
//! `stylize` runs every frame, and an inserted component is a deferred command that does not land
//! until the schedule's next sync point. Wearing the skin wins it outright, and as a bonus
//! [`light_the_characters`](crate::entities::light_the_characters) then treats these surfaces
//! exactly as it treats the pitch's.
//!
//! And the text is drawn as ordinary screen UI over the photograph rather than into it, because
//! Bevy 0.13 measures text with the window's DPI even when the target is an image -- text
//! rendered into the texture comes out at the wrong size on a Retina display.

use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::hierarchy::HierarchyQueryExt;
use bevy::prelude::*;
use bevy::render::camera::{ClearColorConfig, RenderTarget, ScalingMode};
use bevy::render::mesh::PrimitiveTopology;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::view::RenderLayers;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;

use crate::entities::character::CharacterSkin;
use crate::entities::WornCharacters;
use crate::game::{GameState, Team};
use crate::rendering::batching::NoMerge;
use crate::rendering::stylized::Unstylised;

/// Only the banner camera draws this layer, and it draws nothing else. Layer 1 belongs to the
/// tactics sign; an entity with no `RenderLayers` at all sits on layer 0, which is why the two
/// cameras cannot see each other's subjects.
const LAYER: u8 = 2;

/// The photograph, and the space the text is laid out in. 2.5:1, which is the shape the banner
/// keeps at every window size.
const WIDTH: f32 = 1600.0;
const HEIGHT: f32 = 640.0;

/// How long the banner stays up, how long it takes to arrive, and how long to leave.
///
/// Longer than the `RESET_DELAY_SECS` pause it starts in, deliberately: lengthening that pause
/// would change kickoff timing and the reset cadence a trained policy was trained against, so the
/// banner outlives it instead and fades away over the restart.
const BANNER_SECS: f32 = 2.2;
const ENTER_SECS: f32 = 0.2;
const FADE_SECS: f32 = 0.3;

/// Where the little stage is built: far under the pitch. The layer already hides it, and the
/// distance means a stray unlayered entity cannot wander into shot either.
const STAGE_HOME: Vec3 = Vec3::new(0.0, -600.0, 0.0);

/// How far out the two characters stand, and how big they are drawn.
///
/// The stage is 16 units across and 6.4 tall. They sit out at the ends so the word in the middle
/// has room, and stand on `-1.3` so a 1.4-unit model scaled up reads as centred rather than
/// hanging from the top of the frame.
const STAND_X: f32 = 5.65;
const STAND_Y: f32 = -1.3;
const STAND_SCALE: f32 = 1.6;

/// The banner's state: who is being celebrated, and how long for.
#[derive(Resource, Default)]
pub struct GoalCelebration {
    showing: Option<Showing>,
    /// The scoreline the banner has already reacted to.
    seen: [u32; 2],
}

struct Showing {
    team: Team,
    score: [u32; 2],
    elapsed: f32,
}

#[derive(Component)]
pub struct GoalBannerCamera;
/// The 3D stage, and everything on it.
#[derive(Component)]
pub struct GoalBannerStage;
/// The UI node the photograph is shown in.
#[derive(Component)]
pub struct GoalBannerImage;
/// One side's half of the stage, shown only when that side scores.
#[derive(Component)]
pub struct BannerTeam(Team);
/// One of a side's two characters, and the bob it rides.
#[derive(Component)]
pub struct BannerMonkey {
    base: Vec3,
    phase: f32,
}
/// A scrap of confetti, and the drift it rides.
#[derive(Component)]
pub struct GoalConfetti {
    base: Vec3,
    phase: f32,
}
/// A stand-in wearing what its side wears on the pitch.
#[derive(Component)]
pub struct PodiumStand {
    team: Team,
}
/// A podium surface already put on the banner's layer, so the pass is idempotent.
#[derive(Component)]
pub struct OnTheBannerLayer;
/// One piece of banner text, laid out in the design space and scaled to fit.
#[derive(Component)]
pub struct GoalBannerText {
    left: f32,
    top: f32,
    width: f32,
    font_size: f32,
    /// The scoreline, which is the one piece whose words change and which sits on a slab.
    label: bool,
}

impl GoalCelebration {
    fn start(&mut self, team: Team, score: [u32; 2]) {
        self.showing = Some(Showing { team, score, elapsed: 0.0 });
    }

    fn tick(&mut self, delta: f32) {
        if let Some(showing) = &mut self.showing {
            showing.elapsed += delta;
            if showing.elapsed >= BANNER_SECS {
                self.showing = None;
            }
        }
    }
}

/// Which side just scored, given the last scoreline the banner reacted to and the current one.
///
/// A goal is exactly one side gaining exactly one. Everything else is a resync and re-baselines
/// silently: a match reset drops the score, and a joiner's first snapshot can arrive mid-match
/// carrying a scoreline this client has never seen. Neither is a goal anybody just scored.
fn who_just_scored(seen: [u32; 2], now: [u32; 2]) -> Option<Team> {
    match (now[0].checked_sub(seen[0]), now[1].checked_sub(seen[1])) {
        (Some(1), Some(0)) => Some(Team::Orange),
        (Some(0), Some(1)) => Some(Team::Blue),
        _ => None,
    }
}

/// How far in the arrival is, 0 to 1.
fn entrance(elapsed: f32) -> f32 {
    (elapsed / ENTER_SECS).clamp(0.0, 1.0)
}

/// The overshoot the banner arrives on: up past its size and back down to it.
fn pop(elapsed: f32) -> f32 {
    let enter = entrance(elapsed);
    0.78 + 0.22 * (1.0 - (1.0 - enter).powi(3)) + 0.06 * (enter * std::f32::consts::PI).sin()
}

/// How opaque the banner is: in over [`ENTER_SECS`], out over the last [`FADE_SECS`].
fn opacity(elapsed: f32) -> f32 {
    entrance(elapsed).min(((BANNER_SECS - elapsed) / FADE_SECS).clamp(0.0, 1.0))
}

/// How big the banner is drawn in a window of this size, before the arrival overshoot.
///
/// Wide, but never so tall that it swallows the pitch: whichever of the two limits binds first
/// wins, so a tall narrow window gets a banner sized off its height instead of its width.
fn banner_width(window: Vec2) -> f32 {
    (window.x * 0.9).min(window.y * 0.55 * (WIDTH / HEIGHT))
}

fn flat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: color, perceptual_roughness: 0.92, ..default() })
}

fn glow(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: color, unlit: true, ..default() })
}

/// A piece of scenery on the stage.
///
/// Held out of both of the jungle's whole-scene passes. [`Unstylised`] keeps the cel pass off it:
/// this is flat cartoon art authored for the banner, and banding it into steps of dark is not
/// what it is for. [`NoMerge`] keeps the static-batching pass off it: that pass bakes props into
/// shared draws, and a banner piece baked in with the pitch would lose the render layer that is
/// the only reason it is not visible in the world.
fn part(
    c: &mut Commands,
    parent: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
) -> Entity {
    let piece = c
        .spawn((
            PbrBundle { mesh, material, transform, ..default() },
            RenderLayers::layer(LAYER),
            Unstylised,
            NoMerge,
        ))
        .id();
    c.entity(parent).add_child(piece);
    piece
}

/// The starburst behind the word: one jagged fan, so the whole burst is a single draw and stays
/// inside the transparent photograph rather than bleeding past its edge.
fn burst_mesh() -> Mesh {
    const POINTS: usize = 40;
    let point = |i: usize| {
        let angle = i as f32 * std::f32::consts::TAU / POINTS as f32;
        let (x, y) = if i % 2 == 0 { (7.15, 2.85) } else { (3.7, 1.22) };
        [angle.cos() * x, angle.sin() * y, 0.0]
    };
    let mut positions = Vec::<[f32; 3]>::new();
    for i in 0..POINTS {
        positions.extend([[0.0, 0.0, 0.0], point(i), point(i + 1)]);
    }
    let count = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
}

/// Build the stage, the camera that photographs it, and the UI the photograph is shown in.
///
/// The image store is optional for the reason `stylize` gives for taking its own store that way:
/// without a renderer there is no `Assets<Image>` at all, and a headless app -- the RL
/// environment, or an integration test playing a match with nothing on screen -- would otherwise
/// panic on a banner it was never going to draw. Absent it, this is a no-op, and every other
/// system here finds no stage and quietly does nothing too.
pub fn setup_goal_banner(
    mut c: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let Some(mut images) = images else { return };

    let size = Extent3d {
        width: WIDTH as u32,
        height: HEIGHT as u32,
        depth_or_array_layers: 1,
    };
    let mut target = Image {
        texture_descriptor: TextureDescriptor {
            label: Some("goal-banner"),
            size,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        ..default()
    };
    target.resize(size);
    let target = images.add(target);

    c.spawn((
        GoalBannerCamera,
        GoalBannerStage,
        Camera3dBundle {
            camera: Camera {
                // Ahead of the broadcast camera, so the photograph is ready in the frame that
                // shows it. The tactics sign has taken -1.
                order: -2,
                target: RenderTarget::Image(target.clone()),
                // Nothing behind the burst: the pitch shows through instead.
                clear_color: ClearColorConfig::Custom(Color::NONE),
                // Off between goals. A second 3D camera rendering every frame for a texture
                // nobody is looking at is a cost with no picture to show for it.
                is_active: false,
                ..default()
            },
            // Orthographic, so the two characters are the same size whichever end they stand at
            // and the burst behind them keeps its shape.
            projection: OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: 16.0, height: 6.4 },
                ..default()
            }
            .into(),
            // As the tactics sign does, and for its reason: this is authored art, and the filmic
            // curve would deliver it as something other than it was drawn.
            tonemapping: Tonemapping::None,
            dither: DebandDither::Disabled,
            transform: Transform::from_translation(STAGE_HOME + Vec3::Z * 30.0)
                .looking_at(STAGE_HOME, Vec3::Y),
            ..default()
        },
        RenderLayers::layer(LAYER),
    ));

    let stage = c
        .spawn((
            GoalBannerStage,
            SpatialBundle::from_transform(Transform::from_translation(STAGE_HOME)),
        ))
        .id();

    // The stage's own sun. The characters are unlit -- their form is painted into the texture --
    // so this is for the leaves and the ball, which are not.
    let sun = c
        .spawn((
            DirectionalLightBundle {
                directional_light: DirectionalLight {
                    illuminance: 9000.0,
                    shadows_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(-5.0, 8.0, 15.0).looking_at(Vec3::ZERO, Vec3::Y),
                ..default()
            },
            RenderLayers::layer(LAYER),
        ))
        .id();
    c.entity(stage).add_child(sun);

    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let leaf = meshes.add(Sphere::new(1.0).mesh().ico(0).unwrap());
    let gold = flat(&mut materials, Color::rgb(1.0, 0.70, 0.08));
    let cream = flat(&mut materials, Color::rgb(1.0, 0.95, 0.65));
    let green = flat(&mut materials, Color::rgb(0.25, 0.52, 0.16));
    let dark_green = flat(&mut materials, Color::rgb(0.12, 0.35, 0.19));
    let dark = flat(&mut materials, Color::rgb(0.035, 0.07, 0.065));

    // The burst, drawn twice: a slightly larger cream copy behind gives it its outline.
    let burst = meshes.add(burst_mesh());
    let burst_cream = glow(&mut materials, Color::rgb(1.0, 0.98, 0.72));
    let burst_gold = glow(&mut materials, Color::rgb(1.0, 0.77, 0.16));
    part(
        &mut c,
        stage,
        burst.clone(),
        burst_cream,
        Transform::from_xyz(0.0, 0.0, -2.1).with_scale(Vec3::splat(1.035)),
    );
    part(&mut c, stage, burst, burst_gold, Transform::from_xyz(0.0, 0.0, -2.0));

    // Leaves sweeping in from both corners.
    for side in [-1.0, 1.0] {
        for i in 0..4 {
            part(
                &mut c,
                stage,
                leaf.clone(),
                if i % 2 == 0 { green.clone() } else { dark_green.clone() },
                Transform::from_xyz(side * (4.3 + i as f32 * 0.55), -1.7 + i as f32 * 0.45, -1.0)
                    .with_rotation(Quat::from_rotation_z(side * (0.35 + i as f32 * 0.25)))
                    .with_scale(Vec3::new(2.1, 0.52, 0.15)),
            );
        }
    }

    // The ball, up over the word.
    let ball = part(
        &mut c,
        stage,
        meshes.add(Sphere::new(0.69).mesh().ico(1).unwrap()),
        cream,
        Transform::from_xyz(0.0, 2.05, 0.0),
    );
    for direction in [Vec3::Z, Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y] {
        part(
            &mut c,
            ball,
            leaf.clone(),
            dark.clone(),
            Transform::from_translation(direction * 0.66)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, direction))
                .with_scale(Vec3::new(0.28, 0.055, 0.28)),
        );
    }

    // One half-stage per side: its two characters, and its confetti in its own colour. Hidden
    // until that side scores.
    for team in [Team::Orange, Team::Blue] {
        let team_root = c
            .spawn((
                BannerTeam(team),
                SpatialBundle { visibility: Visibility::Hidden, ..default() },
            ))
            .id();
        c.entity(stage).add_child(team_root);

        for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let base = Vec3::new(side * STAND_X, STAND_Y, 1.0);
            let stand = c
                .spawn((
                    PodiumStand { team },
                    BannerMonkey { base, phase: i as f32 * 1.8 },
                    SpatialBundle::from_transform(
                        Transform::from_translation(base)
                            .with_scale(Vec3::splat(STAND_SCALE))
                            // Turned a little in toward the word, and tipped with it.
                            .with_rotation(
                                Quat::from_rotation_y(-side * 0.30)
                                    * Quat::from_rotation_z(-side * 0.13),
                            ),
                    ),
                    RenderLayers::layer(LAYER),
                ))
                .id();
            c.entity(team_root).add_child(stand);
        }

        let team_mat = flat(&mut materials, team.color());
        for i in 0..30 {
            let a = i as f32 * 2.39996;
            let base = Vec3::new(a.cos() * (3.8 + (i % 4) as f32), a.sin() * 2.35, 1.5);
            let scrap = part(
                &mut c,
                team_root,
                cube.clone(),
                if i % 3 == 0 { gold.clone() } else { team_mat.clone() },
                Transform::from_translation(base).with_scale(Vec3::new(0.14, 0.22, 0.08)),
            );
            c.entity(scrap).insert(GoalConfetti { base, phase: a });
        }
    }

    // The photograph, and the words over it.
    let image = c
        .spawn((
            GoalBannerImage,
            GoalBannerStage,
            ImageBundle {
                image: UiImage::new(target),
                style: Style { position_type: PositionType::Absolute, ..default() },
                visibility: Visibility::Hidden,
                // Over the power badges, and never in the way of a click.
                z_index: ZIndex::Global(100),
                focus_policy: FocusPolicy::Pass,
                ..default()
            },
        ))
        .id();

    // GOAL!, built up in layers: a dropped shadow, then a ring of dark to outline it, then the
    // gold face on top. Eighteen copies of one word is what gives it the weight it has -- the
    // default font has no outline of its own to lean on.
    let mut layers = vec![
        (10.0, 20.0, Color::rgb(0.30, 0.13, 0.035)),
        (5.0, 12.0, Color::rgb(0.90, 0.32, 0.025)),
    ];
    for (x, y) in [
        (-9.0, -9.0), (0.0, -10.0), (9.0, -9.0), (-10.0, 0.0),
        (10.0, 0.0), (-9.0, 9.0), (0.0, 10.0), (9.0, 9.0),
    ] {
        layers.push((x, y, Color::rgb(0.13, 0.12, 0.05)));
    }
    for (x, y) in [(-3.0, 0.0), (3.0, 0.0), (0.0, -3.0), (0.0, 3.0), (0.0, 0.0)] {
        layers.push((x, y, Color::rgb(1.0, 0.77, 0.13)));
    }
    for (x, y, color) in layers {
        spawn_banner_text(
            &mut c,
            image,
            "GOAL!",
            color,
            GoalBannerText { left: 350.0 + x, top: 175.0 + y, width: 900.0, font_size: 250.0, label: false },
        );
    }
    spawn_banner_text(
        &mut c,
        image,
        "",
        Color::rgb(1.0, 0.96, 0.77),
        GoalBannerText { left: 475.0, top: 468.0, width: 650.0, font_size: 36.0, label: true },
    );
}

fn spawn_banner_text(
    c: &mut Commands,
    parent: Entity,
    value: &str,
    color: Color,
    layout: GoalBannerText,
) {
    let slab = layout.label;
    let text = c
        .spawn(TextBundle {
            text: Text::from_section(
                value,
                TextStyle { font_size: layout.font_size, color, ..default() },
            ),
            focus_policy: FocusPolicy::Pass,
            ..default()
        })
        .id();
    let container = c
        .spawn((
            layout,
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                background_color: BackgroundColor(if slab {
                    Color::rgba(0.035, 0.10, 0.065, 0.94)
                } else {
                    Color::NONE
                }),
                focus_policy: FocusPolicy::Pass,
                ..default()
            },
        ))
        .id();
    c.entity(container).add_child(text);
    c.entity(parent).add_child(container);
}

/// Keep each stand wearing what its side is wearing on the pitch.
///
/// [`wear_characters`](crate::entities::wear_characters) in miniature, and for the same reasons:
/// the model is requested when the answer changes, and spawned only once it has genuinely
/// loaded, so a model that never arrives replaces nothing.
pub fn dress_the_podium(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    worn: Res<WornCharacters>,
    mut stands: Query<(Entity, &PodiumStand, Option<&mut CharacterSkin>, Option<&Children>)>,
) {
    for (entity, stand, skin, children) in &mut stands {
        // Owned, because the borrow of the resource cannot outlive this arm.
        let wanted = worn.get(stand.team).map(str::to_owned);

        match (wanted, skin) {
            // Already wearing exactly this. Reveal it once it has finished loading.
            (Some(wanted), Some(mut skin)) if skin.path == wanted => {
                if skin.revealed || !asset_server.is_loaded_with_dependencies(&skin.scene) {
                    continue;
                }
                let scene = skin.scene.clone();
                commands.entity(entity).with_children(|stand| {
                    // The model stands on y=0 in its own space, and the stand is already placed
                    // and scaled, so it hangs here unaltered.
                    stand.spawn(SceneBundle { scene, ..default() });
                });
                skin.revealed = true;
            }

            // A model is wanted, and either none or a different one is on.
            (Some(wanted), _) => {
                for child in children.into_iter().flatten() {
                    commands.entity(*child).despawn_recursive();
                }
                let scene = asset_server.load(&wanted);
                commands
                    .entity(entity)
                    .insert(CharacterSkin { path: wanted, scene, revealed: false });
            }

            // The side is back in the jungle's blocky character, which is built from primitives
            // rather than loaded from a file, so there is nothing to stand here. The banner shows
            // its burst and its confetti and no characters, rather than characters that are not
            // the ones on the pitch.
            (None, Some(_)) => {
                for child in children.into_iter().flatten() {
                    commands.entity(*child).despawn_recursive();
                }
                commands.entity(entity).remove::<CharacterSkin>();
            }
            (None, None) => {}
        }
    }
}

/// Put every part of a stand's model on the banner's layer.
///
/// `RenderLayers` is not inherited in Bevy 0.13 and a glTF scene's meshes arrive as descendants
/// well after the scene is requested, so the layer has to be handed down by hand. Without this
/// the camera photographs an empty stage.
///
/// [`Unstylised`] rides along as belt-and-braces. The stand's own [`CharacterSkin`] is what
/// actually keeps the cel pass off these surfaces -- it decides by walking up, which is immune to
/// the deferred-command race that marking from here would lose -- but a surface that is meant to
/// be left alone may as well say so.
pub fn keep_the_podium_on_its_layer(
    mut commands: Commands,
    stands: Query<Entity, With<PodiumStand>>,
    children: Query<&Children>,
    unmarked: Query<(), Without<OnTheBannerLayer>>,
) {
    for stand in &stands {
        for part in children.iter_descendants(stand) {
            if unmarked.get(part).is_ok() {
                commands.entity(part).insert((
                    OnTheBannerLayer,
                    RenderLayers::layer(LAYER),
                    Unstylised,
                ));
            }
        }
    }
}

/// Put the banner up when the scoreline rises.
///
/// Driven by the score rather than by `GoalScoredEvent` on purpose. A networked spectator never
/// runs `handle_goal_scored` -- the scoring systems are gated off for it -- and learns the score
/// only from the host's snapshot, so an event-driven banner would never appear on the joiner's
/// screen. The score is the one signal both machines already share, and reading it needs no
/// addition to the protocol.
pub fn raise_the_goal_banner(
    mut celebration: ResMut<GoalCelebration>,
    game_state: Res<GameState>,
) {
    let now = game_state.score;
    let scorer = who_just_scored(celebration.seen, now);
    // Re-baselined whatever happened, so a reset or a resync is absorbed rather than latched.
    celebration.seen = now;
    if let Some(team) = scorer {
        // A second goal replaces the first rather than queueing behind it.
        celebration.start(team, now);
    }
}

/// Run the banner's clock and drive everything it is made of.
#[allow(clippy::too_many_arguments)]
pub fn animate_goal_banner(
    time: Res<Time>,
    mut celebration: ResMut<GoalCelebration>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<GoalBannerCamera>>,
    mut image: Query<
        (&mut Style, &mut BackgroundColor, &mut Visibility),
        (With<GoalBannerImage>, Without<BannerTeam>, Without<GoalBannerText>),
    >,
    mut teams: Query<(&BannerTeam, &mut Visibility), Without<GoalBannerImage>>,
    mut monkeys: Query<(&BannerMonkey, &mut Transform), Without<GoalConfetti>>,
    mut confetti: Query<(&GoalConfetti, &mut Transform), Without<BannerMonkey>>,
    mut texts: Query<
        (&GoalBannerText, &Children, &mut Style, &mut BackgroundColor),
        Without<GoalBannerImage>,
    >,
    mut glyphs: Query<&mut Text>,
) {
    celebration.tick(time.delta_seconds());
    let showing = celebration.showing.as_ref();

    for mut camera in &mut cameras {
        camera.is_active = showing.is_some();
    }
    for (team, mut visibility) in &mut teams {
        *visibility = if showing.is_some_and(|s| s.team == team.0) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (mut style, mut tint, mut visibility) in &mut image {
        *visibility = if showing.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        let (Some(showing), Ok(window)) = (showing, windows.get_single()) else { continue };
        let width = banner_width(Vec2::new(window.width(), window.height())) * pop(showing.elapsed);
        let height = width / (WIDTH / HEIGHT);
        style.width = Val::Px(width);
        style.height = Val::Px(height);
        style.left = Val::Px((window.width() - width) * 0.5);
        style.top = Val::Px((window.height() - height) * 0.5);
        tint.0 = Color::rgba(1.0, 1.0, 1.0, opacity(showing.elapsed));
    }

    let Some(showing) = showing else { return };

    if let Ok(window) = windows.get_single() {
        // The text is laid out against the design space and scaled to whatever the banner ended
        // up, so the words keep their place on it at every window size.
        let scale =
            banner_width(Vec2::new(window.width(), window.height())) * pop(showing.elapsed) / WIDTH;
        let alpha = opacity(showing.elapsed);
        for (layout, children, mut style, mut background) in &mut texts {
            let Ok(mut text) = glyphs.get_mut(children[0]) else { continue };
            style.left = Val::Px(layout.left * scale);
            style.top = Val::Px(layout.top * scale);
            style.width = Val::Px(layout.width * scale);
            let section = &mut text.sections[0];
            section.style.font_size = layout.font_size * scale;
            section.style.color.set_a(alpha);
            if layout.label {
                style.padding = UiRect::vertical(Val::Px(10.0 * scale));
                background.0.set_a(0.94 * alpha);
                section.value = format!(
                    "{} SCORES!    {} - {}",
                    showing.team.name(),
                    showing.score[0],
                    showing.score[1]
                );
            }
        }
    }

    for (monkey, mut transform) in &mut monkeys {
        transform.translation =
            monkey.base + Vec3::Y * (showing.elapsed * 9.0 + monkey.phase).sin().abs() * 0.17;
    }
    for (scrap, mut transform) in &mut confetti {
        transform.translation = scrap.base
            + Vec3::new(
                (showing.elapsed * 2.0 + scrap.phase).sin() * 0.18,
                -showing.elapsed * 0.35,
                0.0,
            );
        transform.rotation = Quat::from_euler(
            EulerRot::XYZ,
            showing.elapsed * 2.0,
            scrap.phase,
            showing.elapsed * 3.0 + scrap.phase,
        );
    }
}

/// Take the banner down on a phase change.
///
/// `half_time` can pull the app into the coaching screen on the very goal that raised the banner,
/// which would otherwise leave it hanging over a screen it has nothing to do with.
pub fn clear_goal_banner(
    mut celebration: ResMut<GoalCelebration>,
    mut cameras: Query<&mut Camera, With<GoalBannerCamera>>,
    mut image: Query<&mut Visibility, With<GoalBannerImage>>,
) {
    celebration.showing = None;
    for mut camera in &mut cameras {
        camera.is_active = false;
    }
    for mut visibility in &mut image {
        *visibility = Visibility::Hidden;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_goal_is_one_side_gaining_one() {
        assert_eq!(who_just_scored([1, 1], [2, 1]), Some(Team::Orange));
        assert_eq!(who_just_scored([1, 1], [1, 2]), Some(Team::Blue));
        assert_eq!(who_just_scored([0, 0], [1, 0]), Some(Team::Orange));
    }

    #[test]
    fn a_still_scoreline_raises_nothing() {
        assert_eq!(who_just_scored([2, 3], [2, 3]), None);
        assert_eq!(who_just_scored([0, 0], [0, 0]), None);
    }

    /// A match reset drops the score. Without `checked_sub` the subtraction would wrap, and a
    /// banner appearing at a reset is exactly the kind of thing nobody would think to check.
    #[test]
    fn a_reset_is_absorbed_rather_than_celebrated() {
        assert_eq!(who_just_scored([3, 2], [0, 0]), None);
        assert_eq!(who_just_scored([3, 2], [3, 0]), None);
    }

    /// A joiner's first snapshot can carry a scoreline it has never seen. That is a resync, not
    /// four goals at once.
    #[test]
    fn a_joiner_catching_up_mid_match_does_not_celebrate() {
        assert_eq!(who_just_scored([0, 0], [3, 1]), None);
        // And having caught up, the next real goal still lands.
        assert_eq!(who_just_scored([3, 1], [3, 2]), Some(Team::Blue));
    }

    #[test]
    fn a_second_goal_replaces_the_first() {
        let mut state = GoalCelebration::default();
        state.start(Team::Orange, [1, 0]);
        state.tick(1.9);
        assert!(state.showing.is_some(), "the first banner should still be up");
        state.start(Team::Blue, [1, 1]);
        let showing = state.showing.as_ref().expect("the second banner should be up");
        assert_eq!(showing.team, Team::Blue);
        assert_eq!(showing.elapsed, 0.0, "the replacement starts its own clock");
    }

    #[test]
    fn the_banner_expires_on_its_own() {
        let mut state = GoalCelebration::default();
        state.start(Team::Orange, [1, 0]);
        state.tick(BANNER_SECS - 0.01);
        assert!(state.showing.is_some());
        state.tick(0.02);
        assert!(state.showing.is_none(), "the banner should take itself down");
    }

    #[test]
    fn the_banner_arrives_and_leaves() {
        assert_eq!(opacity(0.0), 0.0, "it fades in rather than popping");
        assert_eq!(opacity(ENTER_SECS), 1.0);
        // Approximate, because the hold's end is `BANNER_SECS - FADE_SECS` recombined and lands a
        // rounding step under 1.0 rather than on it.
        assert!((opacity(BANNER_SECS - FADE_SECS) - 1.0).abs() < 1e-5, "it holds full before it goes");
        assert_eq!(opacity(BANNER_SECS), 0.0);
        assert!(opacity(BANNER_SECS + 1.0) >= 0.0, "never negative");
    }

    /// The arrival overshoots its size and settles back onto it, which is what gives the pop.
    #[test]
    fn the_banner_pops_in_and_settles() {
        assert!(pop(0.0) < 1.0, "it starts small");
        let peak = (0..=20)
            .map(|i| pop(i as f32 * ENTER_SECS / 20.0))
            .fold(0.0f32, f32::max);
        assert!(peak > 1.0, "it should overshoot, peaked at {peak}");
        assert!((pop(ENTER_SECS) - 1.0).abs() < 1e-5, "and settle back to full size");
    }

    /// Wide, but never so tall it swallows the pitch.
    #[test]
    fn the_banner_is_sized_off_whichever_edge_binds_first() {
        // A wide short window is limited by its height.
        let short = banner_width(Vec2::new(3000.0, 600.0));
        assert!(short <= 600.0 * 0.55 * (WIDTH / HEIGHT) + 1e-3, "{short} is too tall for it");
        // A tall narrow one is limited by its width.
        let narrow = banner_width(Vec2::new(800.0, 3000.0));
        assert!((narrow - 720.0).abs() < 1e-3, "{narrow} should be 90% of the width");
    }

    /// The two characters stand clear of the word in the middle.
    #[test]
    fn the_characters_stand_out_of_the_words_way() {
        // The word is laid out from x=350 to x=1250 of 1600, which on a 16-wide stage is
        // -4.5 .. 4.5.
        let word_edge = (1250.0 / WIDTH - 0.5) * 16.0;
        assert!(STAND_X > word_edge, "a character at {STAND_X} would sit under the word");
    }

    #[test]
    fn a_side_is_named_for_the_banner() {
        assert_eq!(Team::Orange.name(), "ORANGE");
        assert_eq!(Team::Blue.name(), "BLUE");
    }
}
