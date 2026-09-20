//! Striking the ball.
//!
//! Before this the ball moved only by being walked into, so there was no passing and no
//! shooting -- a cube could shove the ball toward a goal but never strike it at one. A kick
//! sets the ball's velocity outright, the same way [`crate::systems::movement`] sets a
//! player's, rather than applying an impulse: a pass then arrives at a speed the passer chose,
//! which is what makes a pass worth aiming and a shot worth taking.
//!
//! Only the played game schedules these systems. The headless training environment does not,
//! and no controller there ever sets [`PlayerInput::kick`], so the simulation it samples is
//! unchanged.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use std::collections::HashMap;

use crate::entities::{Ball, CubePlayer, PlayerInput};
use crate::game::config::{
    BALL_RADIUS, CUBE_SIZE, FIELD_HEIGHT, KICK_COOLDOWN_SECS, KICK_LOFT, KICK_RANGE,
    STEAL_COOLDOWN_SECS,
};
use crate::systems::possession::Possession;

/// Per-player re-strike cooldowns, in seconds remaining.
///
/// Without one, a player standing against the ball kicks on every frame and the ball simply
/// vibrates in place instead of travelling.
#[derive(Resource, Default)]
pub struct KickCooldowns(pub HashMap<Entity, f32>);

impl KickCooldowns {
    pub fn ready(&self, player: Entity) -> bool {
        !self.0.contains_key(&player)
    }
}

/// Count down and drop expired kick cooldowns. Mirrors `possession::tick_cooldowns`.
pub fn tick_kick_cooldowns(time: Res<Time>, mut cooldowns: ResMut<KickCooldowns>) {
    let dt = time.delta_seconds();
    cooldowns.0.retain(|_, remaining| {
        *remaining -= dt;
        *remaining > 0.0
    });
}

/// The velocity a kick gives the ball.
///
/// Pure, so the aiming maths can be tested without a world. A zero or non-finite direction
/// yields `None` -- the caller wanted no kick, or asked for one that cannot be aimed.
///
/// The vertical component is a small fraction of the speed ([`KICK_LOFT`]) rather than part of
/// `dir`: it lifts the ball clear of the surface so it is not shoved along underneath the
/// cubes, while keeping every kick's apex far below the crossbar, so lofting a shot can never
/// carry it over the bar.
pub fn kick_velocity(dir: Vec3, speed: f32) -> Option<Vec3> {
    let flat = Vec3::new(dir.x, 0.0, dir.z);
    if !flat.is_finite() || flat.length_squared() < 1e-6 || !speed.is_finite() || speed <= 0.0 {
        return None;
    }
    let flat = flat.normalize();
    Some(flat * speed + Vec3::Y * speed * KICK_LOFT)
}

/// Whether a player at `player_pos` may strike a ball at `ball_pos`.
///
/// Distance is measured in the horizontal plane. A ball bouncing overhead is out of reach for
/// a kick even though its centre may be close, so the vertical gap is checked separately
/// against the height a cube can actually reach.
pub fn within_kick_range(player_pos: Vec3, ball_pos: Vec3) -> bool {
    let flat = Vec2::new(ball_pos.x - player_pos.x, ball_pos.z - player_pos.z);
    if flat.length() > KICK_RANGE {
        return false;
    }
    let reach = CUBE_SIZE / 2.0 + BALL_RADIUS + 0.5;
    (ball_pos.y - player_pos.y).abs() <= reach
}

/// System: consume this frame's [`PlayerInput::kick`] requests.
///
/// A request is honoured when the player is off cooldown and within reach; the request is
/// cleared either way, so a stale one never fires a frame late. Striking the ball also puts
/// the kicker on a possession re-grab cooldown and releases the ball if they were holding it,
/// so a player cannot pass to a teammate and immediately reclaim their own pass.
pub fn apply_kicks(
    mut cooldowns: ResMut<KickCooldowns>,
    mut possession: ResMut<Possession>,
    mut ball_query: Query<(&Transform, &mut Velocity), (With<Ball>, Without<CubePlayer>)>,
    mut player_query: Query<(Entity, &mut PlayerInput, &Transform), With<CubePlayer>>,
) {
    let Ok((ball_transform, mut ball_velocity)) = ball_query.get_single_mut() else {
        // Clear the requests anyway: without a ball they can only go stale.
        for (_, mut input, _) in player_query.iter_mut() {
            input.kick = None;
        }
        return;
    };
    let ball_pos = ball_transform.translation;

    // At most one kick per frame reaches the ball. Two players striking the same ball in the
    // same frame would have the second silently overwrite the first, which reads as one of
    // them kicking thin air; taking the nearest is at least the one who got there.
    let mut best: Option<(Entity, Vec3, f32)> = None;
    for (entity, mut input, transform) in player_query.iter_mut() {
        let Some(request) = input.kick.take() else { continue };
        if !cooldowns.ready(entity) || !within_kick_range(transform.translation, ball_pos) {
            continue;
        }
        let Some(velocity) = kick_velocity(request.dir, request.speed) else { continue };
        let distance = transform.translation.distance(ball_pos);
        if best.map_or(true, |(_, _, d)| distance < d) {
            best = Some((entity, velocity, distance));
        }
    }

    let Some((kicker, velocity, _)) = best else { return };

    ball_velocity.linvel = velocity;
    ball_velocity.angvel = Vec3::ZERO;
    cooldowns.0.insert(kicker, KICK_COOLDOWN_SECS);
    possession.cooldowns.insert(kicker, STEAL_COOLDOWN_SECS);
    if possession.holder == Some(kicker) {
        possession.holder = None;
        possession.steal_progress.clear();
    }
}

/// Clear kick cooldowns, for a goal or round reset. Paired with `possession::clear_possession`.
pub fn clear_kick_cooldowns(mut cooldowns: ResMut<KickCooldowns>) {
    cooldowns.0.clear();
}

/// The lowest a kicked ball can be and still be struck, for callers choosing a target.
pub fn ground_ball_height() -> f32 {
    FIELD_HEIGHT + BALL_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kick_leaves_at_the_requested_speed() {
        let v = kick_velocity(Vec3::new(3.0, 0.0, 0.0), 20.0).expect("kickable");
        let flat = Vec2::new(v.x, v.z).length();
        assert!((flat - 20.0).abs() < 1e-3, "flat speed should be the requested one, got {flat}");
    }

    #[test]
    fn a_kick_is_aimed_where_it_was_pointed() {
        let v = kick_velocity(Vec3::new(0.0, 0.0, -4.0), 10.0).expect("kickable");
        assert!(v.z < 0.0 && v.x.abs() < 1e-3, "should travel along -z, got {v:?}");
    }

    #[test]
    fn a_kick_has_no_direction_of_its_own() {
        assert!(kick_velocity(Vec3::ZERO, 20.0).is_none());
        assert!(kick_velocity(Vec3::Y, 20.0).is_none(), "vertical only is not an aim");
        assert!(kick_velocity(Vec3::X, 0.0).is_none(), "a kick with no speed is not a kick");
    }

    /// Loft must never turn a shot into a ball over the bar.
    ///
    /// The vertical component is a fraction of the kick speed, so the fastest kick in the game
    /// is also the highest one. Its apex still has to clear the ball's resting height by less
    /// than the goal is tall, or shooting hard would be a way to miss.
    #[test]
    fn the_hardest_kick_still_passes_under_the_crossbar() {
        use crate::game::config::{GOAL_HEIGHT, GRAVITY, SHOT_SPEED};
        let v = kick_velocity(Vec3::X, SHOT_SPEED).expect("kickable");
        let apex = ground_ball_height() + v.y * v.y / (2.0 * GRAVITY.abs());
        let bar = FIELD_HEIGHT + GOAL_HEIGHT;
        assert!(apex < bar, "apex {apex} should stay under the crossbar at {bar}");
    }

    #[test]
    fn the_ball_must_be_within_reach_to_be_struck() {
        let player = Vec3::new(0.0, 1.75, 0.0);
        assert!(within_kick_range(player, Vec3::new(1.0, 1.6, 0.0)), "at the feet");
        assert!(!within_kick_range(player, Vec3::new(8.0, 1.6, 0.0)), "across the pitch");
        assert!(
            !within_kick_range(player, Vec3::new(0.2, 9.0, 0.0)),
            "a ball high overhead cannot be kicked"
        );
    }
}
