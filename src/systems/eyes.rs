//! Googly eyes animation system.
//!
//! Makes the pupils move based on the cube's velocity,
//! creating a fun googly eye effect.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{CubePlayer, GooglyPupil};

/// Animate googly eye pupils based on parent cube velocity
pub fn animate_googly_eyes(
    cube_query: Query<&Velocity, With<CubePlayer>>,
    mut pupil_query: Query<(&mut Transform, &GooglyPupil, &Parent)>,
    parent_query: Query<&Parent>,
) {
    for (mut transform, pupil, parent) in pupil_query.iter_mut() {
        // Navigate up the hierarchy: Pupil -> Eye -> Cube
        let eye_entity = parent.get();
        if let Ok(eye_parent) = parent_query.get(eye_entity) {
            let cube_entity = eye_parent.get();
            if let Ok(velocity) = cube_query.get(cube_entity) {
                // Calculate pupil offset based on velocity
                // Pupils move opposite to acceleration (they "lag behind")
                let vel = velocity.linvel;
                let max_offset = 0.06;  // Maximum pupil movement
                let velocity_factor = 0.02;  // How much velocity affects pupil position

                // Map velocity to pupil offset (inverted for "lag" effect)
                let offset_x = (-vel.x * velocity_factor).clamp(-max_offset, max_offset);
                let offset_y = (-vel.y * velocity_factor * 0.5).clamp(-max_offset, max_offset);

                // Apply offset relative to base position
                transform.translation = pupil.base_offset + Vec3::new(offset_x, offset_y, 0.0);
            }
        }
    }
}
