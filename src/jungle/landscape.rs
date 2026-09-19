//! Summit stadium and the rainforest archipelago below it. No simulation entities.
use super::*;
use bevy::render::{
    mesh::Indices, render_asset::RenderAssetUsages, render_resource::PrimitiveTopology,
};

const SUMMIT_X: f32 = 35.;
const SUMMIT_Z: f32 = 24.;
const VALLEY_Y: f32 = -42.;

/// A tapered, folded blade follows an arch and droops at the tip. The two
/// surfaces keep the silhouette readable from the broadcast view and below.
pub(super) fn palm_frond_mesh() -> Mesh {
    let mut p = Vec::new();
    let mut colors = Vec::new();
    let mut ids = Vec::new();
    for i in 0..=12 {
        let t = i as f32 / 12.;
        let width = (t * std::f32::consts::PI).sin().powf(0.7) * 0.58;
        let y = (t * std::f32::consts::PI).sin() * 0.7 - t * t * 1.15;
        for edge in [-1.0_f32, 0.0, 1.0] {
            p.push([t * 3.8, y - edge.abs() * width * 0.30, edge * width]);
            let light = if edge == 0. {
                1.08
            } else if edge < 0. {
                0.86
            } else {
                1.
            };
            colors.push([light, light, light, 1.]);
        }
    }
    for i in 0..12u32 {
        for side in 0..2u32 {
            let a = i * 3 + side;
            let b = a + 3;
            ids.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b, a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    mesh_from(p, colors, ids)
}

fn random(i: usize) -> f32 {
    let mut n = (i as u32).wrapping_mul(747796405).wrapping_add(2891336453);
    n = ((n >> ((n >> 28) + 4)) ^ n).wrapping_mul(277803737);
    ((n >> 22) ^ n) as f32 / u32::MAX as f32
}
fn noise(x: f32, z: f32) -> f32 {
    (x * 0.075 + z * 0.043).sin() * 0.55
        + (z * 0.112 - x * 0.057).cos() * 0.30
        + (x * 0.19 + z * 0.15).sin() * 0.15
}
fn rim(a: f32, rx: f32, rz: f32) -> Vec3 {
    Vec3::new(
        a.cos().signum() * a.cos().abs().sqrt() * rx,
        0.,
        a.sin().signum() * a.sin().abs().sqrt() * rz,
    )
}
fn mesh_from(positions: Vec<[f32; 3]>, colors: Vec<[f32; 4]>, indices: Vec<u32>) -> Mesh {
    let uvs: Vec<_> = positions.iter().map(|p| [p[0] * 0.1, p[2] * 0.1]).collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    mesh
}

struct RawMesh {
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

fn raw_mesh() -> RawMesh {
    RawMesh {
        positions: Vec::new(),
        indices: Vec::new(),
    }
}

fn push_box(mesh: &mut RawMesh, center: Vec3, size: Vec3) {
    let base = mesh.positions.len() as u32;
    let h = size * 0.5;
    for p in [
        Vec3::new(-h.x, -h.y, -h.z),
        Vec3::new(h.x, -h.y, -h.z),
        Vec3::new(h.x, -h.y, h.z),
        Vec3::new(-h.x, -h.y, h.z),
        Vec3::new(-h.x, h.y, -h.z),
        Vec3::new(h.x, h.y, -h.z),
        Vec3::new(h.x, h.y, h.z),
        Vec3::new(-h.x, h.y, h.z),
    ] {
        mesh.positions.push((center + p).to_array());
    }
    mesh.indices.extend_from_slice(&[
        base,
        base + 1,
        base + 2,
        base,
        base + 2,
        base + 3,
        base + 4,
        base + 6,
        base + 5,
        base + 4,
        base + 7,
        base + 6,
        base,
        base + 4,
        base + 5,
        base,
        base + 5,
        base + 1,
        base + 1,
        base + 5,
        base + 6,
        base + 1,
        base + 6,
        base + 2,
        base + 2,
        base + 6,
        base + 7,
        base + 2,
        base + 7,
        base + 3,
        base + 4,
        base,
        base + 3,
        base + 4,
        base + 3,
        base + 7,
    ]);
}

fn push_canopy(mesh: &mut RawMesh, center: Vec3, size: Vec3) {
    let base = mesh.positions.len() as u32;
    let p = [
        center + Vec3::Y * size.y,
        center - Vec3::Y * size.y,
        center + Vec3::X * size.x,
        center - Vec3::X * size.x,
        center + Vec3::Z * size.z,
        center - Vec3::Z * size.z,
    ];
    for v in p {
        mesh.positions.push(v.to_array());
    }
    mesh.indices.extend_from_slice(&[
        base,
        base + 2,
        base + 4,
        base,
        base + 4,
        base + 3,
        base,
        base + 3,
        base + 5,
        base,
        base + 5,
        base + 2,
        base + 1,
        base + 4,
        base + 2,
        base + 1,
        base + 3,
        base + 4,
        base + 1,
        base + 5,
        base + 3,
        base + 1,
        base + 2,
        base + 5,
    ]);
}

fn finish_raw(raw: RawMesh) -> Mesh {
    let colors = vec![[1., 1., 1., 1.]; raw.positions.len()];
    mesh_from(raw.positions, colors, raw.indices)
}

fn batch_far_forest(
    c: &mut Commands,
    k: &Kit,
    meshes: &mut Assets<Mesh>,
    greens: &[Handle<StandardMaterial>],
    entries: &[(Vec3, f32, usize)],
) {
    let mut trunk = raw_mesh();
    let mut canopies: Vec<_> = (0..greens.len()).map(|_| raw_mesh()).collect();
    for (p, h, seed) in entries {
        let top = *p + Vec3::new(noise(p.x, p.z) * 0.7, *h, 0.);
        push_box(
            &mut trunk,
            *p + Vec3::Y * (*h * 0.5),
            Vec3::new(0.28 + *h * 0.02, *h, 0.28 + *h * 0.02),
        );
        for j in 0..2 {
            let a = *seed as f32 * 1.73 + j as f32 * 3.14;
            push_canopy(
                &mut canopies[(*seed + j) % greens.len()],
                top + Vec3::new(
                    a.cos() * *h * 0.16,
                    -(j as f32) * *h * 0.15,
                    a.sin() * *h * 0.16,
                ),
                Vec3::new(*h * 0.58, *h * 0.35, *h * 0.50),
            );
        }
    }
    c.spawn(PbrBundle {
        mesh: meshes.add(finish_raw(trunk)),
        material: k.wood.clone(),
        ..default()
    });
    for (raw, material) in canopies.into_iter().zip(greens.iter()) {
        c.spawn(PbrBundle {
            mesh: meshes.add(finish_raw(raw)),
            material: material.clone(),
            ..default()
        });
    }
}
/// One closed summit cap and a continuous fractured cliff, not a stack of ellipsoids.
fn massif(rx: f32, rz: f32, top: f32, bottom: f32, seed: usize, summit: bool) -> Mesh {
    let n = 64usize;
    let layers = 8usize;
    let mut p = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    p.push([0., top, 0.]);
    colors.push([0.28, 0.49, 0.10, 1.]);
    for layer in 0..=layers {
        let t = layer as f32 / layers as f32;
        for i in 0..=n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let fissure =
                (a * 7. + seed as f32).sin() * 0.65 + (a * 13. + seed as f32).cos() * 0.24;
            let spread = if layer == 0 {
                1.
            } else {
                // Keep the upper face behind carvings and cascades; widen only
                // below the inhabited ledges into the mountain's foot.
                1. - t * 0.13 + (t - 0.65).max(0.) * 0.55
            };
            let mut v = rim(a, rx * spread, rz * spread);
            v += Vec3::new(a.cos(), 0., a.sin()) * fissure * t.min(0.2) * 1.5;
            v.y = top + (bottom - top) * t;
            if layer != 0 && layer != layers {
                v.y += (random(seed + i * 3 + layer * 19) - 0.5) * 2.;
            }
            p.push(v.to_array());
            let shade = 0.5 + (a * 9. + layer as f32 * 0.8).sin() * 0.24;
            let col = if layer == 0 {
                [0.27 + shade * 0.07, 0.45 + shade * 0.1, 0.11, 1.]
            } else if shade > 0.82 && layer < 5 {
                [0.19, 0.33, 0.13, 1.]
            } else {
                [
                    0.30 + shade * 0.12,
                    0.32 + shade * 0.11,
                    0.27 + shade * 0.10,
                    1.,
                ]
            };
            colors.push(col);
        }
    }
    for i in 0..n {
        indices.extend_from_slice(&[0, (i + 2) as u32, (i + 1) as u32]);
    }
    for layer in 0..layers {
        for i in 0..n {
            let a = (1 + layer * (n + 1) + i) as u32;
            let b = a + (n + 1) as u32;
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    if !summit {
        for v in &mut p {
            v[0] += v[1] * 0.09;
        }
    }
    mesh_from(p, colors, indices)
}
fn valley_height(x: f32, z: f32) -> f32 {
    VALLEY_Y + noise(x, z) * 5.
}

fn mountain(radius: f32, peak: f32, seed: usize) -> Mesh {
    let mut p = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    let segments = 24usize;
    for layer in 0..=10 {
        let t = layer as f32 / 10.;
        let r = radius * (0.05 + t.powf(0.8) * 0.95);
        for i in 0..=segments {
            let a = i as f32 / segments as f32 * std::f32::consts::TAU;
            let rough = 0.87 + random(seed + i % segments) * 0.26;
            p.push([
                a.cos() * r * rough + (1. - t) * 3.,
                peak + (-44. - peak) * t,
                a.sin() * r * 0.72 * rough,
            ]);
            let moss = random(seed + i % segments * 7 + layer * 19) > 0.52;
            colors.push(if moss {
                [0.22, 0.40, 0.18, 1.]
            } else {
                [0.37, 0.43, 0.38, 1.]
            });
        }
    }
    for layer in 0..10 {
        for i in 0..segments {
            let a = (layer * (segments + 1) + i) as u32;
            let b = a + (segments + 1) as u32;
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    mesh_from(p, colors, indices)
}
fn valley_mesh() -> Mesh {
    let n = 100;
    let mut p = Vec::new();
    let mut c = Vec::new();
    let mut ids = Vec::new();
    for row in 0..=n {
        for col in 0..=n {
            let x = -260. + col as f32 * 5.2;
            let z = -310. + row as f32 * 5.2;
            p.push([x, valley_height(x, z), z]);
            let v = noise(x * 2., z * 2.);
            c.push([0.14 + v * 0.04, 0.32 + v * 0.05, 0.14 + v * 0.03, 1.]);
        }
    }
    for row in 0..n {
        for col in 0..n {
            let a = (row * (n + 1) + col) as u32;
            let b = a + (n + 1) as u32;
            ids.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    mesh_from(p, c, ids)
}
fn sprig(c: &mut Commands, k: &Kit, p: Vec3, s: f32, seed: usize) {
    for j in 0..5 {
        let a = j as f32 * 1.256 + seed as f32;
        let rot = Quat::from_rotation_y(-a) * Quat::from_rotation_z(0.4);
        c.spawn(PbrBundle {
            mesh: k.leaf.clone(),
            material: k.greens[(seed + j) % 3].clone(),
            transform: Transform::from_translation(
                p + Vec3::new(a.cos() * s * 0.35, s * 0.3, a.sin() * s * 0.35),
            )
            .with_rotation(rot)
            .with_scale(Vec3::new(s, 0.14 * s, 0.32 * s)),
            ..default()
        });
    }
}

fn banana(c: &mut Commands, k: &Kit, p: Vec3, s: f32) {
    for i in 0..7 {
        let a = -1.15 + i as f32 * 0.32;
        let v = p + Vec3::new(a.sin() * s, (1. - a.cos()) * s, 0.);
        let e = block(
            c,
            k,
            k.flower_yellow.clone(),
            v,
            Vec3::new(s * 0.3, s * 0.23, s * 0.09),
        );
        c.entity(e).insert(
            Transform::from_translation(v)
                .with_scale(Vec3::new(s * 0.3, s * 0.23, s * 0.09))
                .with_rotation(Quat::from_rotation_z(a)),
        );
    }
}
fn banner(c: &mut Commands, k: &Kit, p: Vec3, team: Team, scale: f32) {
    let mat = if team == Team::Orange {
        k.orange.clone()
    } else {
        k.blue.clone()
    };
    block(c, k, mat, p, Vec3::new(2.4 * scale, 3.7 * scale, 0.08));
    beam(
        c,
        k,
        k.wood_light.clone(),
        p + Vec3::new(-1.5 * scale, 2. * scale, 0.),
        p + Vec3::new(1.5 * scale, 2. * scale, 0.),
        0.17 * scale,
    );
    banana(c, k, p + Vec3::new(0., -0.3 * scale, 0.1), 0.85 * scale);
}
fn torch(c: &mut Commands, k: &Kit, p: Vec3, flame: &Handle<StandardMaterial>) {
    block(
        c,
        k,
        k.rock_light.clone(),
        p + Vec3::Y * 0.45,
        Vec3::new(0.75, 0.9, 0.75),
    );
    block(
        c,
        k,
        k.wood.clone(),
        p + Vec3::Y * 1.3,
        Vec3::new(0.24, 1.1, 0.24),
    );
    oval(
        c,
        k,
        k.gold.clone(),
        p + Vec3::Y * 1.85,
        Vec3::new(0.45, 0.16, 0.45),
        0.,
    );
    c.spawn(PbrBundle {
        mesh: k.leaf.clone(),
        material: flame.clone(),
        transform: Transform::from_translation(p + Vec3::Y * 2.3)
            .with_scale(Vec3::new(0.23, 0.6, 0.23)),
        ..default()
    });
}
fn bridge(c: &mut Commands, k: &Kit, a: Vec3, b: Vec3, width: f32) {
    let d = b - a;
    let side = Vec3::new(-d.z, 0., d.x).normalize();
    let n = (d.length() / 0.55) as usize;
    let at = |t: f32| a.lerp(b, t) - Vec3::Y * (t * std::f32::consts::PI).sin() * 2.;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let p = at(t);
        beam(
            c,
            k,
            k.wood_light.clone(),
            p - side * width * 0.5,
            p + side * width * 0.5,
            0.3,
        );
        if i % 3 == 0 {
            for sign in [-1., 1.] {
                let q = p + side * width * 0.48 * sign;
                beam(c, k, k.wood.clone(), q, q + Vec3::Y * 1.5, 0.1);
                let next = at(((i + 3).min(n)) as f32 / n as f32)
                    + side * width * 0.48 * sign
                    + Vec3::Y * 1.5;
                beam(c, k, k.rope.clone(), q + Vec3::Y * 1.5, next, 0.065);
            }
        }
    }
}
fn monkey_relief(c: &mut Commands, k: &Kit, p: Vec3, s: f32) {
    oval(
        c,
        k,
        k.rock_light.clone(),
        p,
        Vec3::new(2.2, 2.25, 0.45) * s,
        0.,
    );
    oval(
        c,
        k,
        k.rock.clone(),
        p + Vec3::Z * 0.36 * s,
        Vec3::new(1.85, 1.9, 0.25) * s,
        0.,
    );
    for side in [-1., 1.] {
        oval(
            c,
            k,
            k.rock_light.clone(),
            p + Vec3::new(side * 1.85, 0.2, 0.16) * s,
            Vec3::new(0.63, 0.86, 0.3) * s,
            0.,
        );
        oval(
            c,
            k,
            k.face.clone(),
            p + Vec3::new(side * 0.64, 0.25, 0.61) * s,
            Vec3::new(0.78, 0.86, 0.13) * s,
            0.,
        );
        oval(
            c,
            k,
            k.dark.clone(),
            p + Vec3::new(side * 0.62, 0.39, 0.77) * s,
            Vec3::new(0.16, 0.23, 0.09) * s,
            0.,
        );
    }
    oval(
        c,
        k,
        k.face.clone(),
        p + Vec3::new(0., -0.72, 0.67) * s,
        Vec3::new(1.13, 0.72, 0.15) * s,
        0.,
    );
    oval(
        c,
        k,
        k.rock.clone(),
        p + Vec3::new(0., -0.44, 0.86) * s,
        Vec3::new(0.3, 0.17, 0.075) * s,
        0.,
    );
    for i in 0..6 {
        let a = i as f32 * 0.28 - 0.7;
        block(
            c,
            k,
            k.rock.clone(),
            p + Vec3::new(a.sin() * 0.64, -0.87 - a.cos() * 0.22, 0.87) * s,
            Vec3::new(0.2, 0.075, 0.06) * s,
        );
    }
}
fn pagoda(c: &mut Commands, k: &Kit, p: Vec3, s: f32) {
    block(c, k, k.wood.clone(), p, Vec3::new(4.5, 0.3, 4.5) * s);
    for x in [-1., 1.] {
        for z in [-1., 1.] {
            beam(
                c,
                k,
                k.wood_light.clone(),
                p + Vec3::new(x * 1.8, 0., z * 1.8) * s,
                p + Vec3::new(x * 1.8, 3.6, z * 1.8) * s,
                0.23 * s,
            );
        }
    }
    for layer in 0..4 {
        let size = 5.7 - layer as f32 * 1.25;
        block(
            c,
            k,
            k.gold.clone(),
            p + Vec3::Y * (3.6 + layer as f32 * 0.38) * s,
            Vec3::new(size, 0.36, size) * s,
        );
    }
    block(
        c,
        k,
        k.wood.clone(),
        p + Vec3::Y * 5.3 * s,
        Vec3::new(0.2, 0.9, 0.2) * s,
    );
    for side in [-1., 1.] {
        beam(
            c,
            k,
            k.wood_light.clone(),
            p + Vec3::new(-2., 1.1, side * 2.) * s,
            p + Vec3::new(2., 1.1, side * 2.) * s,
            0.12 * s,
        );
    }
}
fn waterfall(
    c: &mut Commands,
    k: &Kit,
    p: Vec3,
    bottom: f32,
    width: f32,
    water: &Handle<StandardMaterial>,
    seed: usize,
) {
    let height = p.y - bottom;
    for i in 0..4 {
        let x = p.x - width / 2. + (i as f32 + 0.5) * width / 4.;
        let q = Vec3::new(x, (p.y + bottom) / 2., p.z);
        block(
            c,
            k,
            water.clone(),
            q,
            Vec3::new(width / 4. + 0.04, height, 0.15),
        );
        for j in 0..2 {
            let e = block(
                c,
                k,
                k.water_light.clone(),
                Vec3::new(
                    x,
                    bottom + (j as f32 + random(seed + i)) * height / 2.,
                    p.z + 0.11,
                ),
                Vec3::new(width / 70., 1.0 + random(seed + j + i) * 2., 0.025),
            );
            c.entity(e).insert(Waterfall {
                top: p.y,
                bottom,
                speed: 6. + random(seed + i) * 3.,
            });
        }
    }
    oval(
        c,
        k,
        water.clone(),
        Vec3::new(p.x, bottom, p.z + 1.),
        Vec3::new(width * 0.9, 0.1, width),
        0.,
    );
    for i in 0..6 {
        let a = i as f32 * 2.399;
        let r = width * (0.2 + random(seed + i) * 0.5);
        let q = Vec3::new(p.x + a.cos() * r, bottom + 0.12, p.z + 1. + a.sin() * r);
        let e = oval(
            c,
            k,
            k.water_light.clone(),
            q,
            Vec3::new(0.5, 0.045, 0.24),
            a,
        );
        c.entity(e).insert(WaterRipple {
            origin: q,
            phase: a,
            vertical: false,
        });
    }
}
fn crowd_silhouette(raw: &mut RawMesh, p: Vec3, scale: f32, seed: usize) {
    let wobble = 0.90 + random(seed) * 0.20;
    let s = scale * wobble;
    push_box(
        raw,
        p + Vec3::Y * (0.55 * s),
        Vec3::new(0.78 * s, 0.95 * s, 0.55 * s),
    );
    push_box(
        raw,
        p + Vec3::Y * (1.18 * s),
        Vec3::new(0.92 * s, 0.78 * s, 0.72 * s),
    );
    push_box(
        raw,
        p + Vec3::new(-0.56 * s, 0.48 * s, 0.),
        Vec3::new(0.20 * s, 0.62 * s, 0.20 * s),
    );
    push_box(
        raw,
        p + Vec3::new(0.56 * s, 0.48 * s, 0.),
        Vec3::new(0.20 * s, 0.62 * s, 0.20 * s),
    );
}

pub(super) fn build(
    c: &mut Commands,
    k: &Kit,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
) {
    let white = material(mats, Color::WHITE);
    c.insert_resource(ClearColor(Color::rgb(0.50, 0.77, 0.89)));
    c.spawn(PbrBundle {
        mesh: meshes.add(massif(SUMMIT_X, SUMMIT_Z, 1.04, -41., 10, true)),
        material: white.clone(),
        ..default()
    });
    c.spawn(PbrBundle {
        mesh: meshes.add(valley_mesh()),
        material: white.clone(),
        ..default()
    });
    let water = mats.add(StandardMaterial {
        base_color: Color::rgb(0.06, 0.50, 0.64),
        perceptual_roughness: 0.18,
        reflectance: 0.65,
        ..default()
    });
    let flame = mats.add(StandardMaterial {
        base_color: Color::rgb(1., 0.65, 0.12),
        emissive: Color::rgb(1., 0.36, 0.025) * 3.,
        ..default()
    });
    let greens: Vec<_> = [
        (0.09, 0.30, 0.18),
        (0.20, 0.43, 0.13),
        (0.39, 0.58, 0.13),
        (0.17, 0.41, 0.26),
        (0.43, 0.57, 0.19),
        (0.10, 0.33, 0.23),
    ]
    .into_iter()
    .map(|(r, g, b)| material(mats, Color::rgb(r, g, b)))
    .collect();
    let flowers: Vec<_> = [(1., 0.24, 0.12), (0.95, 0.62, 0.11), (0.90, 0.31, 0.43)]
        .into_iter()
        .map(|(r, g, b)| material(mats, Color::rgb(r, g, b)))
        .collect();
    let mut orange_crowd = raw_mesh();
    let mut blue_crowd = raw_mesh();
    // The surrounding water is far BELOW the playable summit.
    block(
        c,
        k,
        water.clone(),
        Vec3::new(0., -45., -30.),
        Vec3::new(510., 0.1, 520.),
    );
    // Open rear skyline; audience sectors run along the two goal ends.
    for (side_index, side) in [-1., 1.].into_iter().enumerate() {
        for row in 0..5 {
            let x = side * (28.2 + row as f32 * 0.85);
            let y = 1.4 + row as f32 * 0.56;
            for section in 0..2 {
                let z = -7.3 + section as f32 * 14.6;
                block(
                    c,
                    k,
                    k.wood.clone(),
                    Vec3::new(x, y - 0.18, z),
                    Vec3::new(0.9, 0.35, 13.2),
                );
                block(
                    c,
                    k,
                    k.wood_light.clone(),
                    Vec3::new(x, y + 0.10, z),
                    Vec3::new(0.4, 0.18, 13.2),
                );
                for seat in 0..18 {
                    let seed = side_index * 1000 + row * 60 + section * 22 + seat;
                    let p = Vec3::new(x, y + 0.55, z - 6.15 + seat as f32 * 0.58);
                    if side < 0. {
                        crowd_silhouette(&mut orange_crowd, p, 0.43 + random(seed) * 0.13, seed);
                    } else {
                        crowd_silhouette(&mut blue_crowd, p, 0.43 + random(seed) * 0.13, seed);
                    }
                }
                for edge in [-6.5, 6.5] {
                    beam(
                        c,
                        k,
                        k.wood.clone(),
                        Vec3::new(x, 1., z + edge),
                        Vec3::new(x, y, z + edge),
                        0.22,
                    );
                }
            }
        }
        // Banner gateways, tucked above the far corners.
        let bp = Vec3::new(side * 28., 5.3, -17.);
        for dx in [-1.8, 1.8] {
            beam(
                c,
                k,
                k.wood_light.clone(),
                bp + Vec3::new(dx, -4.2, 0.),
                bp + Vec3::new(dx, 3., 0.),
                0.28,
            );
        }
        banner(
            c,
            k,
            bp,
            if side < 0. { Team::Orange } else { Team::Blue },
            1.35,
        );
        palm_at(c, k, side * 31., -19., 8., side, 1.);
        // Side approach stairs sit on supported shelves rather than hovering.
        for step in 0..16 {
            let y = 1. - step as f32 * 0.44;
            let z = 11. + step as f32 * 0.62;
            block(
                c,
                k,
                k.rock_light.clone(),
                Vec3::new(side * 33., y, z),
                Vec3::new(2.9, 0.42, 0.65),
            );
        }
    }
    c.spawn(PbrBundle {
        mesh: meshes.add(finish_raw(orange_crowd)),
        material: k.orange.clone(),
        ..default()
    });
    c.spawn(PbrBundle {
        mesh: meshes.add(finish_raw(blue_crowd)),
        material: k.blue.clone(),
        ..default()
    });
    // Curved crest fence and old masonry posts follow the summit outline.
    let count = 72;
    for i in 0..count {
        let a = i as f32 / count as f32 * std::f32::consts::TAU;
        let b = (i + 1) as f32 / count as f32 * std::f32::consts::TAU;
        let p = rim(a, SUMMIT_X - 0.7, SUMMIT_Z - 0.7) + Vec3::Y * 1.15;
        let q = rim(b, SUMMIT_X - 0.7, SUMMIT_Z - 0.7) + Vec3::Y * 1.15;
        block(
            c,
            k,
            k.wood_light.clone(),
            p + Vec3::Y * 0.6,
            Vec3::new(0.23, 1.6, 0.23),
        );
        for y in [0.45, 1.05] {
            beam(c, k, k.wood.clone(), p + Vec3::Y * y, q + Vec3::Y * y, 0.16);
        }
        if i % 4 == 0 {
            block(
                c,
                k,
                k.rock_light.clone(),
                p - Vec3::Y * 0.5,
                Vec3::new(1.5, 1.8, 1.2),
            );
            torch(c, k, p + Vec3::Y * 0.42, &flame);
        }
        if i % 2 == 0 {
            sprig(c, k, p - Vec3::Y * 0.12, 0.9, i);
        }
    }
    // Broken vertical buttresses add geological structure without radial paving rings.
    for i in (0..66).step_by(3) {
        let a = i as f32 / 66. * std::f32::consts::TAU;
        let mut p = rim(a, SUMMIT_X - 0.2, SUMMIT_Z - 0.2);
        let h = 4. + random(i + 200) * 8.;
        p.y = -h * 0.5 - 1.;
        let e = block(
            c,
            k,
            if i % 3 == 0 {
                k.rock_light.clone()
            } else {
                k.rock.clone()
            },
            p,
            Vec3::new(1.6 + random(i), h, 1.6),
        );
        c.entity(e).insert(
            Transform::from_translation(p)
                .with_rotation(Quat::from_rotation_y(-a))
                .with_scale(Vec3::new(1.6 + random(i), h, 1.6)),
        );
        // Hanging vines hug the face in irregular lengths.
        for j in 0..2 {
            let mut v = rim(a, SUMMIT_X + 0.55, SUMMIT_Z + 0.5);
            v.y = -1. - j as f32 * 1.35;
            sprig(c, k, v, 0.55 + random(i * 11 + j) * 0.4, i + j);
        }
        if i % 5 == 0 {
            let mut ledge = rim(a, SUMMIT_X + 0.9, SUMMIT_Z + 0.8);
            ledge.y = -8. - random(i) * 10.;
            oval(
                c,
                k,
                k.rock_light.clone(),
                ledge,
                Vec3::new(3., 1.4, 2.7),
                a,
            );
            oval(
                c,
                k,
                greens[i % 6].clone(),
                ledge + Vec3::Y * 1.25,
                Vec3::new(2.8, 0.25, 2.5),
                a,
            );
            palm_at(
                c,
                k,
                ledge.x,
                ledge.z,
                3.5 + random(i) * 2.,
                a,
                ledge.y + 1.5,
            );
        }
    }
    // A recognizable temple entrance under the near touchline.
    block(
        c,
        k,
        k.rock.clone(),
        Vec3::new(0., -7.5, 24.1),
        Vec3::new(8., 10., 1.2),
    );
    for side in [-1., 1.] {
        for course in 0..5 {
            block(
                c,
                k,
                k.rock_light.clone(),
                Vec3::new(side * 3.5, -3. - course as f32 * 1.7, 24.9),
                Vec3::new(1.4, 1.5, 1.1),
            );
        }
    }
    block(
        c,
        k,
        k.rock_light.clone(),
        Vec3::new(0., -2.8, 24.9),
        Vec3::new(8.4, 1.1, 1.3),
    );
    monkey_relief(c, k, Vec3::new(0., -7., 25.05), 1.15);
    for i in 0..20 {
        block(
            c,
            k,
            k.rock_light.clone(),
            Vec3::new(0., -11.5 - i as f32 * 0.48, 25.8 + i as f32 * 0.48),
            Vec3::new(4., 0.48, 0.52),
        );
    }
    for (i, x) in [-24., -12., 12., 24.].into_iter().enumerate() {
        let z = SUMMIT_Z * (1. - (x / SUMMIT_X).powi(4)).powf(0.25) + 0.9;
        banner(
            c,
            k,
            Vec3::new(x, -3.5, z),
            if i % 2 == 0 { Team::Orange } else { Team::Blue },
            1.25,
        );
    }
    // Water drops from spring channels at the edge onto lower vegetated shelves.
    for (i, (x, z, low)) in [(-24., 22., -19.), (22., 22.5, -23.), (-34., -8., -28.)]
        .into_iter()
        .enumerate()
    {
        block(
            c,
            k,
            water.clone(),
            Vec3::new(x, 1.10, z - 0.6),
            Vec3::new(2.8, 0.04, 2.2),
        );
        waterfall(c, k, Vec3::new(x, 1.1, z), low, 2.7, &water, 300 + i * 50);
        oval(
            c,
            k,
            k.rock.clone(),
            Vec3::new(x, low - 1.2, z + 1.7),
            Vec3::new(5., 1.7, 4.),
            0.,
        );
        waterfall(
            c,
            k,
            Vec3::new(x + 1., low, z + 4.),
            -43.,
            3.3,
            &water,
            400 + i * 50,
        );
    }
    // A handful of near outcrops establishes the depth scale and bridge network.
    for (i, (x, z, top, rx, rz)) in [
        (-48., 12., -9., 7., 8.),
        (47., 4., 2., 7., 8.),
        (-47., -27., -5., 10., 9.),
        (52., -32., -8., 10., 12.),
        (-24., -65., -9., 13., 10.),
        (17., -67., -12., 12., 10.),
        (-69., -56., -6., 12., 14.),
        (72., -64., -1., 13., 12.),
    ]
    .into_iter()
    .enumerate()
    {
        c.spawn(PbrBundle {
            mesh: meshes.add(massif(rx, rz, top, -44., i * 73 + 100, false)),
            material: white.clone(),
            transform: Transform::from_xyz(x, 0., z),
            ..default()
        });
        pagoda(c, k, Vec3::new(x, top + 0.2, z), 0.8 + random(i) * 0.4);
        // Dense, unequal clumps occupy the rim; keep a small clear pavilion
        // approach rather than distributing five identical trees on bare turf.
        for shrub in 0..8 {
            let a = shrub as f32 * 2.399 + i as f32;
            let r = 0.62 + random(i * 91 + shrub) * 0.30;
            let q = Vec3::new(
                x + top * 0.09 + a.cos() * rx * r,
                top + 0.18,
                z + a.sin() * rz * r,
            );
            let size = 1.35 + random(i * 49 + shrub) * 1.75;
            oval(
                c,
                k,
                greens[(i + shrub) % 6].clone(),
                q,
                Vec3::new(size, size * 0.6, size * 0.8),
                a,
            );
            if shrub % 4 == 0 {
                sprig(c, k, q + Vec3::Y * 0.5, size * 0.9, shrub);
            }
        }
        for patch in 0..3 {
            let a = patch as f32 * 2.399 + i as f32;
            let depth = 2. + random(patch + i * 37) * 18.;
            let t = depth / (top + 44.);
            let scale = 1. - t * 0.13 + (t - 0.65).max(0.) * 0.55;
            let mut q = rim(a, rx * scale, rz * scale);
            q += Vec3::new(x + (top - depth) * 0.09, top - depth, z);
            let size = 1.0 + random(patch + i * 77) * 1.4;
            oval(
                c,
                k,
                greens[(i + patch) % 6].clone(),
                q,
                Vec3::new(size, size * 0.7, size),
                a,
            );
            if patch % 2 == 0 {
                sprig(c, k, q, size, patch);
            }
        }
        for t in 0..2 {
            let a = t as f32 * 2.399;
            let tx = x + a.cos() * rx * 0.65;
            let tz = z + a.sin() * rz * 0.65;
            palm_at(c, k, tx, tz, 3.5 + random(t + i) * 2.5, a, top);
        }
        if i % 2 == 0 {
            waterfall(
                c,
                k,
                Vec3::new(x - 2., top, z + rz * 0.95),
                -43.,
                2.7,
                &water,
                i * 19,
            );
        }
    }
    bridge(
        c,
        k,
        Vec3::new(-33., -5., 17.),
        Vec3::new(-45., -8.8, 13.),
        2.5,
    );
    bridge(c, k, Vec3::new(33., -4., 17.), Vec3::new(47., 2., 9.), 2.5);
    bridge(
        c,
        k,
        Vec3::new(-44., -4.8, -32.),
        Vec3::new(-25., -8.8, -57.),
        2.1,
    );
    bridge(
        c,
        k,
        Vec3::new(45., 2., -2.),
        Vec3::new(53., -7.8, -22.),
        2.1,
    );
    monkey_relief(c, k, Vec3::new(-48., -12., 19.), 1.6);
    banner(c, k, Vec3::new(47., 8., 5.), Team::Blue, 0.8);
    // Distant vegetation is assembled into a few static meshes. This keeps the
    // horizon rich without issuing one draw call per background tree.
    let mut far_entries: Vec<(Vec3, f32, usize)> = Vec::with_capacity(550);
    for i in 0..320 {
        let x = (random(i * 7 + 3000) - 0.5) * 360.;
        let z = -220. + random(i * 7 + 3001) * 300.;
        if x.abs() < 40. && z.abs() < 31. {
            continue;
        }
        far_entries.push((
            Vec3::new(x, valley_height(x, z), z),
            8. + random(i * 7 + 3002) * 13.,
            i,
        ));
    }
    // Irregular broadleaf clumps colonize the cliff ledges, breaking up long bare faces.
    for i in 0..60 {
        let a = random(i * 3 + 8201) * std::f32::consts::TAU;
        let depth = 2. + random(i * 3 + 8202) * 28.;
        let flare = 1. + depth / 42. * 0.12;
        let mut p = rim(a, SUMMIT_X * flare + 0.2, SUMMIT_Z * flare + 0.3);
        p.y = -depth;
        let s = 0.65 + random(i + 9200) * 1.0;
        oval(
            c,
            k,
            greens[i % 6].clone(),
            p,
            Vec3::new(s, 0.65 * s, s * 0.7),
            a,
        );
        if i % 3 == 0 {
            sprig(c, k, p + Vec3::Y * 0.45, s, i);
        }
    }
    for i in 0..10 {
        let x = -170. + i as f32 * 14. + random(i + 910) * 10.;
        let z = -105. - random(i + 911) * 100.;
        let top = -32. + random(i + 912) * 25.;
        let rx = 10. + random(i + 913) * 15.;
        c.spawn(PbrBundle {
            mesh: meshes.add(mountain(rx * 1.5, top, i * 107 + 90)),
            material: white.clone(),
            transform: Transform::from_xyz(x, 0., z),
            ..default()
        });
        for t in 0..10 {
            let a = t as f32 * 2.399;
            let fraction = 0.2 + random(t + i * 43) * 0.75;
            let r = rx * 1.5 * (0.05 + fraction.powf(0.8) * 0.95) * 0.90;
            far_entries.push((
                Vec3::new(
                    x + a.cos() * r + (1. - fraction) * 3.,
                    top + (-44. - top) * fraction,
                    z + a.sin() * r * 0.72,
                ),
                8. + random(t + i) * 6.,
                i + t,
            ));
        }
    }
    batch_far_forest(c, k, meshes, &greens, &far_entries);
    // The distant ancestor temple is the visual destination of the bridge route.
    c.spawn(PbrBundle {
        mesh: meshes.add(massif(8., 6., -17., -31., 718, false)),
        material: white.clone(),
        transform: Transform::from_xyz(-10., 0., -108.),
        ..default()
    });
    c.spawn(PbrBundle {
        mesh: meshes.add(mountain(26., -17., 717)),
        material: white.clone(),
        transform: Transform::from_xyz(-10., 0., -110.),
        ..default()
    });
    for tier in 0..5 {
        let width = 15. - tier as f32 * 2.;
        block(
            c,
            k,
            k.rock_light.clone(),
            Vec3::new(-10., -16. + tier as f32 * 1.8, -108.),
            Vec3::new(width, 1.8, 9. - tier as f32),
        );
    }
    monkey_relief(c, k, Vec3::new(-10., -7., -103.), 2.);
    for i in 0..24 {
        let a = i as f32 * 2.399;
        let p = Vec3::new(-10. + a.cos() * 10., -28., -110. + a.sin() * 8.);
        sprig(c, k, p, 2., i);
    }
    // Foreground flowers and canopy on the lower ledges leave the playing plane clear.
    for i in 0..24 {
        let a = i as f32 / 24. * std::f32::consts::TAU;
        let mut p = rim(a, SUMMIT_X + 0.4, SUMMIT_Z + 0.4);
        p.y = 0.8;
        sprig(c, k, p, 1.15, i);
        if i % 5 == 0 {
            for petal in 0..5 {
                let b = petal as f32 * 1.256;
                oval(
                    c,
                    k,
                    flowers[i % 3].clone(),
                    p + Vec3::new(b.cos() * 0.35, 0.65, b.sin() * 0.35),
                    Vec3::new(0.4, 0.13, 0.2),
                    b,
                );
            }
        }
    }
    // Localized gorge mist, not an opaque screen-space wash.
    let mist = mats.add(StandardMaterial {
        base_color: Color::rgba(0.80, 0.93, 0.96, 0.10),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for i in 0..8 {
        let x = (random(i + 501) - 0.5) * 180.;
        let z = -95. + random(i + 701) * 120.;
        if x.abs() < 35. && z.abs() < 28. {
            continue;
        }
        oval(
            c,
            k,
            mist.clone(),
            Vec3::new(x, -24. + random(i) * 5., z),
            Vec3::new(10., 2., 5.),
            i as f32,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summit_encloses_playable_field_and_has_real_relief() {
        assert!((24f32 / SUMMIT_X).powi(4) + (16f32 / SUMMIT_Z).powi(4) < 1.);
        let mesh = massif(SUMMIT_X, SUMMIT_Z, 1.04, -41., 10, true);
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(p)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        assert!(p.iter().any(|v| v[1] < -40.));
        assert!(p.iter().all(|v| v[1] <= 1.041));
        assert!(valley_height(80., 0.) < -35.);
    }
}
