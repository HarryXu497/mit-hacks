use bevy::prelude::*;
use crate::game::config::*;

/// Component for cube fragments during decomposition
#[derive(Component)]
pub struct CubeFragment {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

/// Spawn decomposition effect at a position with a specific color
pub fn spawn_decomposition(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    position: Vec3,
    color: Color,
) {
    let fragment_size = CUBE_SIZE / 4.0;
    let fragment_mesh = meshes.add(Cuboid::new(fragment_size, fragment_size, fragment_size));

    // Create 8 fragments (2x2x2 grid)
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                let offset = Vec3::new(x, y, z) * (CUBE_SIZE / 4.0);
                let fragment_pos = position + offset;

                // Random-ish velocity based on position
                let velocity = (offset.normalize_or_zero() + Vec3::Y * 0.5) * 8.0;

                let fragment_material = materials.add(StandardMaterial {
                    base_color: color,
                    ..default()
                });

                commands.spawn((
                    PbrBundle {
                        mesh: fragment_mesh.clone(),
                        material: fragment_material,
                        transform: Transform::from_translation(fragment_pos),
                        ..default()
                    },
                    CubeFragment {
                        velocity,
                        lifetime: 0.0,
                        max_lifetime: 0.8,
                    },
                ));
            }
        }
    }
}

/// System to animate and remove cube fragments
pub fn animate_fragments(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Transform, &mut CubeFragment, &Handle<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let delta = time.delta_seconds();
    let gravity = Vec3::new(0.0, -15.0, 0.0);

    for (entity, mut transform, mut fragment, material_handle) in query.iter_mut() {
        // Update lifetime
        fragment.lifetime += delta;

        // Remove if expired
        if fragment.lifetime >= fragment.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Apply velocity and gravity
        fragment.velocity += gravity * delta;
        transform.translation += fragment.velocity * delta;

        // Rotate
        transform.rotate_x(delta * 5.0);
        transform.rotate_y(delta * 3.0);

        // Scale down over time
        let progress = fragment.lifetime / fragment.max_lifetime;
        let scale = 1.0 - progress;
        transform.scale = Vec3::splat(scale.max(0.1));

        // Fade out
        if let Some(material) = materials.get_mut(material_handle) {
            let alpha = 1.0 - progress;
            material.base_color.set_a(alpha);
            material.alpha_mode = AlphaMode::Blend;
        }
    }
}
