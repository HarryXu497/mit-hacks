use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{CubePlayer, PlayerInput};
use crate::game::config::*;

pub fn apply_player_movement(
    mut query: Query<(&PlayerInput, &mut Velocity, &CubePlayer, &Transform)>,
    rapier_context: Res<RapierContext>,
) {
    for (input, mut velocity, _player, transform) in query.iter_mut() {
        // Movement in world coordinates (simple and direct)
        let target_velocity = Vec3::new(
            input.movement.x * CUBE_MAX_SPEED,
            velocity.linvel.y,
            input.movement.y * CUBE_MAX_SPEED,
        );

        // Smooth acceleration
        let current_horizontal = Vec3::new(velocity.linvel.x, 0.0, velocity.linvel.z);
        let target_horizontal = Vec3::new(target_velocity.x, 0.0, target_velocity.z);
        let new_horizontal = current_horizontal.lerp(target_horizontal, 0.2);

        velocity.linvel.x = new_horizontal.x;
        velocity.linvel.z = new_horizontal.z;

        // Rotate cube to face movement direction using angular velocity (physics-based)
        if input.movement.length() > 0.1 {
            let target_angle = input.movement.x.atan2(input.movement.y);
            let current_angle = transform.rotation.to_euler(EulerRot::YXZ).0;

            // Calculate shortest angle difference
            let mut angle_diff = target_angle - current_angle;
            // Normalize to [-PI, PI]
            while angle_diff > std::f32::consts::PI {
                angle_diff -= std::f32::consts::TAU;
            }
            while angle_diff < -std::f32::consts::PI {
                angle_diff += std::f32::consts::TAU;
            }

            // Apply rotation via physics angular velocity
            velocity.angvel.y = angle_diff * 10.0;
        } else {
            // Stop rotating when not moving
            velocity.angvel.y = 0.0;
        }

        // Jump - only if grounded
        if input.jump {
            // Check if grounded via raycast
            let ray_origin = transform.translation;
            let ray_dir = Vec3::NEG_Y;
            let max_dist = CUBE_SIZE / 2.0 + 0.05;  // Tight tolerance

            // Also check that Y velocity is low (not already in the air)
            // Strict threshold: cube must be almost stationary vertically
            let is_in_air = velocity.linvel.y.abs() > 0.1;

            if !is_in_air {
                if let Some(_) = rapier_context.cast_ray(
                    ray_origin,
                    ray_dir,
                    max_dist,
                    true,
                    QueryFilter::default().exclude_sensors(),
                ) {
                    velocity.linvel.y = CUBE_JUMP_FORCE / CUBE_MASS;
                }
            }
        }

        // Clamp position to stay on field
        // This is handled by the physics colliders, but we can add extra clamping if needed
    }
}

/// Clamp player velocity to max speed
pub fn clamp_velocities(
    mut query: Query<&mut Velocity, With<CubePlayer>>,
) {
    for mut velocity in query.iter_mut() {
        let horizontal_speed = Vec2::new(velocity.linvel.x, velocity.linvel.z).length();
        if horizontal_speed > CUBE_MAX_SPEED {
            let factor = CUBE_MAX_SPEED / horizontal_speed;
            velocity.linvel.x *= factor;
            velocity.linvel.z *= factor;
        }
    }
}
