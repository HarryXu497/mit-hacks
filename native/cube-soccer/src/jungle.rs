//! Procedural jungle presentation. All added geometry is collider-free.
use crate::entities::{Ball, CubePlayer};
use crate::game::config::*;
use crate::systems::display::DigitSegment;
use bevy::prelude::*;
pub(crate) mod landscape;

#[derive(Component)]
pub struct Waterfall {
    top: f32,
    bottom: f32,
    speed: f32,
}

#[derive(Component)]
pub struct WaterRipple {
    origin: Vec3,
    phase: f32,
    vertical: bool,
}

pub fn animate_water(time: Res<Time>, mut water: Query<(&mut Transform, &WaterRipple)>) {
    for (mut transform, ripple) in &mut water {
        let wave = (time.elapsed_seconds() * 2.8 + ripple.phase).sin();
        transform.translation = ripple.origin;
        if ripple.vertical {
            transform.translation.z += wave * 0.10;
        } else {
            transform.translation.y += wave * 0.07;
            transform.translation.z += (time.elapsed_seconds() * 0.6 + ripple.phase).sin() * 0.35;
        }
    }
}

pub(crate) struct Kit {
    pub(crate) cube: Handle<Mesh>,
    pub(crate) leaf: Handle<Mesh>,
    pub(crate) palm_frond: Handle<Mesh>,
    pub(crate) stone: Handle<Mesh>,
    pub(crate) terrain: Handle<Mesh>,
    pub(crate) grass: [Handle<StandardMaterial>; 2],
    pub(crate) greens: [Handle<StandardMaterial>; 3],
    pub(crate) rock: Handle<StandardMaterial>,
    pub(crate) rock_light: Handle<StandardMaterial>,
    pub(crate) wood: Handle<StandardMaterial>,
    pub(crate) wood_light: Handle<StandardMaterial>,
    pub(crate) rope: Handle<StandardMaterial>,
    pub(crate) gold: Handle<StandardMaterial>,
    pub(crate) white: Handle<StandardMaterial>,
    pub(crate) dark: Handle<StandardMaterial>,
    pub(crate) water_light: Handle<StandardMaterial>,
    pub(crate) orange: Handle<StandardMaterial>,
    pub(crate) blue: Handle<StandardMaterial>,
    pub(crate) face: Handle<StandardMaterial>,
    pub(crate) fur: Handle<StandardMaterial>,
    pub(crate) fur_dark: Handle<StandardMaterial>,
    pub(crate) muzzle: Handle<StandardMaterial>,
    pub(crate) eye_white: Handle<StandardMaterial>,
    pub(crate) ink: Handle<StandardMaterial>,
    pub(crate) flower_yellow: Handle<StandardMaterial>,
}
pub(crate) fn material(m: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    m.add(StandardMaterial {
        base_color: c,
        perceptual_roughness: 0.92,
        ..default()
    })
}
pub(crate) fn block(c: &mut Commands, k: &Kit, mat: Handle<StandardMaterial>, p: Vec3, size: Vec3) -> Entity {
    c.spawn(PbrBundle {
        mesh: k.cube.clone(),
        material: mat,
        transform: Transform::from_translation(p).with_scale(size),
        ..default()
    })
    .id()
}
pub(crate) fn oval(
    c: &mut Commands,
    k: &Kit,
    mat: Handle<StandardMaterial>,
    p: Vec3,
    size: Vec3,
    rotation: f32,
) -> Entity {
    c.spawn(PbrBundle {
        mesh: k.terrain.clone(),
        material: mat,
        transform: Transform::from_translation(p)
            .with_scale(size)
            .with_rotation(Quat::from_rotation_y(rotation)),
        ..default()
    })
    .id()
}
pub(crate) fn beam(c: &mut Commands, k: &Kit, mat: Handle<StandardMaterial>, a: Vec3, b: Vec3, width: f32) {
    let d = b - a;
    c.spawn(PbrBundle {
        mesh: k.cube.clone(),
        material: mat,
        transform: Transform::from_translation((a + b) * 0.5)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, d.normalize()))
            .with_scale(Vec3::new(width, d.length(), width)),
        ..default()
    });
}
fn palm_at(c: &mut Commands, k: &Kit, x: f32, z: f32, h: f32, phase: f32, ground: f32) {
    let bend = Vec3::new(phase.cos() * 0.8, 0., phase.sin() * 0.55);
    for segment in 0..4 {
        let t = segment as f32 / 4.;
        let u = (segment + 1) as f32 / 4.;
        let foot = Vec3::new(x, ground, z);
        beam(
            c,
            k,
            k.wood.clone(),
            foot + Vec3::Y * h * t + bend * t * t,
            foot + Vec3::Y * h * u + bend * u * u,
            0.32 * (1. - t * 0.38),
        );
    }
    let crown = Vec3::new(x, ground + h, z) + bend;
    for j in 0..7 {
        let a = j as f32 * std::f32::consts::TAU / 7. + phase;
        let rot = Quat::from_rotation_y(a) * Quat::from_rotation_z(0.10 + (j % 3) as f32 * 0.10);
        c.spawn(PbrBundle {
            mesh: k.palm_frond.clone(),
            material: k.greens[j % 3].clone(),
            transform: Transform::from_translation(crown)
                .with_rotation(rot)
                // Crowns scale with trunk height so tall palms read as palms at
                // broadcast distance instead of bristles on a pole.
                .with_scale(Vec3::splat(1.05 + h * 0.082) * (0.88 + (j % 3) as f32 * 0.12)),
            ..default()
        });
    }
}
/// Constant-width ink line around a form: an enlarged copy of the same mesh with
/// its front faces culled, so only the far side of the shell is drawn. Growing by
/// a fixed world offset rather than a percentage keeps the line the same weight
/// on a head and on a fingertip.
pub(crate) const INK: f32 = 0.022;

/// The original blocky character, restored. Kept tagged as an actor surface so
/// the restyled shading still separates players from the field.
pub(crate) fn monkey(c: &mut Commands, k: &Kit, parent: Entity, team: Team) {
    let fur = if team == Team::Orange {
        k.orange.clone()
    } else {
        k.blue.clone()
    };
    let parts = [
        (
            Vec3::new(0., 0., 0.),
            Vec3::new(0.8, 0.8, 0.65),
            fur.clone(),
        ),
        (
            Vec3::new(0., 0.57, 0.08),
            Vec3::new(1.05, 0.95, 0.9),
            fur.clone(),
        ),
        (
            Vec3::new(0., 0.51, 0.55),
            Vec3::new(0.76, 0.66, 0.08),
            k.face.clone(),
        ),
        (
            Vec3::new(-0.62, 0.55, 0.05),
            Vec3::new(0.25, 0.37, 0.25),
            fur.clone(),
        ),
        (
            Vec3::new(0.62, 0.55, 0.05),
            Vec3::new(0.25, 0.37, 0.25),
            fur.clone(),
        ),
        (
            Vec3::new(-0.19, 0.62, 0.61),
            Vec3::new(0.10, 0.19, 0.05),
            k.dark.clone(),
        ),
        (
            Vec3::new(0.19, 0.62, 0.61),
            Vec3::new(0.10, 0.19, 0.05),
            k.dark.clone(),
        ),
        (
            Vec3::new(-0.48, -0.15, 0.05),
            Vec3::new(0.25, 0.55, 0.27),
            fur.clone(),
        ),
        (
            Vec3::new(0.48, -0.15, 0.05),
            Vec3::new(0.25, 0.55, 0.27),
            fur.clone(),
        ),
        (
            Vec3::new(-0.24, -0.55, 0.1),
            Vec3::new(0.29, 0.34, 0.43),
            k.face.clone(),
        ),
        (
            Vec3::new(0.24, -0.55, 0.1),
            Vec3::new(0.29, 0.34, 0.43),
            k.face.clone(),
        ),
    ];
    for (p, s, m) in parts {
        let e = block(c, k, m, p, s);
        c.entity(e)
            .insert(crate::rendering::stylized::ActorSurface)
            // Tagged so a generated model can take the whole blocky character off in one pass.
            .insert(crate::entities::CharacterSkinReplaces);
        c.entity(parent).add_child(e);
        // Ink shell around the same box, at the same place, grown by a fixed
        // world offset: the outline treatment without touching the silhouette.
        let shell = c
            .spawn((
                PbrBundle {
                    mesh: k.cube.clone(),
                    material: k.ink.clone(),
                    transform: Transform::from_translation(p).with_scale(s + Vec3::splat(INK)),
                    ..default()
                },
                crate::entities::CharacterSkinReplaces,
            ))
            .id();
        c.entity(parent).add_child(shell);
    }
}

/// A crowd silhouette is intentionally chunkier than a player. The arms are
/// separate children so a wave reads as many individual people rather than a
/// single animated texture or a row of duplicate cubes.
fn crowd_monkey(c: &mut Commands, k: &Kit, parent: Entity, team: Team, variant: usize) {
    let fur = if team == Team::Orange {
        k.orange.clone()
    } else {
        k.blue.clone()
    };
    let accent = if variant % 3 == 0 {
        k.face.clone()
    } else if variant % 3 == 1 {
        k.gold.clone()
    } else {
        k.white.clone()
    };
    let bob = 0.92 + (variant % 4) as f32 * 0.045;
    let body = block(
        c,
        k,
        fur.clone(),
        Vec3::new(0., 0., 0.),
        Vec3::new(0.72, 0.8, 0.58) * bob,
    );
    c.entity(parent).add_child(body);
    for (p, s, m) in [
        (
            Vec3::new(0., 0.62, 0.04),
            Vec3::new(0.9, 0.82, 0.76) * bob,
            fur.clone(),
        ),
        (
            Vec3::new(0., 0.59, 0.38),
            Vec3::new(0.6, 0.52, 0.08) * bob,
            k.face.clone(),
        ),
        (
            Vec3::new(-0.23, 0.64, 0.57),
            Vec3::new(0.1, 0.16, 0.05) * bob,
            k.dark.clone(),
        ),
        (
            Vec3::new(0.23, 0.64, 0.57),
            Vec3::new(0.1, 0.16, 0.05) * bob,
            k.dark.clone(),
        ),
        (
            Vec3::new(0., 0.18, 0.34),
            Vec3::new(0.86, 0.13, 0.1) * bob,
            accent.clone(),
        ),
    ] {
        let e = block(c, k, m, p, s);
        c.entity(parent).add_child(e);
    }
    for (side, lean) in [(-1., -0.28), (1., 0.28)] {
        let e = block(
            c,
            k,
            fur.clone(),
            Vec3::new(side * 0.55, 0.17, 0.02),
            Vec3::new(0.16, 0.58, 0.2) * bob,
        );
        c.entity(e).insert(
            Transform::from_translation(Vec3::new(side * 0.55, 0.17, 0.02))
                .with_rotation(Quat::from_rotation_z(lean)),
        );
        c.entity(parent).add_child(e);
    }
}

/// The shared material and mesh set for everything in the jungle. Extracted so
/// the creation clearing is built from the exact same palette as the stadium:
/// one source of truth means the two screens cannot drift apart.
pub(crate) fn build_kit(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Kit {
    let mut rock_mesh = Sphere::new(1.).mesh().ico(1).unwrap();
    rock_mesh.duplicate_vertices();
    rock_mesh.compute_flat_normals();
    let terrain_mesh = Sphere::new(1.).mesh().ico(2).unwrap();
    Kit {
        cube: meshes.add(Cuboid::new(1., 1., 1.)),
        leaf: meshes.add(Sphere::new(1.).mesh().ico(0).unwrap()),
        palm_frond: meshes.add(landscape::palm_frond_mesh()),
        stone: meshes.add(rock_mesh),
        terrain: meshes.add(terrain_mesh),
        grass: [
            material(mats, Color::rgb(0.39, 0.65, 0.15)),
            material(mats, Color::rgb(0.47, 0.72, 0.20)),
        ],
        greens: [
            material(mats, Color::rgb(0.12, 0.35, 0.19)),
            material(mats, Color::rgb(0.25, 0.52, 0.16)),
            material(mats, Color::rgb(0.53, 0.72, 0.19)),
        ],
        rock: material(mats, Color::rgb(0.28, 0.34, 0.29)),
        rock_light: material(mats, Color::rgb(0.49, 0.48, 0.36)),
        wood: material(mats, Color::rgb(0.30, 0.15, 0.065)),
        wood_light: material(mats, Color::rgb(0.55, 0.30, 0.12)),
        rope: material(mats, Color::rgb(0.68, 0.46, 0.22)),
        gold: material(mats, Color::rgb(0.76, 0.48, 0.17)),
        white: material(mats, Color::rgb(0.98, 0.94, 0.74)),
        dark: material(mats, Color::rgb(0.035, 0.07, 0.065)),
        water_light: material(mats, Color::rgb(0.48, 0.86, 0.88)),
        orange: material(mats, Color::rgb(0.87, 0.36, 0.06)),
        blue: material(mats, Color::rgb(0.08, 0.34, 0.85)),
        face: material(mats, Color::rgb(0.98, 0.77, 0.42)),
        fur: material(mats, Color::rgb(0.67, 0.41, 0.18)),
        fur_dark: material(mats, Color::rgb(0.42, 0.23, 0.10)),
        muzzle: material(mats, Color::rgb(0.94, 0.77, 0.54)),
        eye_white: material(mats, Color::rgb(0.98, 0.98, 0.96)),
        // Outline shell: front faces culled so only the inside of an enlarged
        // copy is drawn, which reads as an ink line around the form it wraps.
        // Unlit, so the stylise pass leaves it alone.
        ink: mats.add(StandardMaterial {
            base_color: Color::rgb(0.05, 0.04, 0.06),
            unlit: true,
            cull_mode: Some(bevy::render::render_resource::Face::Front),
            ..default()
        }),
        flower_yellow: material(mats, Color::rgb(1.0, 0.72, 0.08)),
    }
}

pub fn build_jungle(
    mut c: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut old: Query<&mut Handle<Mesh>, (Without<DigitSegment>, Without<Ball>)>,
    players: Query<(Entity, &CubePlayer, Option<&Children>)>,
    // The animated node each player's visuals hang from, and whether it is already wearing a
    // generated character. See `entities::character`.
    visuals: Query<(Entity, Has<crate::entities::CharacterSkin>), With<crate::entities::PlayerVisual>>,
    balls: Query<Entity, With<Ball>>,
    mut digits: Query<(&mut Transform, &Handle<StandardMaterial>), With<DigitSegment>>,
) {
    // Remove only old draw meshes; preserve physics entities and scoreboard segments.
    for mut mesh in &mut old {
        *mesh = Handle::default();
    }
    let scoreboard_z = -FIELD_DEPTH / 2. - 3.0;
    // Keep the functional digits on the camera-facing side of the decorative panel.
    let scoreboard_target = Vec3::new(0., 5.8, scoreboard_z + 0.5);
    let scoreboard_source = Vec3::new(0., 5.25, -ARENA_DEPTH / 2. + WALL_THICKNESS + 0.25);
    for (mut t, mat) in &mut digits {
        // LED segments should remain readable in daylight and independent of
        // scene lighting; the existing score system still controls their colour.
        if let Some(material) = mats.get_mut(mat) {
            material.unlit = true;
        }
        t.translation = scoreboard_target + (t.translation - scoreboard_source) * 1.224;
        t.scale *= 1.224;
    }
    let k = build_kit(&mut meshes, &mut mats);
    c.insert_resource(ClearColor(Color::rgb(0.18, 0.38, 0.30)));
    // Continuous terrain meets the unchanged playable plane without a raised slab.
    for i in 0..(FIELD_WIDTH as usize / 2) {
        block(
            &mut c,
            &k,
            k.grass[i % 2].clone(),
            Vec3::new(-FIELD_WIDTH / 2. + 1. + i as f32 * 2., 1.12, 0.),
            Vec3::new(2., 0.04, FIELD_DEPTH),
        );
    }
    let field_edge_x = FIELD_WIDTH / 2. - 0.3;
    let field_edge_z = FIELD_DEPTH / 2. - 0.3;
    let y = 1.16;
    for x in [-field_edge_x, 0., field_edge_x] {
        block(
            &mut c,
            &k,
            k.white.clone(),
            Vec3::new(x, y, 0.),
            Vec3::new(0.09, 0.025, FIELD_DEPTH - 0.6),
        );
    }
    for z in [-field_edge_z, field_edge_z] {
        block(
            &mut c,
            &k,
            k.white.clone(),
            Vec3::new(0., y, z),
            Vec3::new(FIELD_WIDTH - 0.6, 0.025, 0.09),
        );
    }
    let center_circle_radius = FIELD_DEPTH * 0.19;
    for i in 0..64 {
        let a = i as f32 * std::f32::consts::TAU / 64.;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 64.;
        beam(
            &mut c,
            &k,
            k.white.clone(),
            Vec3::new(
                a.cos() * center_circle_radius,
                y,
                a.sin() * center_circle_radius,
            ),
            Vec3::new(
                b.cos() * center_circle_radius,
                y,
                b.sin() * center_circle_radius,
            ),
            0.075,
        );
    }
    let penalty_half_depth = GOAL_DEPTH / 2. + 2.5;
    let penalty_box_depth = 4.2;
    let penalty_inner_x = field_edge_x - penalty_box_depth;
    for side in [-1., 1.] {
        for z in [-penalty_half_depth, penalty_half_depth] {
            block(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(side * (field_edge_x - penalty_box_depth / 2.), y, z),
                Vec3::new(penalty_box_depth, 0.025, 0.09),
            );
        }
        block(
            &mut c,
            &k,
            k.white.clone(),
            Vec3::new(side * penalty_inner_x, y, 0.),
            Vec3::new(0.09, 0.025, penalty_half_depth * 2.),
        );
        let goal_x = side * FIELD_WIDTH / 2.;
        let goal_top = FIELD_HEIGHT + GOAL_HEIGHT;
        for z in [-GOAL_DEPTH / 2., GOAL_DEPTH / 2.] {
            block(
                &mut c,
                &k,
                k.gold.clone(),
                Vec3::new(goal_x, FIELD_HEIGHT + GOAL_HEIGHT / 2., z),
                Vec3::new(0.30, GOAL_HEIGHT, 0.30),
            );
            beam(
                &mut c,
                &k,
                k.wood.clone(),
                Vec3::new(goal_x, goal_top, z),
                Vec3::new(goal_x + side * GOAL_NET_DEPTH, FIELD_HEIGHT + 0.1, z),
                0.15,
            );
        }
        block(
            &mut c,
            &k,
            k.gold.clone(),
            Vec3::new(goal_x, goal_top, 0.),
            Vec3::new(0.3, 0.3, GOAL_DEPTH + 0.2),
        );
        for j in 0..((GOAL_DEPTH * 2.) as i32 + 1) {
            let z = -GOAL_DEPTH / 2. + j as f32 * 0.5;
            beam(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(goal_x + side * GOAL_NET_DEPTH, FIELD_HEIGHT + 0.1, z),
                Vec3::new(goal_x + side * GOAL_NET_DEPTH, goal_top, z),
                0.035,
            );
            beam(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(goal_x, goal_top, z),
                Vec3::new(goal_x + side * GOAL_NET_DEPTH, goal_top, z),
                0.035,
            );
        }
        for j in 0..11 {
            let h = FIELD_HEIGHT + 0.1 + j as f32 * (GOAL_HEIGHT / 10.);
            beam(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(goal_x + side * GOAL_NET_DEPTH, h, -GOAL_DEPTH / 2.),
                Vec3::new(goal_x + side * GOAL_NET_DEPTH, h, GOAL_DEPTH / 2.),
                0.035,
            );
        }
        let team = if side < 0. {
            k.orange.clone()
        } else {
            k.blue.clone()
        };
        block(
            &mut c,
            &k,
            k.wood.clone(),
            Vec3::new(side * (FIELD_WIDTH / 2. + 2.), 4., -FIELD_DEPTH / 2. - 1.5),
            Vec3::new(0.22, 6., 0.22),
        );
        block(
            &mut c,
            &k,
            team,
            Vec3::new(side * (FIELD_WIDTH / 2. + 1.2), 5., -FIELD_DEPTH / 2. - 1.5),
            Vec3::new(1.5, 2.5, 0.1),
        );
    }
    let score_block =
        |c: &mut Commands, k: &Kit, mat: Handle<StandardMaterial>, p: Vec3, size: Vec3| {
            block(
                c,
                k,
                mat,
                Vec3::new(
                    p.x * 1.7,
                    5.8 + (p.y - 4.1) * 1.35,
                    scoreboard_z + (p.z - scoreboard_z) * 1.7,
                ),
                Vec3::new(size.x * 1.7, size.y * 1.35, size.z),
            )
        };
    // Timber housing surrounds the existing functional seven-segment scoreboard.
    score_block(
        &mut c,
        &k,
        k.dark.clone(),
        Vec3::new(0., 4.1, scoreboard_z),
        Vec3::new(8.28, 3.46, 0.22),
    );
    for x in [-4.4, 4.4] {
        score_block(
            &mut c,
            &k,
            k.wood.clone(),
            Vec3::new(x, 3.1, scoreboard_z - 0.1),
            Vec3::new(0.5, 5.8, 0.65),
        );
    }
    for h in [2.2, 6.0] {
        score_block(
            &mut c,
            &k,
            k.gold.clone(),
            Vec3::new(0., h, scoreboard_z - 0.1),
            Vec3::new(9.4, 0.35, 0.7),
        );
    }
    for i in 0..3 {
        score_block(
            &mut c,
            &k,
            k.wood.clone(),
            Vec3::new(0., 6.3 + i as f32 * 0.5, scoreboard_z - 1.3),
            Vec3::new(8. - i as f32 * 1.5, 0.5, 1.5),
        );
    }
    let flame = mats.add(StandardMaterial {
        base_color: Color::rgb(1.0, 0.55, 0.04),
        emissive: Color::rgb(1.0, 0.25, 0.01) * 3.0,
        ..default()
    });
    for x in [-7.2, 7.2] {
        block(
            &mut c,
            &k,
            k.rock.clone(),
            Vec3::new(x, 2.1, scoreboard_z - 2.5),
            Vec3::new(0.8, 2.2, 0.8),
        );
        block(
            &mut c,
            &k,
            k.gold.clone(),
            Vec3::new(x, 3.25, scoreboard_z - 2.5),
            Vec3::new(1., 0.3, 1.),
        );
        c.spawn(PbrBundle {
            mesh: k.leaf.clone(),
            material: flame.clone(),
            transform: Transform::from_xyz(x, 3.8, scoreboard_z - 2.5)
                .with_scale(Vec3::new(0.35, 0.7, 0.35)),
            ..default()
        });
    }
    for i in 0..16 {
        if (5..11).contains(&i) {
            continue;
        }
        let x = (-4.2 + i as f32 * 0.55) * 1.7;
        let h = 8.6 - (i as f32 * 1.7).sin().abs() * 0.35;
        c.spawn(PbrBundle {
            mesh: k.leaf.clone(),
            material: k.greens[i % 3].clone(),
            transform: Transform::from_xyz(x, h, scoreboard_z + 0.4)
                .with_rotation(Quat::from_rotation_z(i as f32))
                .with_scale(Vec3::new(0.5, 0.15, 0.38)),
            ..default()
        });
    }
    // Dress each player that is not already wearing a generated character.
    //
    // The blocks go on the player's `PlayerVisual` node rather than on the body, for the same
    // reason everything else visible does: the body's transform belongs to Rapier and to
    // `movement.rs`, while the visual node is free to bob, lean and squash. Hung off the body
    // these would be the only part of the character that did not move.
    //
    // A player already wearing a forged model is skipped outright -- the generated character
    // replaces this one rather than layering over it.
    //
    // (The mesh-blanking pass above does not touch a generated character: its meshes belong to a
    // glTF scene that the asset server spawns asynchronously, frames after this one-shot system
    // has run.)
    for (body, player, children) in &players {
        let visual = children
            .into_iter()
            .flatten()
            .find_map(|child| visuals.get(*child).ok());

        match visual {
            Some((_, true)) => continue,
            Some((node, false)) => monkey(&mut c, &k, node, player.team),
            // No visual node at all: an app that spawned players without `spawn_player_with_eyes`.
            // Dress the body directly, as this did before the node existed.
            None => monkey(&mut c, &k, body, player.team),
        }
    }
    for ball in &balls {
        c.entity(ball)
            .insert(crate::rendering::stylized::ActorSurface);
        let shell = c
            .spawn(PbrBundle {
                mesh: meshes.add(Sphere::new(BALL_RADIUS + INK).mesh().uv(24, 14)),
                material: k.ink.clone(),
                ..default()
            })
            .id();
        c.entity(ball).add_child(shell);
        for direction in [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ] {
            let patch = c
                .spawn((
                    PbrBundle {
                        mesh: k.stone.clone(),
                        material: k.dark.clone(),
                        transform: Transform::from_translation(direction * (BALL_RADIUS - 0.015))
                            .with_rotation(Quat::from_rotation_arc(Vec3::Y, direction))
                            .with_scale(Vec3::new(0.18, 0.035, 0.18)),
                        ..default()
                    },
                    crate::rendering::stylized::ActorSurface,
                ))
                .id();
            c.entity(ball).add_child(patch);
        }
    }
    // A woven touchline fence makes the retained colliders feel like a
    // believable jungle stadium boundary. The gameplay colliders themselves
    // remain in the original entity systems and are never changed here.
    let extended_z = FIELD_DEPTH / 2. + SIDE_EXTENSION;
    for z in [-extended_z, extended_z] {
        beam(
            &mut c,
            &k,
            k.rope.clone(),
            Vec3::new(-FIELD_WIDTH / 2., 2.05, z),
            Vec3::new(FIELD_WIDTH / 2., 2.05, z),
            0.07,
        );
        for i in 0..13 {
            let x = -FIELD_WIDTH / 2. + i as f32 * FIELD_WIDTH / 12.;
            block(
                &mut c,
                &k,
                k.wood_light.clone(),
                Vec3::new(x, 1.6 + (i % 2) as f32 * 0.08, z),
                Vec3::new(0.22, 1.25, 0.22),
            );
        }
    }
    for side in [-1., 1.] {
        for z in [-1., 1.] {
            beam(
                &mut c,
                &k,
                k.rope.clone(),
                Vec3::new(
                    side * FIELD_WIDTH / 2.,
                    2.05,
                    z * (FIELD_DEPTH / 2. + SIDE_EXTENSION / 2.),
                ),
                Vec3::new(
                    side * FIELD_WIDTH / 2.,
                    2.05,
                    z * (FIELD_DEPTH / 2. + SIDE_EXTENSION),
                ),
                0.07,
            );
        }
        let gap_center_z = (FIELD_DEPTH / 2. + GOAL_DEPTH / 2.) / 2.;
        for z in [-gap_center_z, gap_center_z] {
            beam(
                &mut c,
                &k,
                k.rope.clone(),
                Vec3::new(side * FIELD_WIDTH / 2., 1.5, z),
                Vec3::new(side * FIELD_WIDTH / 2., 2.05, z),
                0.07,
            );
        }
        for j in 0..5 {
            let x = side * (FIELD_WIDTH / 2. - 10. + j as f32 * 1.3);
            block(
                &mut c,
                &k,
                k.wood_light.clone(),
                Vec3::new(x, 1.65, -FIELD_DEPTH / 2. - 1.0),
                Vec3::new(1.25, 0.22, 0.75),
            );
            let e = c
                .spawn(SpatialBundle {
                    transform: Transform::from_xyz(x, 2.25, -FIELD_DEPTH / 2. - 1.0)
                        .with_scale(Vec3::splat(0.46)),
                    ..default()
                })
                .id();
            crowd_monkey(
                &mut c,
                &k,
                e,
                if side < 0. { Team::Orange } else { Team::Blue },
                j,
            );
        }
    }
    landscape::build(&mut c, &k, &mut meshes, &mut mats);
}

pub fn animate_jungle(time: Res<Time>, mut water: Query<(&mut Transform, &Waterfall)>) {
    for (mut t, w) in &mut water {
        t.translation.y -= time.delta_seconds() * w.speed;
        if t.translation.y < w.bottom {
            t.translation.y = w.top;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_rapier3d::prelude::Collider;

    #[test]
    fn dressing_preserves_simulation_and_scoreboard() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Startup, build_jungle);
        let original_mesh = app
            .world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(1., 1., 1.));
        let pose = Transform::from_xyz(-6., 2.5, 0.);
        let player = app
            .world
            .spawn((
                CubePlayer {
                    team: Team::Orange,
                    index: 0,
                    can_jump: true,
                },
                pose,
                original_mesh.clone(),
                Collider::cuboid(0.75, 0.75, 0.75),
            ))
            .id();
        let digit = app.world.spawn((DigitSegment, original_mesh.clone())).id();
        app.update();
        assert_eq!(
            app.world.get::<Transform>(player).unwrap().translation,
            pose.translation
        );
        assert!(app.world.get::<Collider>(player).is_some());
        assert_eq!(app.world.query::<&Collider>().iter(&app.world).count(), 1);
        assert_eq!(
            app.world.get::<Handle<Mesh>>(digit).unwrap(),
            &original_mesh
        );
        assert!(app.world.get::<Children>(player).unwrap().len() >= 11);
        assert!(app.world.query::<&Handle<Mesh>>().iter(&app.world).count() > 300);
    }

    /// A player with a visual node is dressed on the *node*, not on the body.
    ///
    /// This is what lets the blocky character bob and lean with everything else; hung off the
    /// body it would be the only part of a player that never moved.
    #[test]
    fn the_blocky_character_is_dressed_onto_the_animated_node() {
        use crate::entities::{CharacterSkinReplaces, PlayerVisual};

        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Startup, build_jungle);

        let visual = app
            .world
            .spawn((PlayerVisual::new(0., 1.), SpatialBundle::default()))
            .id();
        let body = app
            .world
            .spawn((
                CubePlayer {
                    team: Team::Orange,
                    index: 0,
                    can_jump: true,
                },
                SpatialBundle::default(),
            ))
            .id();
        app.world.entity_mut(body).add_child(visual);
        app.update();

        let on_node = app.world.get::<Children>(visual).map_or(0, |c| c.len());
        assert!(
            on_node >= 11,
            "the character should hang off the animated node, found {on_node} children"
        );
        // The body keeps only the node itself.
        assert_eq!(app.world.get::<Children>(body).unwrap().len(), 1);

        // And every block is tagged, so a generated model can take the whole thing off at once.
        let tagged = app
            .world
            .query::<&CharacterSkinReplaces>()
            .iter(&app.world)
            .count();
        assert!(tagged >= 11, "blocks must be removable as a unit, found {tagged}");
    }

    /// A player already wearing a generated model is left alone entirely.
    #[test]
    fn a_player_wearing_a_generated_model_is_not_dressed_again() {
        use crate::entities::{CharacterSkin, PlayerVisual};

        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Startup, build_jungle);

        let visual = app
            .world
            .spawn((
                PlayerVisual::new(0., 1.),
                CharacterSkin {
                    path: "characters/base.glb#Scene0".to_owned(),
                    scene: Handle::default(),
                    revealed: true,
                },
                SpatialBundle::default(),
            ))
            .id();
        let body = app
            .world
            .spawn((
                CubePlayer {
                    team: Team::Blue,
                    index: 0,
                    can_jump: true,
                },
                SpatialBundle::default(),
            ))
            .id();
        app.world.entity_mut(body).add_child(visual);
        app.update();

        assert!(
            app.world.get::<Children>(visual).is_none(),
            "a generated character replaces the blocky one rather than layering over it"
        );
    }
}
