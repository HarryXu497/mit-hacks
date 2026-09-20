//! "The Sitting": the painting clearing, on one of the stadium's own islands.
//!
//! Everything here is built from the same [`Kit`] the stadium uses, so the
//! clearing cannot drift away from the match visually. The island is not new:
//! it is one of the outcrops the landscape already raises around the bowl, and
//! the clearing simply furnishes its top. All geometry is collider-free and
//! permanent, so the camera can *travel* from here to the pitch instead of
//! cutting, and the easel is still standing on the mountainside during play.
//!
//! Positions are written in the clearing's own frame — x right, y up from the
//! island's grass table, z toward the painter — and turned to face the stadium
//! by [`place`] and [`facing`]. Nothing here hardcodes a world coordinate.

use super::{facing, place, CreationScene, EASEL_LOCAL};
use crate::game::Team;
use crate::jungle::{beam, block, build_kit, material, monkey, oval, Kit, INK};
use bevy::prelude::*;

/// Canvas face size in world units. Tall rather than square: a standing figure
/// is the subject, and the taller frame also leaves the stadium visible past
/// the easel's edge rather than walling it off.
pub const CANVAS_W: f32 = 3.1;
pub const CANVAS_H: f32 = 3.9;
/// Forward lean of the canvas, matching how a real easel tips its board back.
pub const CANVAS_TILT: f32 = 0.13;

/// The model's heading within the clearing's frame: three-quarters toward the
/// painter, a little toward the easel. Any further round and his front falls
/// out of the sun.
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

    let canvas = easel(&mut c, &k, &mut mats, &mut images);
    model_stone(&mut c, &k);
    pigment_shelf(&mut c, &k, &mut mats);
    undergrowth(&mut c, &k);

    c.insert_resource(CreationScene { canvas });
}

/// A box in the clearing's frame: placed and turned with the clearing.
fn slab(c: &mut Commands, k: &Kit, mat: Handle<StandardMaterial>, local: Vec3, size: Vec3) -> Entity {
    c.spawn(PbrBundle {
        mesh: k.cube.clone(),
        material: mat,
        transform: Transform::from_translation(place(local))
            .with_rotation(facing())
            .with_scale(size),
        ..default()
    })
    .id()
}

/// Bamboo tripod, rope lashings, and the stretched bark canvas.
///
/// Returns the canvas entity. The canvas carries its own texture, created here
/// and handed to the paint pass; nothing else in the scene writes to it.
fn easel(
    c: &mut Commands,
    k: &Kit,
    mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Entity {
    let base = EASEL_LOCAL;
    let top = base + Vec3::new(0., CANVAS_H + 1.05, 0.);

    // Three legs: two forward, one back, like a real field easel.
    for (dx, dz) in [(-1.15_f32, 0.42_f32), (1.15, 0.42), (0., -1.25)] {
        beam(
            c,
            k,
            k.wood_light.clone(),
            place(base + Vec3::new(dx, -0.35, dz)),
            place(top - Vec3::new(dx * 0.18, 0.9, dz * 0.18)),
            0.17,
        );
    }
    // Cross-brace and the ledge the canvas rests on.
    beam(
        c,
        k,
        k.wood.clone(),
        place(base + Vec3::new(-1.35, 0.55, 0.46)),
        place(base + Vec3::new(1.35, 0.55, 0.46)),
        0.2,
    );
    // Rope lashing where the legs meet, the handmade detail the art direction asks for.
    for i in 0..4 {
        let y = top.y - 0.75 - i as f32 * 0.17;
        beam(
            c,
            k,
            k.rope.clone(),
            place(Vec3::new(base.x - 0.5, y, base.z - 0.3)),
            place(Vec3::new(base.x + 0.5, y, base.z - 0.3)),
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
    // Turned with the clearing, then tipped back on its own axis.
    let lean = facing() * Quat::from_rotation_x(-CANVAS_TILT);
    review_board(c, k, mats, paintings.appearance.display.clone());
    c.insert_resource(paintings);

    // Ink shell behind the canvas, matching every other form in the world.
    c.spawn(PbrBundle {
        mesh: k.cube.clone(),
        material: k.ink.clone(),
        transform: Transform::from_translation(place(centre))
            .with_rotation(lean)
            .with_scale(Vec3::new(CANVAS_W + INK * 6., CANVAS_H + INK * 6., 0.1)),
        ..default()
    });
    // Bark frame edging, warm against the pale canvas. Sits a little proud of
    // the ink shell so the timber, not the outline, is what borders the work.
    for (off, size) in [
        (Vec3::new(0., CANVAS_H * 0.5 + 0.1, 0.), Vec3::new(CANVAS_W + 0.34, 0.2, 0.26)),
        (Vec3::new(0., -CANVAS_H * 0.5 - 0.1, 0.), Vec3::new(CANVAS_W + 0.34, 0.2, 0.26)),
        (Vec3::new(-CANVAS_W * 0.5 - 0.1, 0., 0.), Vec3::new(0.2, CANVAS_H + 0.34, 0.26)),
        (Vec3::new(CANVAS_W * 0.5 + 0.1, 0., 0.), Vec3::new(0.2, CANVAS_H + 0.34, 0.26)),
    ] {
        c.spawn(PbrBundle {
            mesh: k.cube.clone(),
            material: k.wood_light.clone(),
            transform: Transform::from_translation(place(centre) + lean * off)
                .with_rotation(lean)
                .with_scale(size),
            ..default()
        });
    }

    c.spawn((
        PbrBundle {
            mesh: k.cube.clone(),
            material: surface,
            transform: Transform::from_translation(place(centre))
                .with_rotation(lean)
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
    let centre = place(EASEL_LOCAL + Vec3::new(-3.3, 0.35 + h * 0.5, 1.55));
    let lean = facing() * Quat::from_rotation_y(0.22) * Quat::from_rotation_x(-0.2);
    let face = mats.add(StandardMaterial {
        base_color_texture: Some(appearance),
        unlit: true,
        ..default()
    });
    // Back to front: ink shell, timber frame, then the painting itself. Each
    // step forward is larger than the last is thick, so the frame reads as a
    // frame rather than being swallowed by the outline behind it.
    let parts = [
        (k.ink.clone(), Vec3::new(w + INK * 6., h + INK * 6., 0.1), 0.0),
        (k.wood_light.clone(), Vec3::new(w + 0.3, h + 0.3, 0.1), 0.06),
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
            transform: Transform::from_translation(centre + lean * Vec3::new(0., 0., 0.12))
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
    let stone = EASEL_LOCAL + Vec3::new(-4.55, -0.1, -1.5);
    // Warm flat rock, tilted a touch so it looks sat-upon rather than placed.
    c.spawn(PbrBundle {
        mesh: k.stone.clone(),
        material: k.rock_light.clone(),
        transform: Transform::from_translation(place(stone))
            .with_scale(Vec3::new(1.9, 0.72, 1.7))
            .with_rotation(Quat::from_rotation_z(0.06)),
        ..default()
    });

    // The model himself, turned to face the easel rather than the camera.
    let rest = place(stone + Vec3::new(0., 1.33, 0.));
    let body = c
        .spawn((
            SpatialBundle::from_transform(
                Transform::from_translation(rest)
                    .with_rotation(facing() * Quat::from_rotation_y(MODEL_YAW))
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
    let shelf = EASEL_LOCAL + Vec3::new(2.62, 0.16, 0.95);
    slab(c, k, k.rock.clone(), shelf, Vec3::new(1.5, 0.34, 3.5));
    // The wet rag, folded at the near end: click it to wipe pigment off.
    c.spawn((
        PbrBundle {
            mesh: k.cube.clone(),
            material: k.white.clone(),
            transform: Transform::from_translation(place(shelf + Vec3::new(0., 0.24, 1.42)))
                .with_rotation(facing() * Quat::from_rotation_y(0.3))
                .with_scale(Vec3::new(0.62, 0.12, 0.4)),
            ..default()
        },
        super::paint::Rag,
        crate::rendering::batching::NoMerge,
    ));
    for (i, rgb) in super::paint::PIGMENTS.iter().enumerate() {
        let row = i / 2;
        let col = i % 2;
        let p = place(shelf + Vec3::new(-0.34 + col as f32 * 0.68, 0.24, -1.32 + row as f32 * 0.66));
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

/// Leaves and a few flowers on the island's turf, framing the shot.
///
/// The island already carries its own rim planting; this is the near dressing
/// that makes the working area read as a used clearing rather than a stage.
fn undergrowth(c: &mut Commands, k: &Kit) {
    for i in 0..18 {
        let a = i as f32 * 2.399;
        let r = 4.2 + (i as f32 * 0.7).sin() * 1.1;
        let local = Vec3::new(a.cos() * r, 0.15, a.sin() * r * 0.9);
        // Leave the whole middle of the island clear. The easel, the tactics
        // table and the sightline between them all run down this corridor, and
        // a single leaf in it fills the frame at these camera distances.
        if local.x.abs() < 4.6 {
            continue;
        }
        let s = 0.5 + (i as f32 * 1.3).sin().abs() * 0.4;
        for leaf in 0..3 {
            let b = leaf as f32 * 2.09 + i as f32;
            oval(
                c,
                k,
                k.greens[i % 3].clone(),
                place(local + Vec3::new(b.cos() * s * 0.5, 0.2 + leaf as f32 * 0.16, b.sin() * s * 0.5)),
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
                    place(local + Vec3::new(b.cos() * 0.3, 0.6, b.sin() * 0.3)),
                    Vec3::new(0.33, 0.11, 0.17),
                    b,
                );
            }
        }
    }
    let _ = block;
}
