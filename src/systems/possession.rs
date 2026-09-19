use bevy::prelude::*;
use std::collections::HashMap;
use crate::game::{
    Team, TOUCH_RANGE, DRIBBLE_OFFSET, SHOOT_THRESHOLD, SHOOT_POWER_MIN, SHOOT_POWER_MAX,
    BALL_RADIUS, BARRIER_GROUP,
};
use bevy_rapier3d::prelude::{Velocity, RapierContext, QueryFilter, CollisionGroups, Group};
use crate::entities::{Ball, CubePlayer, PlayerInput};
use crate::game::{STEAL_COOLDOWN_SECS, STEAL_CONTACT_SECS};

/// Single source of truth for who holds the ball and per-player re-grab cooldowns.
#[derive(Resource, Default)]
pub struct Possession {
    pub holder: Option<Entity>,
    pub cooldowns: HashMap<Entity, f32>,
    /// Accumulated in-range contact time for opponents contesting a held ball.
    pub steal_progress: HashMap<Entity, f32>,
}

/// Lightweight view of a player for the pure possession decision.
pub struct PlayerRef {
    pub team: Team,
    pub id: u64,        // Entity bits, so the core needs no ECS types
    pub pos: Vec3,
    pub on_cooldown: bool,
    /// Seconds this player has held steal contact on the ball (opponents only).
    pub contact_time: f32,
}

/// Result of the possession decision for one frame.
#[derive(Debug, PartialEq)]
pub struct HolderDecision {
    pub holder: Option<u64>,
    pub stole_from: Option<u64>,
}

/// Decide who holds the ball this frame.
/// - Free ball: nearest eligible player (any team) within `TOUCH_RANGE` takes it
///   instantly (loose-ball pickup is not gated on contact time).
/// - Held ball: the nearest eligible OPPONENT that has held steal contact for at
///   least `STEAL_CONTACT_SECS` steals it (reported in `stole_from`); teammates
///   never steal; otherwise the current holder keeps it.
/// - "Eligible" = within range AND not on cooldown.
/// - If the current holder id is absent from `players` (despawned), treat as free.
pub fn resolve_holder(players: &[PlayerRef], ball_pos: Vec3, current: Option<u64>) -> HolderDecision {
    let in_range = |p: &PlayerRef| p.pos.distance(ball_pos) <= TOUCH_RANGE;

    let closest = |pred: &dyn Fn(&PlayerRef) -> bool| -> Option<&PlayerRef> {
        players
            .iter()
            .filter(|p| pred(p))
            .min_by(|a, b| {
                a.pos.distance(ball_pos)
                    .partial_cmp(&b.pos.distance(ball_pos))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    };

    let current_ref = current.and_then(|id| players.iter().find(|p| p.id == id));

    match current_ref {
        Some(cur) => {
            let stealer = closest(&|p: &PlayerRef| {
                p.team != cur.team
                    && !p.on_cooldown
                    && in_range(p)
                    && p.contact_time >= STEAL_CONTACT_SECS
            });
            match stealer {
                Some(s) => HolderDecision { holder: Some(s.id), stole_from: Some(cur.id) },
                None => HolderDecision { holder: Some(cur.id), stole_from: None },
            }
        }
        None => {
            let taker = closest(&|p: &PlayerRef| !p.on_cooldown && in_range(p));
            HolderDecision { holder: taker.map(|p| p.id), stole_from: None }
        }
    }
}

/// The horizontal forward direction the cube faces, from its (yaw-only) rotation.
/// Cubes lock X/Z rotation, so `rotation * Vec3::Z` is a pure heading.
pub fn cube_facing(rotation: Quat) -> Vec3 {
    let fwd = rotation * Vec3::Z;
    let flat = Vec3::new(fwd.x, 0.0, fwd.z);
    if flat.length() < 1e-3 {
        Vec3::Z
    } else {
        flat.normalize()
    }
}

/// How far ahead of the holder to place the dribbled ball: the full
/// `max_offset`, unless a wall is closer (`wall_hit_distance`), in which case stop
/// the ball just short of the wall so it can't be teleported outside the arena.
pub fn dribble_offset(wall_hit_distance: Option<f32>, max_offset: f32, ball_radius: f32) -> f32 {
    match wall_hit_distance {
        Some(d) => (d - ball_radius).clamp(0.0, max_offset),
        None => max_offset,
    }
}

/// Carry point for the ball while dribbling: a spot ahead of the holder.
/// Zero aim returns the holder position (caller skips steering in that case).
pub fn dribble_target(holder_pos: Vec3, aim_dir: Vec3) -> Vec3 {
    let flat = Vec3::new(aim_dir.x, 0.0, aim_dir.z);
    if flat.length() < 1e-3 {
        holder_pos
    } else {
        holder_pos + flat.normalize() * DRIBBLE_OFFSET
    }
}

/// Whether a shoot strength should fire a shot this frame.
pub fn is_shooting(strength: f32) -> bool {
    strength > SHOOT_THRESHOLD
}

/// The direction a shot travels: the player's movement/aim if any, otherwise the
/// direction the cube faces (so a standing player still shoots forward, and the
/// ball never fires back into the cube).
pub fn shoot_direction(movement_aim: Vec3, facing: Vec3) -> Vec3 {
    let flat = Vec3::new(movement_aim.x, 0.0, movement_aim.z);
    if flat.length() < 1e-3 {
        facing
    } else {
        flat
    }
}

/// Map a shoot strength in [SHOOT_THRESHOLD, 1] to an impulse magnitude in
/// [SHOOT_POWER_MIN, SHOOT_POWER_MAX].
pub fn power_for(strength: f32) -> f32 {
    let t = ((strength - SHOOT_THRESHOLD) / (1.0 - SHOOT_THRESHOLD)).clamp(0.0, 1.0);
    SHOOT_POWER_MIN + t * (SHOOT_POWER_MAX - SHOOT_POWER_MIN)
}

/// Horizontal shot impulse along the aim direction, or `None` for zero aim.
pub fn shoot_impulse(aim_dir: Vec3, strength: f32) -> Option<Vec3> {
    let flat = Vec3::new(aim_dir.x, 0.0, aim_dir.z);
    if flat.length() < 1e-3 {
        None
    } else {
        Some(flat.normalize() * power_for(strength))
    }
}

/// Count down and drop expired re-grab cooldowns.
pub fn tick_cooldowns(time: Res<Time>, mut possession: ResMut<Possession>) {
    let dt = time.delta_seconds();
    possession.cooldowns.retain(|_, remaining| {
        *remaining -= dt;
        *remaining > 0.0
    });
}

/// Acquire / steal the ball based on proximity, updating `Possession`.
/// Stealing from a holder requires sustained contact (`STEAL_CONTACT_SECS`);
/// loose-ball pickup stays instant.
pub fn update_possession(
    time: Res<Time>,
    mut possession: ResMut<Possession>,
    ball_query: Query<&Transform, With<Ball>>,
    player_query: Query<(Entity, &Transform, &CubePlayer)>,
) {
    let Ok(ball_transform) = ball_query.get_single() else { return; };
    let ball_pos = ball_transform.translation;
    let dt = time.delta_seconds();

    // Team of the current holder (if any), to know who may contest for a steal.
    let holder_team = possession
        .holder
        .and_then(|h| player_query.get(h).ok().map(|(_, _, pl)| pl.team));

    // Advance the tackle timers: opponents of the holder within range accrue
    // contact time; anyone who left range (or if the ball is free) is cleared.
    if let Some(hteam) = holder_team {
        let contesting: Vec<Entity> = player_query
            .iter()
            .filter(|(entity, transform, pl)| {
                pl.team != hteam
                    && transform.translation.distance(ball_pos) <= TOUCH_RANGE
                    && !possession.cooldowns.contains_key(entity)
            })
            .map(|(entity, _, _)| entity)
            .collect();
        possession.steal_progress.retain(|e, _| contesting.contains(e));
        for e in contesting {
            *possession.steal_progress.entry(e).or_insert(0.0) += dt;
        }
    } else {
        possession.steal_progress.clear();
    }

    let refs: Vec<PlayerRef> = player_query
        .iter()
        .map(|(entity, transform, player)| PlayerRef {
            team: player.team,
            id: entity.to_bits(),
            pos: transform.translation,
            on_cooldown: possession.cooldowns.contains_key(&entity),
            contact_time: possession.steal_progress.get(&entity).copied().unwrap_or(0.0),
        })
        .collect();

    let current = possession.holder.map(|e| e.to_bits());
    let decision = resolve_holder(&refs, ball_pos, current);

    if let Some(loser_bits) = decision.stole_from {
        let loser = Entity::from_bits(loser_bits);
        possession.cooldowns.insert(loser, STEAL_COOLDOWN_SECS);
        possession.steal_progress.clear(); // fresh contest after a successful steal
    }
    possession.holder = decision.holder.map(Entity::from_bits);
}

/// Fire a shot when the holder is shooting; releases possession + cooldown.
/// Runs BEFORE `dribble_ball` so a shooting frame does not also dribble.
pub fn shoot_ball(
    mut possession: ResMut<Possession>,
    mut ball_query: Query<&mut Velocity, With<Ball>>,
    player_query: Query<(&PlayerInput, &Transform), With<CubePlayer>>,
) {
    let Some(holder) = possession.holder else { return; };
    let Ok((input, holder_transform)) = player_query.get(holder) else { return; };
    if !is_shooting(input.shoot) {
        return;
    }
    // Shoot along the movement/aim; fall back to facing when standing still so a
    // stationary tap fires forward instead of doing nothing.
    let facing = cube_facing(holder_transform.rotation);
    let aim = Vec3::new(input.movement.x, 0.0, input.movement.y);
    let dir = shoot_direction(aim, facing);
    if let Some(impulse) = shoot_impulse(dir, input.shoot) {
        if let Ok(mut vel) = ball_query.get_single_mut() {
            vel.linvel.x = impulse.x;
            vel.linvel.z = impulse.z;
        }
        possession.holder = None;
        possession.cooldowns.insert(holder, STEAL_COOLDOWN_SECS);
    }
}

/// Glue the ball to a point just in front of the holder's face each frame, so it
/// stays in front and can never get shoved behind the cube. Position-anchored
/// (not a velocity nudge) so physics can't trap the ball against the body.
pub fn dribble_ball(
    possession: Res<Possession>,
    rapier: Res<RapierContext>,
    mut ball_query: Query<(&mut Transform, &mut Velocity), (With<Ball>, Without<CubePlayer>)>,
    player_query: Query<&Transform, (With<CubePlayer>, Without<Ball>)>,
) {
    let Some(holder) = possession.holder else { return; };
    let Ok(holder_transform) = player_query.get(holder) else { return; };

    let facing = cube_facing(holder_transform.rotation);
    let origin = holder_transform.translation;

    // Don't carry the ball through a wall: cast forward against barriers only
    // (goal openings have no collider, so the ball can still be dribbled in).
    let wall_filter = QueryFilter::default()
        .exclude_sensors()
        .groups(CollisionGroups::new(Group::ALL, BARRIER_GROUP));
    let wall_hit = rapier
        .cast_ray(origin, facing, DRIBBLE_OFFSET, true, wall_filter)
        .map(|(_, toi)| toi);
    let offset = dribble_offset(wall_hit, DRIBBLE_OFFSET, BALL_RADIUS);
    let carry = origin + facing * offset;

    let Ok((mut ball_transform, mut ball_vel)) = ball_query.get_single_mut() else { return; };
    // Anchor the ball's horizontal position to the (wall-clamped) carry point;
    // leave Y to gravity so it rests/rolls at ground height. Zero horizontal
    // velocity so it stays planted rather than drifting off the anchor.
    ball_transform.translation.x = carry.x;
    ball_transform.translation.z = carry.z;
    ball_vel.linvel.x = 0.0;
    ball_vel.linvel.z = 0.0;
}

/// Clear possession + cooldowns + steal timers (called on goal / round reset).
pub fn clear_possession(mut possession: ResMut<Possession>) {
    possession.holder = None;
    possession.cooldowns.clear();
    possession.steal_progress.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    // `contact` = accumulated steal-contact seconds (only matters for opponents).
    fn p(team: Team, id: u64, x: f32, z: f32, cd: bool, contact: f32) -> PlayerRef {
        PlayerRef { team, id, pos: Vec3::new(x, 1.0, z), on_cooldown: cd, contact_time: contact }
    }

    #[test]
    fn free_ball_goes_to_nearest_in_range() {
        let players = vec![
            p(Team::Orange, 1, 0.2, 0.0, false, 0.0),
            p(Team::Blue, 2, 10.0, 0.0, false, 0.0),
        ];
        let d = resolve_holder(&players, Vec3::ZERO, None);
        assert_eq!(d, HolderDecision { holder: Some(1), stole_from: None });
    }

    #[test]
    fn opponent_steals_after_sustained_contact() {
        // Blue has held contact past the threshold → steal succeeds.
        let players = vec![
            p(Team::Orange, 1, 0.2, 0.0, false, 0.0),
            p(Team::Blue, 2, 0.3, 0.0, false, STEAL_CONTACT_SECS),
        ];
        let d = resolve_holder(&players, Vec3::ZERO, Some(1));
        assert_eq!(d, HolderDecision { holder: Some(2), stole_from: Some(1) });
    }

    #[test]
    fn opponent_in_range_but_brief_contact_does_not_steal() {
        // Blue is in range but hasn't held contact long enough → holder keeps it.
        let players = vec![
            p(Team::Orange, 1, 0.2, 0.0, false, 0.0),
            p(Team::Blue, 2, 0.3, 0.0, false, STEAL_CONTACT_SECS * 0.5),
        ];
        let d = resolve_holder(&players, Vec3::ZERO, Some(1));
        assert_eq!(d, HolderDecision { holder: Some(1), stole_from: None });
    }

    #[test]
    fn teammate_does_not_steal() {
        let players = vec![
            p(Team::Orange, 1, 0.2, 0.0, false, 0.0),
            p(Team::Orange, 3, 0.3, 0.0, false, 10.0),
        ];
        let d = resolve_holder(&players, Vec3::ZERO, Some(1));
        assert_eq!(d, HolderDecision { holder: Some(1), stole_from: None });
    }

    #[test]
    fn cooldown_blocks_acquisition() {
        let players = vec![p(Team::Orange, 1, 0.2, 0.0, true, 0.0)];
        let d = resolve_holder(&players, Vec3::ZERO, None);
        assert_eq!(d, HolderDecision { holder: None, stole_from: None });
    }

    #[test]
    fn shoot_direction_falls_back_to_facing_when_standing() {
        let facing = Vec3::new(0.0, 0.0, 1.0);
        // Moving: use the movement aim.
        let d = shoot_direction(Vec3::new(1.0, 0.0, 0.0), facing);
        assert!(d.x > 0.0 && d.z.abs() < 1e-6);
        // Standing still: fall back to facing so the shot still fires forward.
        let d2 = shoot_direction(Vec3::ZERO, facing);
        assert!((d2 - facing).length() < 1e-6);
    }

    #[test]
    fn cube_facing_matches_yaw() {
        // Facing forward (+Z) at yaw 0, and +X after a +90° yaw.
        let f0 = cube_facing(Quat::from_rotation_y(0.0));
        assert!((f0 - Vec3::Z).length() < 1e-5);
        let f90 = cube_facing(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        assert!((f90 - Vec3::X).length() < 1e-5, "got {f90:?}");
    }

    #[test]
    fn dribble_target_is_ahead_of_holder() {
        let t = dribble_target(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        assert!((t.x - DRIBBLE_OFFSET).abs() < 1e-6);
        assert!((t.z - 0.0).abs() < 1e-6);
    }

    #[test]
    fn dribble_offset_clamps_at_walls() {
        // No wall in range: full offset.
        assert_eq!(dribble_offset(None, 1.6, 0.6), 1.6);
        // Wall far beyond the offset: still full offset (clamped to max).
        assert_eq!(dribble_offset(Some(5.0), 1.6, 0.6), 1.6);
        // Wall close: stop the ball just short of it (hit distance minus radius).
        assert!((dribble_offset(Some(1.0), 1.6, 0.6) - 0.4).abs() < 1e-6);
        // Wall right on the cube: don't go negative.
        assert_eq!(dribble_offset(Some(0.3), 1.6, 0.6), 0.0);
    }

    #[test]
    fn dribble_target_zero_aim_returns_holder() {
        let holder = Vec3::new(2.0, 1.0, 3.0);
        assert_eq!(dribble_target(holder, Vec3::ZERO), holder);
    }

    #[test]
    fn power_scales_between_min_and_max() {
        assert!((power_for(1.0) - SHOOT_POWER_MAX).abs() < 1e-6);
        assert!((power_for(SHOOT_THRESHOLD) - SHOOT_POWER_MIN).abs() < 1e-6);
        assert!(power_for(0.75) > SHOOT_POWER_MIN && power_for(0.75) < SHOOT_POWER_MAX);
    }

    #[test]
    fn shoot_impulse_points_along_aim_zero_is_none() {
        let imp = shoot_impulse(Vec3::new(0.0, 0.0, 1.0), 1.0).unwrap();
        assert!(imp.z > 0.0 && imp.x.abs() < 1e-6);
        assert!((imp.length() - SHOOT_POWER_MAX).abs() < 1e-4);
        assert!(shoot_impulse(Vec3::ZERO, 1.0).is_none());
    }

    #[test]
    fn is_shooting_threshold() {
        assert!(is_shooting(0.6));
        assert!(!is_shooting(0.4));
    }
}
