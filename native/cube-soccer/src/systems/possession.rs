use bevy::prelude::*;
use std::collections::HashMap;
use crate::game::{
    Team, TOUCH_RANGE, CONTROL_RADIUS,
};
use crate::entities::{Ball, CubePlayer};
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

    // Loose-ball hysteresis: if the current holder is farther than CONTROL_RADIUS
    // from the ball (the ball escaped), release possession before deciding.
    let current = current.filter(|id| {
        players
            .iter()
            .find(|p| p.id == *id)
            .map_or(false, |p| p.pos.distance(ball_pos) <= CONTROL_RADIUS)
    });

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
    fn holder_released_when_ball_rolls_beyond_control_radius() {
        use crate::game::CONTROL_RADIUS;
        // Ball escaped: the holder is now farther than CONTROL_RADIUS from it, and
        // nobody is within TOUCH_RANGE -> loose ball (no holder).
        let players = vec![ p(Team::Orange, 1, CONTROL_RADIUS + 1.0, 0.0, false, 0.0) ];
        let d = resolve_holder(&players, Vec3::ZERO, Some(1));
        assert_eq!(d, HolderDecision { holder: None, stole_from: None });
    }

    #[test]
    fn holder_kept_while_ball_within_control_radius() {
        use crate::game::{CONTROL_RADIUS, TOUCH_RANGE};
        // Holder outside TOUCH_RANGE but inside CONTROL_RADIUS keeps possession
        // (hysteresis band).
        let dist = (TOUCH_RANGE + CONTROL_RADIUS) / 2.0;
        let players = vec![ p(Team::Orange, 1, dist, 0.0, false, 0.0) ];
        let d = resolve_holder(&players, Vec3::ZERO, Some(1));
        assert_eq!(d, HolderDecision { holder: Some(1), stole_from: None });
    }

}
