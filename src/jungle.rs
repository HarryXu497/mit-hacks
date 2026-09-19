//! Procedural jungle presentation. All added geometry is collider-free.
use crate::entities::{Ball, CubePlayer};
use crate::game::config::*;
use crate::systems::display::DigitSegment;
use bevy::prelude::*;

#[derive(Component)]
pub struct Sway {
    base: Quat,
    phase: f32,
}
#[derive(Component)]
pub struct Waterfall {
    top: f32,
    bottom: f32,
    speed: f32,
}

/// Each spectator participates in a traveling stadium wave.
#[derive(Component)]
pub struct CrowdWave {
    base_y: f32,
    phase: f32,
}

#[derive(Component)]
pub struct CrowdArm {
    parent: Entity,
    side: f32,
}

pub fn animate_crowd_arms(
    time: Res<Time>,
    crowd: Query<&CrowdWave>,
    mut arms: Query<(&mut Transform, &CrowdArm)>,
) {
    for (mut t, arm) in &mut arms {
        if let Ok(spectator) = crowd.get(arm.parent) {
            let wave = (time.elapsed_seconds() * 1.7 - spectator.phase)
                .sin()
                .max(0.)
                .powi(6);
            t.translation.y = -0.15 + wave * 0.95;
            t.rotation = Quat::from_rotation_z(-arm.side * wave * 2.4);
        }
    }
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

struct Kit {
    cube: Handle<Mesh>,
    leaf: Handle<Mesh>,
    stone: Handle<Mesh>,
    grass: [Handle<StandardMaterial>; 2],
    greens: [Handle<StandardMaterial>; 3],
    rock: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    orange: Handle<StandardMaterial>,
    blue: Handle<StandardMaterial>,
    face: Handle<StandardMaterial>,
}
fn material(m: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    m.add(StandardMaterial {
        base_color: c,
        perceptual_roughness: 0.92,
        ..default()
    })
}
fn block(c: &mut Commands, k: &Kit, mat: Handle<StandardMaterial>, p: Vec3, size: Vec3) -> Entity {
    c.spawn(PbrBundle {
        mesh: k.cube.clone(),
        material: mat,
        transform: Transform::from_translation(p).with_scale(size),
        ..default()
    })
    .id()
}
fn beam(c: &mut Commands, k: &Kit, mat: Handle<StandardMaterial>, a: Vec3, b: Vec3, width: f32) {
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
fn palm(c: &mut Commands, k: &Kit, x: f32, z: f32, h: f32, phase: f32) {
    palm_at(c, k, x, z, h, phase, 0.);
}
fn palm_at(c: &mut Commands, k: &Kit, x: f32, z: f32, h: f32, phase: f32, ground: f32) {
    beam(
        c,
        k,
        k.wood.clone(),
        Vec3::new(x, ground, z),
        Vec3::new(x + 0.45, ground + h, z),
        0.38,
    );
    for j in 0..7 {
        let a = j as f32 * std::f32::consts::TAU / 7. + phase;
        let rot = Quat::from_rotation_y(a) * Quat::from_rotation_z(-0.2);
        c.spawn((
            PbrBundle {
                mesh: k.leaf.clone(),
                material: k.greens[j % 3].clone(),
                transform: Transform::from_xyz(x + 0.45, ground + h, z)
                    .with_rotation(rot)
                    .with_scale(Vec3::new(3.0, 0.23, 1.0)),
                ..default()
            },
            Sway {
                base: rot,
                phase: a,
            },
        ));
    }
}
fn monkey(c: &mut Commands, k: &Kit, parent: Entity, team: Team) {
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
        if p.y == -0.15 {
            c.entity(e).insert(CrowdArm {
                parent,
                side: p.x.signum(),
            });
        }
        c.entity(parent).add_child(e);
    }
    // An angular curling tail, readable even at the match camera distance.
    for i in 0..9 {
        let a = i as f32 * 0.48;
        let p = Vec3::new(
            0.45 + a.cos() * 0.42,
            0.2 + a.sin() * 0.42,
            -0.6 - i as f32 * 0.025,
        );
        let e = block(c, k, fur.clone(), p, Vec3::splat(0.19));
        c.entity(parent).add_child(e);
    }
}

pub fn build_jungle(
    mut c: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut old: Query<&mut Handle<Mesh>, (Without<DigitSegment>, Without<Ball>)>,
    players: Query<(Entity, &CubePlayer)>,
    balls: Query<Entity, With<Ball>>,
    mut digits: Query<&mut Transform, With<DigitSegment>>,
) {
    // Remove only old draw meshes; preserve physics entities and scoreboard segments.
    for mut mesh in &mut old {
        *mesh = Handle::default();
    }
    let scoreboard_z = -FIELD_DEPTH / 2. - 3.0;
    // Keep the functional digits on the camera-facing side of the decorative panel.
    let scoreboard_target = Vec3::new(0., 4.1, scoreboard_z + 0.35);
    let scoreboard_source = Vec3::new(0., 5.25, -ARENA_DEPTH / 2. + WALL_THICKNESS + 0.25);
    for mut t in &mut digits {
        t.translation = scoreboard_target + (t.translation - scoreboard_source) * 0.72;
        t.scale *= 0.72;
    }
    let mut rock_mesh = Sphere::new(1.).mesh().ico(1).unwrap();
    rock_mesh.duplicate_vertices();
    rock_mesh.compute_flat_normals();
    let k = Kit {
        cube: meshes.add(Cuboid::new(1., 1., 1.)),
        leaf: meshes.add(Sphere::new(1.).mesh().ico(0).unwrap()),
        stone: meshes.add(rock_mesh),
        grass: [
            material(&mut mats, Color::rgb(0.39, 0.65, 0.15)),
            material(&mut mats, Color::rgb(0.47, 0.72, 0.20)),
        ],
        greens: [
            material(&mut mats, Color::rgb(0.12, 0.35, 0.19)),
            material(&mut mats, Color::rgb(0.25, 0.52, 0.16)),
            material(&mut mats, Color::rgb(0.53, 0.72, 0.19)),
        ],
        rock: material(&mut mats, Color::rgb(0.36, 0.43, 0.36)),
        wood: material(&mut mats, Color::rgb(0.37, 0.20, 0.09)),
        gold: material(&mut mats, Color::rgb(0.76, 0.48, 0.17)),
        white: material(&mut mats, Color::rgb(0.98, 0.94, 0.74)),
        dark: material(&mut mats, Color::rgb(0.035, 0.07, 0.065)),
        water: material(&mut mats, Color::rgb(0.16, 0.67, 0.84)),
        orange: material(&mut mats, Color::rgb(0.87, 0.36, 0.06)),
        blue: material(&mut mats, Color::rgb(0.08, 0.34, 0.85)),
        face: material(&mut mats, Color::rgb(0.98, 0.77, 0.42)),
    };
    c.insert_resource(ClearColor(Color::rgb(0.53, 0.75, 0.78)));
    block(
        &mut c,
        &k,
        k.rock.clone(),
        Vec3::new(0., 0.1, 0.),
        Vec3::new(ARENA_WIDTH, 1.3, ARENA_DEPTH),
    );
    block(
        &mut c,
        &k,
        k.greens[0].clone(),
        Vec3::new(0., 0.9, 0.),
        Vec3::new(ARENA_WIDTH, 0.25, ARENA_DEPTH),
    );
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
    // Timber housing surrounds the existing functional seven-segment scoreboard.
    block(
        &mut c,
        &k,
        k.dark.clone(),
        Vec3::new(0., 4.1, scoreboard_z),
        Vec3::new(8.28, 3.46, 0.22),
    );
    for x in [-4.4, 4.4] {
        block(
            &mut c,
            &k,
            k.wood.clone(),
            Vec3::new(x, 3.1, scoreboard_z - 0.1),
            Vec3::new(0.5, 5.8, 0.65),
        );
    }
    for h in [2.2, 6.0] {
        block(
            &mut c,
            &k,
            k.gold.clone(),
            Vec3::new(0., h, scoreboard_z - 0.1),
            Vec3::new(9.4, 0.35, 0.7),
        );
    }
    for i in 0..3 {
        block(
            &mut c,
            &k,
            k.rock.clone(),
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
        let x = -4.2 + i as f32 * 0.55;
        let h = 6.1 - (i as f32 * 1.7).sin().abs() * 0.35;
        c.spawn(PbrBundle {
            mesh: k.leaf.clone(),
            material: k.greens[i % 3].clone(),
            transform: Transform::from_xyz(x, h, scoreboard_z + 0.4)
                .with_rotation(Quat::from_rotation_z(i as f32))
                .with_scale(Vec3::new(0.5, 0.15, 0.38)),
            ..default()
        });
    }
    for (e, p) in &players {
        monkey(&mut c, &k, e, p.team);
    }
    for ball in &balls {
        for direction in [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ] {
            let patch = c
                .spawn(PbrBundle {
                    mesh: k.stone.clone(),
                    material: k.dark.clone(),
                    transform: Transform::from_translation(direction * (BALL_RADIUS - 0.015))
                        .with_rotation(Quat::from_rotation_arc(Vec3::Y, direction))
                        .with_scale(Vec3::new(0.18, 0.035, 0.18)),
                    ..default()
                })
                .id();
            c.entity(ball).add_child(patch);
        }
    }
    // Low boundary walls make the retained field colliders visible.
    let extended_z = FIELD_DEPTH / 2. + SIDE_EXTENSION;
    for z in [-extended_z, extended_z] {
        block(
            &mut c,
            &k,
            k.rock.clone(),
            Vec3::new(0., 1.5, z),
            Vec3::new(FIELD_WIDTH, 1., 0.12),
        );
        for i in 0..9 {
            let x = -FIELD_WIDTH / 2. + i as f32 * FIELD_WIDTH / 8.;
            block(
                &mut c,
                &k,
                k.gold.clone(),
                Vec3::new(x, 1.9, z),
                Vec3::new(0.28, 1.5, 0.28),
            );
        }
    }
    for side in [-1., 1.] {
        let side_gap = (FIELD_DEPTH - GOAL_DEPTH) / 2.;
        for z in [-1., 1.] {
            block(
                &mut c,
                &k,
                k.rock.clone(),
                Vec3::new(
                    side * FIELD_WIDTH / 2.,
                    1.5,
                    z * (FIELD_DEPTH / 2. + SIDE_EXTENSION / 2.),
                ),
                Vec3::new(0.12, 1., SIDE_EXTENSION),
            );
        }
        let gap_center_z = (FIELD_DEPTH / 2. + GOAL_DEPTH / 2.) / 2.;
        for z in [-gap_center_z, gap_center_z] {
            block(
                &mut c,
                &k,
                k.rock.clone(),
                Vec3::new(side * FIELD_WIDTH / 2., 1.5, z),
                Vec3::new(0.12, 1., side_gap),
            );
        }
        for j in 0..4 {
            let x = side * (FIELD_WIDTH / 2. - 10. + j as f32 * 1.3);
            block(
                &mut c,
                &k,
                k.wood.clone(),
                Vec3::new(x, 1.7, -FIELD_DEPTH / 2. - 1.),
                Vec3::new(1.2, 0.25, 1.),
            );
            let e = c
                .spawn(SpatialBundle {
                    transform: Transform::from_xyz(x, 2.4, -FIELD_DEPTH / 2. - 1.)
                        .with_scale(Vec3::splat(0.55)),
                    ..default()
                })
                .id();
            monkey(
                &mut c,
                &k,
                e,
                if side < 0. { Team::Orange } else { Team::Blue },
            );
        }
    }
    // Broad, stepped spectator terraces outside the retained physical boundary.
    // Leave a central opening for the scoreboard and keep the near touchline clear.
    for side in [-1., 1.] {
        for row in 0..3 {
            let z = -FIELD_DEPTH / 2. - 2.2 - row as f32 * 1.25;
            let y = 1.3 + row as f32 * 0.65;
            block(
                &mut c,
                &k,
                k.wood.clone(),
                Vec3::new(side * 11., y, z),
                Vec3::new(12., 0.35, 1.15),
            );
            for seat in 0..8 {
                let x = side * (6. + seat as f32 * 1.35);
                let spectator = c
                    .spawn((
                        SpatialBundle {
                            transform: Transform::from_xyz(x, y + 0.6, z)
                                .with_scale(Vec3::splat(0.45)),
                            ..default()
                        },
                        CrowdWave {
                            base_y: y + 0.6,
                            phase: x * 0.22 + row as f32 * 0.32,
                        },
                    ))
                    .id();
                monkey(
                    &mut c,
                    &k,
                    spectator,
                    if side < 0. { Team::Orange } else { Team::Blue },
                );
            }
        }
        for x in [6., 11., 16.] {
            let x = side * x;
            beam(
                &mut c,
                &k,
                k.gold.clone(),
                Vec3::new(x, 0., FIELD_DEPTH / -2. - 5.),
                Vec3::new(x, 4.8, FIELD_DEPTH / -2. - 5.),
                0.18,
            );
            block(
                &mut c,
                &k,
                if side < 0. {
                    k.orange.clone()
                } else {
                    k.blue.clone()
                },
                Vec3::new(x, 4., FIELD_DEPTH / -2. - 4.9),
                Vec3::new(1.3, 1.4, 0.08),
            );
        }
    }
    // Deterministic perimeter placement keeps the open field clear.
    for i in 0..76 {
        let t = i as f32 * 2.39996;
        let (x, z) = if i < 40 {
            (-37. + i as f32 * 1.9, -27. - (i % 4) as f32 * 2.)
        } else {
            (
                if i % 2 == 0 {
                    -(FIELD_WIDTH / 2. + 4. + (i % 5) as f32 * 0.8)
                } else {
                    FIELD_WIDTH / 2. + 4. + (i % 5) as f32 * 0.8
                },
                -13. + (i - 40) as f32 * 0.8,
            )
        };
        let h = 2. + (i % 5) as f32 * 0.75;
        c.spawn(PbrBundle {
            mesh: k.stone.clone(),
            material: k.rock.clone(),
            transform: Transform::from_xyz(x, h * 0.35, z)
                .with_scale(Vec3::new(2.2, h, 2.))
                .with_rotation(Quat::from_rotation_y(t)),
            ..default()
        });
        if i % 3 == 0 {
            palm(&mut c, &k, x, z, h + 3.5, t);
        }
        for j in 0..3 {
            c.spawn(PbrBundle {
                mesh: k.leaf.clone(),
                material: k.greens[(i + j) % 3].clone(),
                transform: Transform::from_xyz(x + (j as f32 - 1.) * 0.8, h + 0.3, z)
                    .with_scale(Vec3::new(2., 0.9, 1.5))
                    .with_rotation(Quat::from_rotation_y(t + j as f32)),
                ..default()
            });
        }
    }
    // The pitch sits on a rock mesa above a flowing river gorge.
    let river = mats.add(StandardMaterial {
        base_color: Color::rgba(0.16, 0.67, 0.79, 0.82),
        perceptual_roughness: 0.18,
        metallic: 0.18,
        reflectance: 0.65,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    block(
        &mut c,
        &k,
        k.greens[0].clone(),
        Vec3::new(0., -9., 0.),
        Vec3::new(135., 2., 105.),
    );
    block(
        &mut c,
        &k,
        river.clone(),
        Vec3::new(0., -6.8, 0.),
        Vec3::new(94., 0.16, 78.),
    );
    for layer in 0..4 {
        let y = -1.5 - layer as f32 * 1.65;
        for i in 0..40 {
            let angle = i as f32 * std::f32::consts::TAU / 40.;
            let a = angle.cos();
            let b = angle.sin();
            let p = Vec3::new(
                a.signum() * a.abs().sqrt() * (ARENA_WIDTH / 2. - 1.),
                y,
                b.signum() * b.abs().sqrt() * (ARENA_DEPTH / 2. - 1.),
            );
            c.spawn(PbrBundle {
                mesh: k.stone.clone(),
                material: k.rock.clone(),
                transform: Transform::from_translation(p).with_scale(Vec3::new(5., 2.2, 4.)),
                ..default()
            });
        }
    }
    for side in [-1., 1.] {
        let fall_x = side * 26.;
        let fall_z = -29.;
        // Asymmetric faceted cliffs support the water source and divide the backdrop.
        for j in 0..8 {
            c.spawn(PbrBundle {
                mesh: k.stone.clone(),
                material: k.rock.clone(),
                transform: Transform::from_xyz(
                    side * (18. + j as f32 * 3.8),
                    2. + (j % 3) as f32 * 2.,
                    -34.,
                )
                .with_scale(Vec3::new(5., 7. + (j % 3) as f32 * 1.8, 5.)),
                ..default()
            });
            palm(
                &mut c,
                &k,
                side * (20. + j as f32 * 3.8),
                -37.,
                10. + (j % 3) as f32,
                j as f32,
            );
        }
        block(
            &mut c,
            &k,
            river.clone(),
            Vec3::new(fall_x, 10.6, -32.),
            Vec3::new(4.8, 0.15, 7.),
        );
        // Thin translucent water ribbons, falling highlights, and a broad landing pool.
        for ribbon in 0..12 {
            let x = fall_x - 2.2 + ribbon as f32 * 0.4;
            let e = block(
                &mut c,
                &k,
                river.clone(),
                Vec3::new(x, 2., fall_z),
                Vec3::new(0.43, 17.4, 0.18),
            );
            c.entity(e).insert(WaterRipple {
                origin: Vec3::new(x, 2., fall_z),
                phase: ribbon as f32,
                vertical: true,
            });
            for streak in 0..3 {
                let e = block(
                    &mut c,
                    &k,
                    k.white.clone(),
                    Vec3::new(x, -6. + streak as f32 * 5.4, fall_z + 0.16),
                    Vec3::new(0.055, 1.4 + ribbon as f32 * 0.06, 0.025),
                );
                c.entity(e).insert(Waterfall {
                    top: 10.5,
                    bottom: -6.5,
                    speed: 7. + ribbon as f32 * 0.18,
                });
            }
        }
        for i in 0..24 {
            let a = i as f32 * 2.399;
            let p = Vec3::new(
                fall_x + a.cos() * (1. + (i % 4) as f32 * 0.6),
                -6.45,
                fall_z + 1. + a.sin() * 1.7,
            );
            let e = c
                .spawn(PbrBundle {
                    mesh: k.leaf.clone(),
                    material: k.white.clone(),
                    transform: Transform::from_translation(p).with_scale(Vec3::new(0.65, 0.1, 0.4)),
                    ..default()
                })
                .id();
            c.entity(e).insert(WaterRipple {
                origin: p,
                phase: a,
                vertical: false,
            });
        }
        // River-bank islands and foreground vegetation establish a lower landscape.
        for i in 0..18 {
            let z = -35. + i as f32 * 4.5;
            let x = side * (39. + (i % 3) as f32 * 2.);
            c.spawn(PbrBundle {
                mesh: k.stone.clone(),
                material: k.greens[i % 3].clone(),
                transform: Transform::from_xyz(x, -6.4, z).with_scale(Vec3::new(6., 2., 4.5)),
                ..default()
            });
            if i % 2 == 0 {
                palm_at(&mut c, &k, x, z, 3. + (i % 4) as f32, i as f32, -4.8);
            }
        }
        for i in 0..36 {
            let p = Vec3::new(
                side * (32. + (i % 4) as f32 * 1.8),
                -6.55,
                -31. + i as f32 * 1.8,
            );
            let e = block(&mut c, &k, k.water.clone(), p, Vec3::new(0.12, 0.03, 1.5));
            c.entity(e).insert(WaterRipple {
                origin: p,
                phase: i as f32 * 0.7,
                vertical: false,
            });
        }
        // Sagging bridge from the stands across the gorge to a viewing platform.
        for j in 0..24 {
            let x = side * (18. + j as f32 * 0.65);
            let h = 2.8 - (j as f32 / 23. * std::f32::consts::PI).sin() * 1.4;
            block(
                &mut c,
                &k,
                k.gold.clone(),
                Vec3::new(x, h, -25.),
                Vec3::new(0.6, 0.18, 2.2),
            );
            if j % 3 == 0 {
                for z in [-26., -24.] {
                    beam(
                        &mut c,
                        &k,
                        k.wood.clone(),
                        Vec3::new(x, h, z),
                        Vec3::new(x, h + 1.3, z),
                        0.12,
                    );
                    if j < 21 {
                        beam(
                            &mut c,
                            &k,
                            k.gold.clone(),
                            Vec3::new(x, h + 1.3, z),
                            Vec3::new(x + side * 1.95, h + 1.15, z),
                            0.065,
                        );
                    }
                }
            }
        }
    }
}

pub fn animate_jungle(
    time: Res<Time>,
    mut leaves: Query<(&mut Transform, &Sway), Without<CrowdWave>>,
    mut water: Query<(&mut Transform, &Waterfall), (Without<Sway>, Without<CrowdWave>)>,
    mut crowd: Query<(&mut Transform, &CrowdWave), (Without<Sway>, Without<Waterfall>)>,
) {
    for (mut t, spectator) in &mut crowd {
        let wave = (time.elapsed_seconds() * 1.7 - spectator.phase)
            .sin()
            .max(0.)
            .powi(6);
        t.translation.y = spectator.base_y + wave * 0.65;
        t.rotation =
            Quat::from_rotation_z((time.elapsed_seconds() * 2.3 + spectator.phase).sin() * 0.07);
    }
    for (mut t, s) in &mut leaves {
        t.rotation =
            s.base * Quat::from_rotation_z((time.elapsed_seconds() * 0.8 + s.phase).sin() * 0.045);
    }
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
}
