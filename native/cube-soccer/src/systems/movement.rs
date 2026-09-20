use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{CubePlayer, PlayerInput};
use crate::game::config::*;

/// Next horizontal velocity `(x, z)` from control input, folding in tactic
/// speed/accel factors. Pure (no ECS): `input` is `(move_x, move_z)`, `current`
/// and the return are `(x, z)`.
pub fn next_horizontal_velocity(input: Vec2, current: Vec2, speed_factor: f32, accel_factor: f32) -> Vec2 {
    let max_speed = CUBE_MAX_SPEED * speed_factor;
    let target = input * max_speed;
    let lerp_rate = (0.2 * accel_factor).clamp(0.0, 1.0);
    current.lerp(target, lerp_rate)
}

pub fn apply_player_movement(
    mut query: Query<(&PlayerInput, &mut Velocity, &CubePlayer, &Transform, &crate::systems::status_effects::StatusEffects)>,
    rapier_context: Res<RapierContext>,
) {
    for (input, mut velocity, _player, transform, status) in query.iter_mut() {
        let current_h = Vec2::new(velocity.linvel.x, velocity.linvel.z);
        let new_h = next_horizontal_velocity(input.movement, current_h, status.speed_factor, status.accel_factor);
        velocity.linvel.x = new_h.x;
        velocity.linvel.z = new_h.y; // Vec2.y is the world-Z component

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

/// Safety cap: bound horizontal speed to MAX_SPEED_SAFETY * CUBE_MAX_SPEED so a
/// knockback/force spike can't blow up the physics. Normal control never exceeds
/// 1x, so this only binds during external pushes.
pub fn clamp_velocities(mut query: Query<&mut Velocity, With<CubePlayer>>) {
    let cap = CUBE_MAX_SPEED * MAX_SPEED_SAFETY;
    for mut velocity in query.iter_mut() {
        let horizontal_speed = Vec2::new(velocity.linvel.x, velocity.linvel.z).length();
        if horizontal_speed > cap {
            let factor = cap / horizontal_speed;
            velocity.linvel.x *= factor;
            velocity.linvel.z *= factor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_cube_velocity_decays_to_zero() {
        let mut v = Vec2::new(10.0, 0.0);
        for _ in 0..40 {
            v = next_horizontal_velocity(Vec2::new(1.0, 0.0), v, 0.0, 1.0);
        }
        assert!(v.length() < 0.01, "frozen cube velocity should decay to ~0, got {:?}", v);
    }

    #[test]
    fn normal_cube_approaches_max_speed() {
        let mut v = Vec2::ZERO;
        for _ in 0..100 {
            v = next_horizontal_velocity(Vec2::new(1.0, 0.0), v, 1.0, 1.0);
        }
        assert!((v.x - CUBE_MAX_SPEED).abs() < 0.1, "should approach CUBE_MAX_SPEED, got {}", v.x);
    }

    #[test]
    fn speed_factor_scales_top_speed() {
        let mut v = Vec2::ZERO;
        for _ in 0..100 {
            v = next_horizontal_velocity(Vec2::new(1.0, 0.0), v, 0.5, 1.0);
        }
        assert!((v.x - CUBE_MAX_SPEED * 0.5).abs() < 0.1, "half speed_factor => half top speed, got {}", v.x);
    }
}
