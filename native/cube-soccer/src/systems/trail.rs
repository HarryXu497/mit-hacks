use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::CubePlayer;
use crate::game::config::Team;

/// A trail particle that fades and shrinks over time
#[derive(Component)]
pub struct TrailParticle {
    pub lifetime: f32,
    pub max_lifetime: f32,
}

/// Resource to track spawn timing
#[derive(Resource)]
pub struct TrailSpawnTimer(pub Timer);

impl Default for TrailSpawnTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.03, TimerMode::Repeating))
    }
}

/// Spawn trail particles behind moving cubes
pub fn spawn_trail_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<TrailSpawnTimer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(&Transform, &Velocity, &CubePlayer)>,
) {
    timer.0.tick(time.delta());

    if !timer.0.just_finished() {
        return;
    }

    for (transform, velocity, player) in query.iter() {
        let horizontal_speed = Vec2::new(velocity.linvel.x, velocity.linvel.z).length();

        // Only spawn trail if moving fast enough
        if horizontal_speed < 2.0 {
            continue;
        }

        // Get team color with some transparency
        let base_color = match player.team {
            Team::Orange => Color::rgba(1.0, 0.5, 0.0, 0.8),
            Team::Blue => Color::rgba(0.2, 0.4, 1.0, 0.8),
        };

        // Spawn position slightly behind the cube (opposite to velocity)
        let velocity_dir = Vec3::new(velocity.linvel.x, 0.0, velocity.linvel.z).normalize_or_zero();
        let spawn_pos = transform.translation - velocity_dir * 0.5 + Vec3::new(
            (rand_f32() - 0.5) * 0.3,
            (rand_f32() - 0.5) * 0.3 - 0.2,
            (rand_f32() - 0.5) * 0.3,
        );

        // Size based on speed
        let size = 0.1 + (horizontal_speed / 20.0).min(0.15);

        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(size, size, size)),
                material: materials.add(StandardMaterial {
                    base_color,
                    emissive: base_color.into(),
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                }),
                transform: Transform::from_translation(spawn_pos)
                    .with_rotation(Quat::from_euler(
                        EulerRot::XYZ,
                        rand_f32() * std::f32::consts::TAU,
                        rand_f32() * std::f32::consts::TAU,
                        rand_f32() * std::f32::consts::TAU,
                    )),
                ..default()
            },
            TrailParticle {
                lifetime: 0.0,
                max_lifetime: 0.4 + rand_f32() * 0.2,
            },
        ));
    }
}

/// Animate trail particles (fade, shrink, despawn)
pub fn animate_trail_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut TrailParticle, &mut Transform, &Handle<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut particle, mut transform, material_handle) in query.iter_mut() {
        particle.lifetime += time.delta_seconds();

        let progress = particle.lifetime / particle.max_lifetime;

        if progress >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // Shrink over time
        let scale = 1.0 - progress * 0.8;
        transform.scale = Vec3::splat(scale);

        // Slight upward drift
        transform.translation.y += time.delta_seconds() * 0.5;

        // Rotate slowly
        transform.rotate_y(time.delta_seconds() * 2.0);

        // Fade out
        if let Some(material) = materials.get_mut(material_handle) {
            let alpha = 1.0 - progress;
            material.base_color = material.base_color.with_a(alpha * 0.8);
        }
    }
}

/// Simple pseudo-random function (deterministic but varied enough for particles)
fn rand_f32() -> f32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    nanos as f32 / u32::MAX as f32
}
