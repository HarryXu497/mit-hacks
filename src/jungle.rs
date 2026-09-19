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
    beam(
        c,
        k,
        k.wood.clone(),
        Vec3::new(x, 0., z),
        Vec3::new(x + 0.45, h, z),
        0.38,
    );
    for j in 0..7 {
        let a = j as f32 * std::f32::consts::TAU / 7. + phase;
        let rot = Quat::from_rotation_y(a) * Quat::from_rotation_z(-0.2);
        c.spawn((
            PbrBundle {
                mesh: k.leaf.clone(),
                material: k.greens[j % 3].clone(),
                transform: Transform::from_xyz(x + 0.45, h, z)
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
    for mut t in &mut digits {
        t.translation =
            Vec3::new(0., 4.1, -10.7) + (t.translation - Vec3::new(0., 5.25, -14.45)) * 0.72;
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
        Vec3::new(0., -1., 0.),
        Vec3::new(32., 3., 32.),
    );
    block(
        &mut c,
        &k,
        k.greens[0].clone(),
        Vec3::new(0., 0.9, 0.),
        Vec3::new(30., 0.25, 30.),
    );
    for i in 0..12 {
        block(
            &mut c,
            &k,
            k.grass[i % 2].clone(),
            Vec3::new(-11. + i as f32 * 2., 1.12, 0.),
            Vec3::new(2., 0.04, 16.),
        );
    }
    let y = 1.16;
    for x in [-11.7, 0., 11.7] {
        block(
            &mut c,
            &k,
            k.white.clone(),
            Vec3::new(x, y, 0.),
            Vec3::new(0.09, 0.025, 15.4),
        );
    }
    for z in [-7.7, 7.7] {
        block(
            &mut c,
            &k,
            k.white.clone(),
            Vec3::new(0., y, z),
            Vec3::new(23.5, 0.025, 0.09),
        );
    }
    for i in 0..64 {
        let a = i as f32 * std::f32::consts::TAU / 64.;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 64.;
        beam(
            &mut c,
            &k,
            k.white.clone(),
            Vec3::new(a.cos() * 3., y, a.sin() * 3.),
            Vec3::new(b.cos() * 3., y, b.sin() * 3.),
            0.075,
        );
    }
    for side in [-1., 1.] {
        for z in [-5.5, 5.5] {
            block(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(side * 9.8, y, z),
                Vec3::new(3.8, 0.025, 0.09),
            );
        }
        block(
            &mut c,
            &k,
            k.white.clone(),
            Vec3::new(side * 7.9, y, 0.),
            Vec3::new(0.09, 0.025, 11.),
        );
        for z in [-3., 3.] {
            block(
                &mut c,
                &k,
                k.gold.clone(),
                Vec3::new(side * 12., 3., z),
                Vec3::new(0.30, 4., 0.30),
            );
            beam(
                &mut c,
                &k,
                k.wood.clone(),
                Vec3::new(side * 12., 5., z),
                Vec3::new(side * 14.5, 1.1, z),
                0.15,
            );
        }
        block(
            &mut c,
            &k,
            k.gold.clone(),
            Vec3::new(side * 12., 5., 0.),
            Vec3::new(0.3, 0.3, 6.2),
        );
        for j in 0..13 {
            let z = -3. + j as f32 * 0.5;
            beam(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(side * 14.45, 1.1, z),
                Vec3::new(side * 14.45, 5., z),
                0.035,
            );
            beam(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(side * 12., 5., z),
                Vec3::new(side * 14.45, 5., z),
                0.035,
            );
        }
        for j in 0..9 {
            let h = 1.1 + j as f32 * 0.48;
            beam(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(side * 14.45, h, -3.),
                Vec3::new(side * 14.45, h, 3.),
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
            Vec3::new(side * 14., 4., -7.),
            Vec3::new(0.22, 6., 0.22),
        );
        block(
            &mut c,
            &k,
            team,
            Vec3::new(side * 13.2, 5., -7.),
            Vec3::new(1.5, 2.5, 0.1),
        );
    }
    // Timber housing surrounds the existing functional seven-segment scoreboard.
    block(
        &mut c,
        &k,
        k.dark.clone(),
        Vec3::new(0., 4.1, -10.7),
        Vec3::new(8.28, 3.46, 0.22),
    );
    for x in [-4.4, 4.4] {
        block(
            &mut c,
            &k,
            k.wood.clone(),
            Vec3::new(x, 3.1, -10.8),
            Vec3::new(0.5, 5.8, 0.65),
        );
    }
    for h in [2.2, 6.0] {
        block(
            &mut c,
            &k,
            k.gold.clone(),
            Vec3::new(0., h, -10.8),
            Vec3::new(9.4, 0.35, 0.7),
        );
    }
    for i in 0..3 {
        block(
            &mut c,
            &k,
            k.rock.clone(),
            Vec3::new(0., 6.3 + i as f32 * 0.5, -12.),
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
            Vec3::new(x, 2.1, -13.2),
            Vec3::new(0.8, 2.2, 0.8),
        );
        block(
            &mut c,
            &k,
            k.gold.clone(),
            Vec3::new(x, 3.25, -13.2),
            Vec3::new(1., 0.3, 1.),
        );
        c.spawn(PbrBundle {
            mesh: k.leaf.clone(),
            material: flame.clone(),
            transform: Transform::from_xyz(x, 3.8, -13.2).with_scale(Vec3::new(0.35, 0.7, 0.35)),
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
            transform: Transform::from_xyz(x, h, -10.3)
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
    for z in [-13., 13.] {
        block(
            &mut c,
            &k,
            k.rock.clone(),
            Vec3::new(0., 1.5, z),
            Vec3::new(24., 1., 0.12),
        );
        for x in [-11., -7., -3., 3., 7., 11.] {
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
        for z in [-8., 8.] {
            block(
                &mut c,
                &k,
                k.rock.clone(),
                Vec3::new(side * 12., 1.5, z),
                Vec3::new(0.12, 1., 10.),
            );
        }
        for j in 0..4 {
            let x = side * (8. + j as f32 * 1.3);
            block(
                &mut c,
                &k,
                k.wood.clone(),
                Vec3::new(x, 1.7, -11.5),
                Vec3::new(1.2, 0.25, 1.),
            );
            let e = c
                .spawn(SpatialBundle {
                    transform: Transform::from_xyz(x, 2.4, -11.5).with_scale(Vec3::splat(0.55)),
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
    // Deterministic perimeter placement keeps the open field clear.
    for i in 0..76 {
        let t = i as f32 * 2.39996;
        let (x, z) = if i < 40 {
            (-28. + i as f32 * 1.4, -18. - (i % 4) as f32 * 2.)
        } else {
            (
                if i % 2 == 0 {
                    -17. - (i % 5) as f32
                } else {
                    17. + (i % 5) as f32
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
    for side in [-1., 1.] {
        // Terrace and whitewater tie the falls into the cliff rather than a floating sheet.
        block(
            &mut c,
            &k,
            k.rock.clone(),
            Vec3::new(side * 15.6, 11.3, -16.3),
            Vec3::new(3.7, 0.7, 2.),
        );
        block(
            &mut c,
            &k,
            k.water.clone(),
            Vec3::new(side * 15.6, 1.05, -14.8),
            Vec3::new(4., 0.12, 3.),
        );
        for i in 0..7 {
            let x = side * (14.4 + i as f32 * 0.4);
            c.spawn(PbrBundle {
                mesh: k.stone.clone(),
                material: k.white.clone(),
                transform: Transform::from_xyz(x, 1.2, -15.8).with_scale(Vec3::new(0.4, 0.18, 0.5)),
                ..default()
            });
        }
        block(
            &mut c,
            &k,
            k.water.clone(),
            Vec3::new(side * 21., 0.4, -15.),
            Vec3::new(7., 0.15, 32.),
        );
        for j in 0..5 {
            block(
                &mut c,
                &k,
                k.rock.clone(),
                Vec3::new(side * (17. + j as f32 * 2.), 5. + (j % 2) as f32, -24.),
                Vec3::new(3., 12., 5.),
            );
        }
        for j in 0..5 {
            block(
                &mut c,
                &k,
                k.water.clone(),
                Vec3::new(side * (14.5 + j as f32 * 0.55), 6., -16.0),
                Vec3::new(0.55, 10.8, 0.12),
            );
            let e = block(
                &mut c,
                &k,
                k.white.clone(),
                Vec3::new(side * (14.5 + j as f32 * 0.55), 2. + j as f32 * 1.7, -15.9),
                Vec3::new(0.12, 1.4, 0.03),
            );
            c.entity(e).insert(Waterfall {
                top: 11.,
                bottom: 1.5,
                speed: 3. + j as f32 * 0.25,
            });
        }
        // Rope bridge and a small open viewing hut.
        for j in 0..16 {
            let x = side * (10.5 + j as f32 * 0.45);
            let h = 4.3 - ((j as f32 / 15.) * std::f32::consts::PI).sin() * 0.7;
            block(
                &mut c,
                &k,
                k.gold.clone(),
                Vec3::new(x, h, -17.5),
                Vec3::new(0.48, 0.15, 1.6),
            );
            if j % 3 == 0 {
                for z in [-18.2, -16.8] {
                    block(
                        &mut c,
                        &k,
                        k.wood.clone(),
                        Vec3::new(x, h + 0.6, z),
                        Vec3::new(0.1, 1.3, 0.1),
                    );
                }
            }
        }
        for z in [-18.2, -16.8] {
            beam(
                &mut c,
                &k,
                k.gold.clone(),
                Vec3::new(side * 10.5, 5., z),
                Vec3::new(side * 17.25, 5., z),
                0.09,
            );
        }
        let x = side * 17.25;
        block(
            &mut c,
            &k,
            k.wood.clone(),
            Vec3::new(x, 4., -17.5),
            Vec3::new(4., 0.3, 4.),
        );
        for dx in [-1.6, 1.6] {
            for dz in [-1.6, 1.6] {
                block(
                    &mut c,
                    &k,
                    k.wood.clone(),
                    Vec3::new(x + dx, 4., -17.5 + dz),
                    Vec3::new(0.2, 8., 0.2),
                );
            }
        }
        for j in 0..4 {
            block(
                &mut c,
                &k,
                k.gold.clone(),
                Vec3::new(x, 7.7 + j as f32 * 0.35, -17.5),
                Vec3::new(4.6 - j as f32, 0.4, 4.6 - j as f32),
            );
        }
    }
}

pub fn animate_jungle(
    time: Res<Time>,
    mut leaves: Query<(&mut Transform, &Sway)>,
    mut water: Query<(&mut Transform, &Waterfall), Without<Sway>>,
) {
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
