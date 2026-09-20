//! The wordmark as real geometry, with monkeys climbing it.
//!
//! A flat text overlay was the one thing on the title card that did not belong
//! to the world: everything behind it is lit, shaded and outlined by the same
//! rules, and the logo sat in front of all of it like a sticker. Extruding the
//! same typeface into the scene puts it under the same light as the stadium and
//! lets the characters interact with it.
//!
//! The letters are extruded from the shipped TTF rather than modelled, so the
//! wordmark and the menu stay the same typeface by construction.

use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use bevy::render::render_asset::RenderAssetUsages;
use meshtext::{MeshGenerator, MeshText, TextSection};

use crate::rendering::batching::NoMerge;

/// Depth of the extrusion relative to cap height. Deep enough to catch a
/// different shade on the side walls, shallow enough that the counters of
/// letters like A and O do not tunnel into darkness at this camera angle.
const DEPTH: f32 = 0.34;

/// World height of one line of the wordmark.
const LINE_HEIGHT: f32 = 3.2;

/// Gap between the two lines, as a fraction of line height.
const LEADING: f32 = 1.22;

/// Ink shell thickness, in the same world units. The scene outlines characters
/// by drawing an enlarged copy with its front faces culled; the wordmark uses
/// the same trick so its contour matches theirs.
const INK: f32 = 0.10;

/// Baked in rather than loaded through the asset server. meshtext wants raw
/// bytes, and the asset root moves depending on how the game was started -- a
/// logo that silently fails to build when the binary is run directly is not
/// worth the indirection for 68 KB.
pub const FONT: &[u8] = include_bytes!("../../assets/fonts/MPLUSRounded1c-Black.ttf");

/// Marks everything the 3D wordmark spawned, so it can be torn down with the
/// title card.
#[derive(Component)]
pub struct Wordmark;

/// Turns one line of text into a mesh, centred on its own bounding box.
///
/// Centring here rather than at the caller means the two lines stack on a
/// shared axis regardless of how wide each one sets.
fn line_mesh(font: &[u8], text: &str, height: f32) -> Option<(Mesh, f32)> {
    let mut generator = MeshGenerator::new(font.to_vec());
    let flat = false;
    let generated: MeshText = generator
        .generate_section(text, flat, None)
        .ok()?;

    let raw = generated.vertices;
    if raw.is_empty() {
        return None;
    }

    // meshtext lays text out on a unit em starting at the origin and extrudes
    // one unit deep. Scale to the world size we want, and pull the extrusion
    // back to DEPTH so the letters do not read as bricks.
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(raw.len() / 3);
    for chunk in raw.chunks_exact(3) {
        let point = Vec3::new(chunk[0], chunk[1], chunk[2]);
        min = min.min(point);
        max = max.max(point);
        positions.push(point.to_array());
    }

    let span = max - min;
    if span.y <= f32::EPSILON {
        return None;
    }
    let scale = height / span.y;
    let centre = Vec3::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5, min.z);

    for position in &mut positions {
        let p = (Vec3::from_array(*position) - centre) * scale;
        // Depth arrives as a unit cube extrusion; squash it independently.
        *position = Vec3::new(p.x, p.y, p.z * DEPTH / (span.z.max(f32::EPSILON) * scale)).to_array();
    }

    let width = span.x * scale;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    // Left unindexed. meshtext emits loose triangles, and `compute_flat_normals`
    // refuses to run on indexed geometry anyway -- it needs each triangle to own
    // its three vertices. Flat normals suit the look regardless: the whole scene
    // is faceted, and smooth letters would be the only rounded surfaces in frame.
    mesh.compute_flat_normals();
    Some((mesh, width))
}

/// Grows a copy of a mesh along its normals, for the ink shell.
///
/// A fixed world offset rather than a percentage, matching `jungle::INK`: it
/// keeps the line the same weight on a wide letter and a narrow one.
///
/// The normals have to be averaged across coincident vertices first. The letter
/// mesh carries flat normals and no shared vertices, so pushing each vertex
/// along its own face normal moves the two sides of every edge apart and the
/// shell bursts into loose triangles. It is invisible at a hairline offset and
/// obvious at any weight worth drawing, which is a good way to conclude the
/// outline "does not work" when it was only ever too thin to see.
fn swollen(mesh: &Mesh, by: f32) -> Option<Mesh> {
    use std::collections::HashMap;

    let mut grown = mesh.clone();
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)?
        .as_float3()?
        .to_vec();
    let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)?.as_float3()?.to_vec();

    /// Quantised so vertices that should be the same point hash together
    /// despite the triangulator's rounding. Fine enough to keep genuinely
    /// distinct corners apart at this scale.
    fn key(p: &[f32; 3]) -> (i32, i32, i32) {
        const GRID: f32 = 2048.;
        (
            (p[0] * GRID).round() as i32,
            (p[1] * GRID).round() as i32,
            (p[2] * GRID).round() as i32,
        )
    }

    let mut welded: HashMap<(i32, i32, i32), Vec3> = HashMap::new();
    for (position, normal) in positions.iter().zip(normals.iter()) {
        *welded.entry(key(position)).or_insert(Vec3::ZERO) += Vec3::from_array(*normal);
    }

    // The letters are an extrusion, so the shell is built as one too: pushed
    // outward in the plane of the type, and moved off each cap along the axis.
    //
    // Offsetting along the averaged normal instead sounds equivalent and is
    // not. The caps are triangulated with interior vertices; a contour vertex
    // averages to a diagonal while its neighbours inside the cap average to
    // straight along the axis, which shears the cap, flips some of its
    // triangles and lays dark slivers across the face of every letter. Keeping
    // the two directions separate cannot distort a flat face.
    let mid_depth = {
        let (mut near, mut far) = (f32::MAX, f32::MIN);
        for p in &positions {
            near = near.min(p[2]);
            far = far.max(p[2]);
        }
        (near + far) * 0.5
    };

    let pushed: Vec<[f32; 3]> = positions
        .iter()
        .zip(normals.iter())
        .map(|(p, n)| {
            let averaged = welded
                .get(&key(p))
                .copied()
                .unwrap_or(Vec3::from_array(*n));
            // Only the in-plane part: the axial part belongs to the cap push.
            // An interior cap vertex averages to pure axis, so this is zero for
            // it and it simply travels with its cap.
            let outward = Vec2::new(averaged.x, averaged.y).normalize_or_zero() * by;
            // Pulled *in* along the axis, not out. The outline comes entirely
            // from the in-plane growth, so the shell has no reason to reach
            // toward the camera -- and every reason not to. The triangulator
            // winds a few cap triangles the other way, and those survive the
            // front-face cull; ahead of the letter they draw as dark slivers
            // across its face, behind it they are simply depth-tested away.
            let axial = if p[2] >= mid_depth { -by } else { by };
            [p[0] + outward.x, p[1] + outward.y, p[2] + axial]
        })
        .collect();
    grown.insert_attribute(Mesh::ATTRIBUTE_POSITION, pushed);
    Some(grown)
}

#[allow(clippy::too_many_arguments)]
/// Builds the wordmark and returns its root, which starts at the origin.
///
/// Placement is the caller's: this lives in front of a fixed title shot in one
/// app and rides an orbiting lobby camera in another, and those want different
/// transforms rather than a default that is wrong for both.
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    font: &[u8],
    lines: [&str; 2],
) -> Entity {
    let banana = materials.add(StandardMaterial {
        base_color: Color::rgb(0.99, 0.80, 0.20),
        perceptual_roughness: 0.62,
        ..default()
    });
    let ink = materials.add(StandardMaterial {
        base_color: Color::rgb(0.05, 0.04, 0.06),
        unlit: true,
        cull_mode: Some(bevy::render::render_resource::Face::Front),
        ..default()
    });

    let root = commands
        .spawn((
            Wordmark,
            NoMerge,
            SpatialBundle::default(),
        ))
        .id();

    for (index, text) in lines.iter().enumerate() {
        let Some((mesh, width)) = line_mesh(font, text, LINE_HEIGHT) else {
            warn!("could not build 3D wordmark for {text:?}");
            continue;
        };
        // Lines stack downward from the first, and each is nudged right of the
        // one above so the block leans into the frame rather than sitting square.
        let y = -(index as f32) * LINE_HEIGHT * LEADING;
        // `line_mesh` centres each line on its own box, so shifting by half its
        // width puts every left edge on the same vertical.
        let x = width * 0.5;
        let line = commands
            .spawn((
                Wordmark,
                NoMerge,
                PbrBundle {
                    mesh: meshes.add(mesh.clone()),
                    material: banana.clone(),
                    transform: Transform::from_xyz(x, y, 0.),
                    ..default()
                },
            ))
            .id();
        commands.entity(root).add_child(line);

        if let Some(shell) = swollen(&mesh, INK) {
            let outline = commands
                .spawn((
                    Wordmark,
                    NoMerge,
                    PbrBundle {
                        mesh: meshes.add(shell),
                        material: ink.clone(),
                        transform: Transform::from_xyz(x, y, 0.),
                        ..default()
                    },
                ))
                .id();
            commands.entity(root).add_child(outline);
        }

        // The line runs from 0 to `width` in this space, so characters can be
        // placed against its ends by fraction.
        const SIT: f32 = 0.78;
        if index == 0 {
            // Astride the top of the line, straddling the extrusion, legs over
            // the front face. Sits over the N/O so it breaks the skyline of the
            // word rather than hanging off an end where it reads as a stray.
            perch(
                commands, meshes, materials, root,
                Vec3::new(width * 0.46, LINE_HEIGHT * 0.5 + 1.08 * SIT, DEPTH * 0.5),
                SIT, false,
            );
        } else {
            // Hanging by both arms off the last letter.
            perch(
                commands, meshes, materials, root,
                Vec3::new(width * 0.94, y + LINE_HEIGHT * 0.5 - 0.72 * SIT, DEPTH * 0.5),
                SIT, true,
            );
            // And one stood in front of the first letter, looking out of frame
            // at the reader. Pushed clear of the face so it never z-fights.
            perch(
                commands, meshes, materials, root,
                Vec3::new(width * 0.10, y - LINE_HEIGHT * 0.5 - 0.55 * 0.62, DEPTH + 0.42),
                0.62, false,
            );
        }
    }
    root
}

/// A blocky monkey small enough to sit on a letter.
///
/// Built here rather than reusing the pitch character: that one is assembled
/// from eleven parts with ink shells on each, and three of them on the logo
/// would cost more draws than the wordmark itself. This is the same silhouette
/// at the size it is actually seen — head, muzzle, ears, eyes, body, limbs.
fn perch(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    parent: Entity,
    at: Vec3,
    scale: f32,
    hanging: bool,
) {
    let cube = meshes.add(Cuboid::new(1., 1., 1.));
    let fur = materials.add(StandardMaterial {
        base_color: Color::rgb(0.67, 0.41, 0.18),
        perceptual_roughness: 0.78,
        ..default()
    });
    let muzzle = materials.add(StandardMaterial {
        base_color: Color::rgb(0.96, 0.79, 0.55),
        perceptual_roughness: 0.74,
        ..default()
    });
    let eye = materials.add(StandardMaterial {
        base_color: Color::rgb(0.06, 0.05, 0.07),
        unlit: true,
        ..default()
    });

    let body = commands
        .spawn((
            Wordmark,
            NoMerge,
            SpatialBundle::from_transform(
                Transform::from_translation(at)
                    .with_scale(Vec3::splat(scale))
                    // A hanging monkey turns to face along the line it grips.
                    .with_rotation(Quat::from_rotation_z(if hanging { 0.22 } else { -0.06 })),
            ),
        ))
        .id();
    commands.entity(parent).add_child(body);

    let mut part = |material: &Handle<StandardMaterial>, offset: Vec3, size: Vec3| {
        let piece = commands
            .spawn((
                Wordmark,
                NoMerge,
                // Same treatment as a player: the shader draws a dark contour
                // and a specular hotspot on anything marked an actor, which is
                // what separates a figure from the thing it is standing on.
                crate::rendering::stylized::ActorSurface,
                PbrBundle {
                    mesh: cube.clone(),
                    material: material.clone(),
                    transform: Transform::from_translation(offset).with_scale(size),
                    ..default()
                },
            ))
            .id();
        commands.entity(body).add_child(piece);
    };

    part(&fur, Vec3::new(0., 0.34, 0.), Vec3::new(0.92, 0.80, 0.80)); // head
    part(&muzzle, Vec3::new(0., 0.22, 0.44), Vec3::new(0.52, 0.40, 0.16)); // muzzle
    part(&fur, Vec3::new(-0.52, 0.38, 0.), Vec3::new(0.18, 0.34, 0.34)); // ears
    part(&fur, Vec3::new(0.52, 0.38, 0.), Vec3::new(0.18, 0.34, 0.34));
    part(&eye, Vec3::new(-0.20, 0.46, 0.41), Vec3::new(0.14, 0.18, 0.06));
    part(&eye, Vec3::new(0.20, 0.46, 0.41), Vec3::new(0.14, 0.18, 0.06));
    part(&fur, Vec3::new(0., -0.34, 0.), Vec3::new(0.72, 0.72, 0.62)); // body

    if hanging {
        // Arms up and gripping; legs dangling.
        part(&fur, Vec3::new(-0.46, 0.34, 0.), Vec3::new(0.20, 0.72, 0.22));
        part(&fur, Vec3::new(0.46, 0.34, 0.), Vec3::new(0.20, 0.72, 0.22));
        part(&fur, Vec3::new(-0.20, -0.86, 0.), Vec3::new(0.22, 0.56, 0.24));
        part(&fur, Vec3::new(0.20, -0.86, 0.), Vec3::new(0.22, 0.56, 0.24));
    } else {
        // Sitting: arms down at the sides, legs forward over the edge.
        part(&fur, Vec3::new(-0.46, -0.34, 0.), Vec3::new(0.20, 0.56, 0.22));
        part(&fur, Vec3::new(0.46, -0.34, 0.), Vec3::new(0.20, 0.56, 0.22));
        part(&fur, Vec3::new(-0.20, -0.72, 0.30), Vec3::new(0.24, 0.24, 0.62));
        part(&fur, Vec3::new(0.20, -0.72, 0.30), Vec3::new(0.24, 0.24, 0.62));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Vec<u8> {
        FONT.to_vec()
    }

    #[test]
    fn a_line_extrudes_to_the_requested_height_and_has_depth() {
        let (mesh, width) = line_mesh(&font(), "MONKEY", LINE_HEIGHT).expect("mesh");
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in positions {
            let p = Vec3::from_array(*p);
            min = min.min(p);
            max = max.max(p);
        }
        let span = max - min;
        assert!(
            (span.y - LINE_HEIGHT).abs() < 0.01,
            "height was {}, wanted {LINE_HEIGHT}",
            span.y
        );
        // Flat text would be the bug worth catching: the whole point is depth.
        assert!(
            (span.z - DEPTH).abs() < 0.01,
            "extrusion was {}, wanted {DEPTH}",
            span.z
        );
        assert!(width > LINE_HEIGHT, "six caps should set wider than they are tall");
        // Centred on its own box, so the two lines stack on a shared axis.
        assert!(((min.x + max.x) * 0.5).abs() < 0.01);
        assert!(((min.y + max.y) * 0.5).abs() < 0.01);
    }

    #[test]
    fn the_ink_shell_encloses_the_letters_it_outlines() {
        let (mesh, _) = line_mesh(&font(), "BUSINESS", LINE_HEIGHT).expect("mesh");
        let shell = swollen(&mesh, INK).expect("shell");
        let extent = |mesh: &Mesh| {
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap();
            let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for p in positions {
                let p = Vec3::from_array(*p);
                min = min.min(p);
                max = max.max(p);
            }
            max - min
        };
        let letters = extent(&mesh);
        let outline = extent(&shell);
        assert!(outline.x > letters.x, "shell is not wider than the letters");
        assert!(outline.y > letters.y, "shell is not taller than the letters");
        // Grown by a fixed offset, so both axes gain about the same amount
        // rather than the wider one gaining more.
        assert!(
            ((outline.x - letters.x) - (outline.y - letters.y)).abs() < INK,
            "growth is proportional, not a constant offset"
        );
    }

    #[test]
    fn both_lines_of_the_wordmark_build() {
        for text in ["MONKEY", "BUSINESS"] {
            assert!(line_mesh(&font(), text, LINE_HEIGHT).is_some(), "{text} failed");
        }
    }
}
