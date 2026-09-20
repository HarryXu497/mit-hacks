//! Googly eyes animation system.
//!
//! Makes the pupils move based on the cube's velocity,
//! creating a fun googly eye effect.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{CubePlayer, GooglyPupil};

/// How far up the hierarchy to look for the body a pupil belongs to.
///
/// Pupil -> Eye -> PlayerVisual -> Cube is the deepest arrangement today; the limit only exists
/// so a malformed hierarchy cannot spin here.
const MAX_ANCESTRY: usize = 8;

/// Find the player body a pupil hangs beneath, however many nodes are in between.
///
/// This used to be a hard-coded two hops, which broke the moment the visuals were re-parented
/// under an animated `PlayerVisual` node: the grandparent stopped being the body and the pupils
/// silently froze. Walking until the body is found makes the effect independent of how the
/// visual hierarchy is arranged.
fn body_of(start: Entity, parents: &Query<&Parent>, bodies: &Query<&Velocity, With<CubePlayer>>) -> Option<Entity> {
    let mut current = start;
    for _ in 0..MAX_ANCESTRY {
        if bodies.get(current).is_ok() {
            return Some(current);
        }
        current = parents.get(current).ok()?.get();
    }
    None
}

/// Animate googly eye pupils based on parent cube velocity
pub fn animate_googly_eyes(
    cube_query: Query<&Velocity, With<CubePlayer>>,
    mut pupil_query: Query<(&mut Transform, &GooglyPupil, &Parent)>,
    parent_query: Query<&Parent>,
) {
    for (mut transform, pupil, parent) in pupil_query.iter_mut() {
        if let Some(cube_entity) = body_of(parent.get(), &parent_query, &cube_query) {
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
