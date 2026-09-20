//! "The Sitting": the painting clearing on a spur peak beside the stadium.
//!
//! Everything here is built from the same [`Kit`] the stadium uses, so the
//! clearing cannot drift away from the match visually. All geometry is
//! collider-free and permanent: the peak is part of the world from the first
//! frame and stays standing through coaching and the match, which is what lets
//! the camera *travel* from here to the pitch instead of cutting.

use super::{CreationScene, EASEL_ANCHOR, PEAK, PEAK_TOP};
use crate::jungle::{beam, block, build_kit, material, monkey, oval, Kit, INK};
use crate::game::Team;
use bevy::prelude::*;

/// Canvas face size in world units. Tall rather than square: a standing figure
/// is the subject, and the taller frame also leaves the stadium visible past
/// the easel's edge rather than walling it off.
pub const CANVAS_W: f32 = 3.1;
pub const CANVAS_H: f32 = 3.9;
/// Forward lean of the canvas, matching how a real easel tips its board back.
pub const CANVAS_TILT: f32 = 0.13;

/// The model's heading: three-quarters toward the painter, a little toward the
/// easel. Any further round and his front falls out of the sun.
pub const MODEL_YAW: f32 = 0.30;

/// Marks the painting surface so the paint pass can find it without a name lookup.
#[derive(Component)]
pub struct CanvasSurface;

/// The second board, shown only in review, carrying the finished appearance
/// painting so both can be looked at together.
#[derive(Component)]
pub struct ReviewBoard;

/// The model's held pose drifts very slightly; see `breathe` in `mod.rs`.
#[derive(Component)]
pub struct PosingModel {
    pub rest: Vec3,
}

/// Builds the clearing. Runs once, at startup, regardless of phase.
pub fn build_clearing(
    mut c: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let k = build_kit(&mut meshes, &mut mats);

    spur(&mut c, &k, &mut meshes);
    let canvas = easel(&mut c, &k, &mut meshes, &mut mats, &mut images);
    model_stone(&mut c, &k);
    pigment_shelf(&mut c, &k, &mut mats);
    undergrowth(&mut c, &k);

    c.insert_resource(CreationScene { canvas });
}

/// The peak itself: a stacked rock spur rising out of the valley beside the
/// summit, wide enough at the top to stand a person, an easel and a model on.
/// Built from tapering slabs rather than a smooth cone so it reads as the same
/// quarried stone as the stadium's outcrops.
fn spur(c: &mut Commands, k: &Kit, meshes: &mut Assets<Mesh>) {
    let tiers = 15;
    for i in 0..tiers {
        let t = i as f32 / (tiers - 1) as f32;
        // Wide, irregular base narrowing to the flat cap. The sine term keeps
        // the stack from reading as a perfectly regular wedding cake.
        let width = 17.5 - t * 9.4 + (t * 9.0).sin() * 0.85;
        let depth = 15.0 - t * 8.1 + (t * 7.0).cos() * 0.8;
        let y = PEAK_TOP - 2.05 - t.powf(0.82) * 30.0;
        let lean = Vec3::new((t * 5.2).sin() * 0.9, 0., (t * 4.1).cos() * 0.75);
        let mat = if i % 3 == 0 {
            k.rock_light.clone()
        } else {
            k.rock.clone()
        };
        block(
            c,
            k,
            mat,
            PEAK + lean + Vec3::Y * y,
            Vec3::new(width, 2.6, depth),
        );
    }
    // Flat grassy cap. Two tones, same pair the pitch uses, so the ground the
    // easel stands on belongs to the same world as the ground the match is on.
    for i in 0..2 {
        let inset = i as f32 * 2.3;
        block(
            c,
            k,
            k.grass[i].clone(),
            PEAK + Vec3::Y * (PEAK_TOP - 0.42 + i as f32 * 0.12),
            Vec3::new(8.6 - inset, 0.9 - i as f32 * 0.3, 7.4 - inset),
        );
    }
    // A few boulders breaking the cap's edge, so the silhouette isn't a slab.
    for i in 0..7 {
        let a = i as f32 * 2.61;
        let r = 3.6 + (i as f32 * 1.7).sin() * 0.5;
        let p = PEAK + Vec3::new(a.cos() * r, PEAK_TOP - 0.3, a.sin() * r * 0.88);
        let s = 0.5 + (i as f32 * 0.9).sin().abs() * 0.45;
        c.spawn(PbrBundle {
            mesh: k.stone.clone(),
            material: k.rock_light.clone(),
            transform: Transform::from_translation(p).with_scale(Vec3::splat(s)),
            ..default()
        });
    }
    let _ = meshes;
}

/// Bamboo tripod, rope lashings, and the stretched bark canvas.
///
/// Returns the canvas entity. The canvas carries its own texture, created here
/// and handed to the paint pass; nothing else in the scene writes to it.
fn easel(
    c: &mut Commands,
    k: &Kit,
    _meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Entity {
    let base = EASEL_ANCHOR;
    let top = base + Vec3::new(0., CANVAS_H + 1.05, 0.);

    // Three legs: two forward, one back, like a real field easel.
    for (dx, dz) in [(-1.15_f32, 0.42_f32), (1.15, 0.42), (0., -1.25)] {
        beam(
            c,
            k,
            k.wood_light.clone(),
            base + Vec3::new(dx, -0.35, dz),
            top - Vec3::new(dx * 0.18, 0.9, dz * 0.18),
            0.17,
        );
    }
    // Cross-brace and the ledge the canvas rests on.
    let ledge_y = base.y + 0.55;
    beam(
        c,
        k,
        k.wood.clone(),
        base + Vec3::new(-1.35, ledge_y - base.y, 0.46),
        base + Vec3::new(1.35, ledge_y - base.y, 0.46),
        0.2,
    );
    // Rope lashing where the legs meet, the handmade detail the art direction asks for.
    for i in 0..4 {
        let y = top.y - 0.75 - i as f32 * 0.17;
        beam(
            c,
            k,
            k.rope.clone(),
            Vec3::new(base.x - 0.5, y, base.z - 0.3),
            Vec3::new(base.x + 0.5, y, base.z - 0.3),
            0.075,
        );
    }

    // The canvas: a thin slab carrying a paintable texture on its face. Both
    // paintings are created here; the easel starts on the appearance.
    let paintings = super::paint::Paintings::new(images);
    let surface = mats.add(StandardMaterial {
        base_color_texture: Some(paintings.appearance.display.clone()),
        // Unlit: a painting surface has to show pigment exactly as chosen, with
        // no shadow from the frame falling across the work. It also keeps the
        // cel pass off it, which would band the painter's own colours.
        unlit: true,
        ..default()
    });
    let centre = base + Vec3::new(0., 0.62 + CANVAS_H * 0.5, 0.52);
    let tilt = Quat::from_rotation_x(-CANVAS_TILT);
    review_board(c, k, mats, paintings.appearance.display.clone());
    c.insert_resource(paintings);

    // Ink shell behind the canvas, matching every other form in the world.
    c.spawn(PbrBundle {
        mesh: k.cube.clone(),
        material: k.ink.clone(),
        transform: Transform::from_translation(centre)
            .with_rotation(tilt)
            .with_scale(Vec3::new(CANVAS_W + INK * 6., CANVAS_H + INK * 6., 0.14 + INK * 4.)),
        ..default()
    });
    // Bark frame edging, warm against the pale canvas.
    for (off, size) in [
        (Vec3::new(0., CANVAS_H * 0.5 + 0.1, 0.), Vec3::new(CANVAS_W + 0.34, 0.2, 0.2)),
        (Vec3::new(0., -CANVAS_H * 0.5 - 0.1, 0.), Vec3::new(CANVAS_W + 0.34, 0.2, 0.2)),
        (Vec3::new(-CANVAS_W * 0.5 - 0.1, 0., 0.), Vec3::new(0.2, CANVAS_H + 0.34, 0.2)),
        (Vec3::new(CANVAS_W * 0.5 + 0.1, 0., 0.), Vec3::new(0.2, CANVAS_H + 0.34, 0.2)),
    ] {
        c.spawn(PbrBundle {
            mesh: k.cube.clone(),
            material: k.wood_light.clone(),
            transform: Transform::from_translation(centre + tilt * off)
                .with_rotation(tilt)
                .with_scale(size),
            ..default()
        });
    }

    c.spawn((
        PbrBundle {
            mesh: k.cube.clone(),
            material: surface,
            transform: Transform::from_translation(centre)
                .with_rotation(tilt)
                .with_scale(Vec3::new(CANVAS_W, CANVAS_H, 0.12)),
            ..default()
        },
        CanvasSurface,
        // The paint pass finds the canvas by entity and repaints its texture,
        // so it must survive the static-draw merge as its own object.
        crate::rendering::batching::NoMerge,
    ))
    .id()
}

/// A smaller board propped to the left of the easel. Hidden until review, when
/// it shows the finished appearance beside the superpower on the easel — the
/// 3D answer to the 2D screen's two side-by-side previews.
fn review_board(
    c: &mut Commands,
    k: &Kit,
    mats: &mut Assets<StandardMaterial>,
    appearance: Handle<Image>,
) {
    let (w, h) = (CANVAS_W * 0.8, CANVAS_H * 0.8);
    let centre = EASEL_ANCHOR + Vec3::new(-3.3, 0.35 + h * 0.5, 1.55);
    let lean = Quat::from_rotation_y(0.22) * Quat::from_rotation_x(-0.2);
    let face = mats.add(StandardMaterial {
        base_color_texture: Some(appearance),
        unlit: true,
        ..default()
    });
    let parts = [
        (k.ink.clone(), Vec3::new(w + INK * 6., h + INK * 6., 0.12 + INK * 4.), 0.0),
        (k.wood_light.clone(), Vec3::new(w + 0.3, h + 0.3, 0.1), -0.03),
    ];
    for (material, size, z) in parts {
        c.spawn((
            PbrBundle {
                mesh: k.cube.clone(),
                material,
                transform: Transform::from_translation(centre + lean * Vec3::new(0., 0., z))
                    .with_rotation(lean)
                    .with_scale(size),
                visibility: Visibility::Hidden,
                ..default()
            },
            ReviewBoard,
            crate::rendering::batching::NoMerge,
        ));
    }
    c.spawn((
        PbrBundle {
            mesh: k.cube.clone(),
            material: face,
            transform: Transform::from_translation(centre + lean * Vec3::new(0., 0., 0.03))
                .with_rotation(lean)
                .with_scale(Vec3::new(w, h, 0.1)),
            visibility: Visibility::Hidden,
            ..default()
        },
        ReviewBoard,
        crate::rendering::batching::NoMerge,
    ));
}

/// Brings the second board out for review and puts it away otherwise.
pub fn show_review_board(
    phase: Res<State<super::CreationPhase>>,
    mut boards: Query<(&mut Visibility, &Handle<StandardMaterial>), With<ReviewBoard>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if !phase.is_changed() {
        return;
    }
    let shown = *phase.get() == super::CreationPhase::Review;
    for (mut visibility, material) in &mut boards {
        *visibility = if shown { Visibility::Inherited } else { Visibility::Hidden };
        // The board's image has been repainted since its material was built.
        // Touching the material is what makes the finished painting show up,
        // rather than the blank bark it was created with.
        if shown {
            let _ = mats.get_mut(material);
        }
    }
}

/// The model's stone, and the monkey holding his pose on it.
///
/// He is deliberately *not* wired to the canvas. He is the reference being
/// painted, not a preview of the painting, so he never changes while you work.
fn model_stone(c: &mut Commands, k: &Kit) {
    let stone = EASEL_ANCHOR + Vec3::new(-4.55, -0.1, -1.5);
    // Warm flat rock, tilted a touch so it looks sat-upon rather than placed.
    c.spawn(PbrBundle {
        mesh: k.stone.clone(),
        material: k.rock_light.clone(),
        transform: Transform::from_translation(stone)
            .with_scale(Vec3::new(1.9, 0.72, 1.7))
            .with_rotation(Quat::from_rotation_z(0.06)),
        ..default()
    });

    // The model himself, turned to face the easel rather than the camera.
    let rest = stone + Vec3::new(0., 1.33, 0.);
    let body = c
        .spawn((
            SpatialBundle::from_transform(
                Transform::from_translation(rest)
                    .with_rotation(Quat::from_rotation_y(MODEL_YAW))
                    .with_scale(Vec3::splat(1.06)),
            ),
            PosingModel { rest },
        ))
        .id();
    monkey(c, k, body, Team::Orange);
}

/// Ground pigment in stone dishes on a shelf beside the easel. These are the
/// colour controls: diegetic objects you click, not a toolbar drawn over the
/// world. `paint.rs` gives them their behaviour.
fn pigment_shelf(c: &mut Commands, k: &Kit, mats: &mut Assets<StandardMaterial>) {
    let shelf = EASEL_ANCHOR + Vec3::new(2.62, 0.16, 0.95);
    block(
        c,
        k,
        k.rock.clone(),
        shelf,
        Vec3::new(1.5, 0.34, 3.5),
    );
    // The wet rag, folded at the near end: click it to wipe pigment off.
    c.spawn((
        PbrBundle {
            mesh: k.cube.clone(),
            material: k.white.clone(),
            transform: Transform::from_translation(shelf + Vec3::new(0., 0.24, 1.42))
                .with_rotation(Quat::from_rotation_y(0.3))
                .with_scale(Vec3::new(0.62, 0.12, 0.4)),
            ..default()
        },
        super::paint::Rag,
        crate::rendering::batching::NoMerge,
    ));
    for (i, rgb) in super::paint::PIGMENTS.iter().enumerate() {
        let row = i / 2;
        let col = i % 2;
        let p = shelf
            + Vec3::new(
                -0.34 + col as f32 * 0.68,
                0.24,
                -1.32 + row as f32 * 0.66,
            );
        // The dish.
        c.spawn(PbrBundle {
            mesh: k.stone.clone(),
            material: k.rock_light.clone(),
            transform: Transform::from_translation(p).with_scale(Vec3::new(0.29, 0.1, 0.29)),
            ..default()
        });
        // The pigment pooled in it, at full authored chroma.
        let colour = material(mats, Color::rgb(rgb[0], rgb[1], rgb[2]));
        c.spawn((
            PbrBundle {
                mesh: k.stone.clone(),
                material: colour,
                transform: Transform::from_translation(p + Vec3::Y * 0.07)
                    .with_scale(Vec3::new(0.21, 0.06, 0.21)),
                ..default()
            },
            super::paint::PigmentDish(i),
            // Clicked by position, so each pool stays a separate entity.
            crate::rendering::batching::NoMerge,
        ));
    }
}

/// Leaves, sprigs and a couple of flowers around the cap, framing the shot
/// without crowding the easel or the model.
fn undergrowth(c: &mut Commands, k: &Kit) {
    for i in 0..18 {
        let a = i as f32 * 2.399;
        let r = 3.1 + (i as f32 * 0.7).sin() * 0.9;
        let p = PEAK + Vec3::new(a.cos() * r, PEAK_TOP + 0.15, a.sin() * r * 0.9);
        // Keep the working triangle — easel, model, painter — clear.
        if p.z > EASEL_ANCHOR.z + 0.4 && p.x.abs() < PEAK.x.abs() + 3.0 {
            continue;
        }
        let s = 0.5 + (i as f32 * 1.3).sin().abs() * 0.4;
        for leaf in 0..3 {
            let b = leaf as f32 * 2.09 + i as f32;
            oval(
                c,
                k,
                k.greens[i % 3].clone(),
                p + Vec3::new(b.cos() * s * 0.5, 0.2 + leaf as f32 * 0.16, b.sin() * s * 0.5),
                Vec3::new(s * 0.8, s * 0.3, s * 0.55),
                b,
            );
        }
        if i % 6 == 0 {
            for petal in 0..5 {
                let b = petal as f32 * 1.256;
                oval(
                    c,
                    k,
                    k.flower_yellow.clone(),
                    p + Vec3::new(b.cos() * 0.3, 0.6, b.sin() * 0.3),
                    Vec3::new(0.33, 0.11, 0.17),
                    b,
                );
            }
        }
    }
}
