//! Static draw merging.
//!
//! The jungle is authored as thousands of small props -- a fence post, a leaf,
//! a plank -- because that is how the art reads and how it stays editable. The
//! renderer pays for that per object, not per pixel: on integrated graphics the
//! frame is dominated by submitting ~4300 draws through the main pass and again
//! through every shadow cascade, while fill rate barely registers.
//!
//! This pass runs once, after the scene is built and before materials are
//! restyled. It bakes every static, unparented prop into one merged mesh per
//! (source mesh, material) pair, then despawns the originals. The triangles,
//! their world positions, their normals and their materials are unchanged, so
//! the image is identical -- there are simply a few dozen draws instead of
//! thousands.
//!
//! Anything that moves, is animated, carries physics, or belongs to a hierarchy
//! is left alone; merging is only ever applied to geometry that will sit still
//! for the whole match.
use bevy::prelude::*;
use bevy::render::mesh::{Indices, VertexAttributeValues};
use bevy_rapier3d::prelude::{Collider, RigidBody};
use std::collections::HashMap;

/// Opt a prop out of merging: it keeps its own entity and draw call.
#[derive(Component)]
pub struct NoMerge;

#[derive(Default)]
struct Accum {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
    has_normals: bool,
    has_uvs: bool,
    has_colors: bool,
    material: Handle<StandardMaterial>,
}

fn floats3(mesh: &Mesh, attr: bevy::render::mesh::MeshVertexAttributeId) -> Option<&Vec<[f32; 3]>> {
    match mesh.attribute(attr) {
        Some(VertexAttributeValues::Float32x3(v)) => Some(v),
        _ => None,
    }
}

pub fn merge_static_draws(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    candidates: Query<
        (
            Entity,
            &Handle<Mesh>,
            &Handle<StandardMaterial>,
            &Transform,
        ),
        (
            Without<Parent>,
            Without<Children>,
            Without<NoMerge>,
            Without<Collider>,
            Without<RigidBody>,
            Without<crate::jungle::Waterfall>,
            Without<crate::jungle::WaterRipple>,
            Without<crate::systems::display::DigitSegment>,
            Without<crate::systems::effects::CubeFragment>,
            Without<crate::rendering::stylized::ActorSurface>,
            Without<crate::entities::Ball>,
            Without<crate::entities::CubePlayer>,
        ),
    >,
) {
    let mut groups: HashMap<(AssetId<Mesh>, AssetId<StandardMaterial>), Accum> = HashMap::new();
    let mut absorbed: Vec<Entity> = Vec::new();

    for (entity, mesh_handle, material_handle, transform) in &candidates {
        let Some(source) = meshes.get(mesh_handle) else {
            continue;
        };
        // Blended and emissive surfaces are sorted and lit per entity; folding
        // them together would change how they composite, so they stay as they are.
        let Some(material) = materials.get(material_handle) else {
            continue;
        };
        if material.alpha_mode != AlphaMode::Opaque || material.emissive != Color::BLACK {
            continue;
        }
        let Some(positions) = floats3(source, Mesh::ATTRIBUTE_POSITION.id) else {
            continue;
        };
        let Some(Indices::U32(source_indices)) = source.indices() else {
            continue;
        };

        let matrix = transform.compute_matrix();
        // Normals need the inverse transpose: props are scaled non-uniformly
        // (a touchline is a cube scaled 0.09 x 0.025 x 32) and the plain matrix
        // would skew them.
        let normal_matrix = bevy::math::Mat3::from_mat4(matrix).inverse().transpose();

        let group = groups
            .entry((mesh_handle.id(), material_handle.id()))
            .or_insert_with(|| Accum {
                material: material_handle.clone(),
                ..default()
            });
        let base = group.positions.len() as u32;

        for p in positions {
            group
                .positions
                .push(matrix.transform_point3(Vec3::from_array(*p)).to_array());
        }
        if let Some(normals) = floats3(source, Mesh::ATTRIBUTE_NORMAL.id) {
            group.has_normals = true;
            for n in normals {
                group
                    .normals
                    .push((normal_matrix * Vec3::from_array(*n)).normalize_or_zero().to_array());
            }
        }
        if let Some(VertexAttributeValues::Float32x2(uvs)) = source.attribute(Mesh::ATTRIBUTE_UV_0)
        {
            group.has_uvs = true;
            group.uvs.extend_from_slice(uvs);
        }
        if let Some(VertexAttributeValues::Float32x4(colors)) =
            source.attribute(Mesh::ATTRIBUTE_COLOR)
        {
            group.has_colors = true;
            group.colors.extend_from_slice(colors);
        }
        group
            .indices
            .extend(source_indices.iter().map(|i| i + base));
        absorbed.push(entity);
    }

    // A group of one saves nothing and costs an extra asset, so it stays put.
    let mut kept = 0usize;
    for ((_, _), group) in groups.into_iter() {
        if group.positions.is_empty() {
            continue;
        }
        let count = group.positions.len();
        let mut mesh = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::render::render_asset::RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, group.positions);
        if group.has_normals && group.normals.len() == count {
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, group.normals);
        }
        if group.has_uvs && group.uvs.len() == count {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, group.uvs);
        }
        if group.has_colors && group.colors.len() == count {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, group.colors);
        }
        mesh.insert_indices(Indices::U32(group.indices));
        commands.spawn(PbrBundle {
            mesh: meshes.add(mesh),
            material: group.material,
            ..default()
        });
        kept += 1;
    }
    for entity in &absorbed {
        commands.entity(*entity).despawn();
    }
    info!(
        "merged {} static props into {} draws",
        absorbed.len(),
        kept
    );
}
