//! Five-a-side formation shape.
//!
//! The formation itself is Jeremy Liu's, authored against the jungle stadium so
//! the ten players read as two organised sides from the broadcast camera rather
//! than as a row of cubes.
//!
//! His original module also carried a `FormationSlot` component, its own
//! `spawn_rosters` system, and matching changes to every reset path. All of that
//! existed for one reason: upstream's `get_spawn_position` took only a team, so
//! five players per side spawned on top of each other and something had to carry
//! the index. main solved that properly with `CubePlayer::index`, and its resets
//! already restore indexed positions — so the scaffolding is gone and only the
//! authored shape survives. `entities::get_spawn_position` delegates here, which
//! keeps one definition of where a player stands.

use crate::game::config::*;
use bevy::prelude::Vec3;

/// Each slot's position as a fraction of the field: `(x from own goal line,
/// z from the centre line)`. Mirrored per team by [`formation_position`].
///
/// Roughly a 1-2-1-1: a deep anchor, two spread midfielders, a forward, and a
/// winger held wide.
pub const FORMATION: [(f32, f32); 5] = [
    (0.12, 0.0),
    (0.29, -0.25),
    (0.29, 0.25),
    (0.43, 0.0),
    (0.16, 0.32),
];

/// Where the player in `slot` stands at kickoff.
///
/// Slots beyond [`FORMATION`] fall back to an even spread across the width of
/// the box, so this stays correct if `PLAYERS_PER_TEAM` is ever raised past the
/// authored five instead of silently stacking players on one another.
pub fn formation_position(team: Team, slot: usize) -> Vec3 {
    let side = if team == Team::Orange { -1.0 } else { 1.0 };

    let (x, z) = if slot < FORMATION.len() {
        FORMATION[slot]
    } else {
        let span = FIELD_DEPTH / 2.0;
        let extra = (PLAYERS_PER_TEAM.max(FORMATION.len() + 1) - 1) as f32;
        let t = slot as f32 / extra; // 0.0 ..= 1.0
        (0.25, (-span / 2.0 + t * span) / FIELD_DEPTH)
    };

    Vec3::new(
        side * FIELD_WIDTH * x,
        FIELD_HEIGHT + CUBE_SIZE,
        FIELD_DEPTH * z,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_sides_mirror_each_other() {
        for slot in 0..PLAYERS_PER_TEAM {
            let orange = formation_position(Team::Orange, slot);
            let blue = formation_position(Team::Blue, slot);
            assert!(orange.x < 0.0, "orange defends -x");
            assert!(blue.x > 0.0, "blue defends +x");
            assert_eq!(orange.x, -blue.x, "slot {slot} is not mirrored");
            assert_eq!(orange.z, blue.z);
        }
    }

    #[test]
    fn no_two_players_start_inside_each_other() {
        let all: Vec<Vec3> = [Team::Orange, Team::Blue]
            .into_iter()
            .flat_map(|team| (0..PLAYERS_PER_TEAM).map(move |slot| formation_position(team, slot)))
            .collect();

        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert!(
                    a.distance(*b) > CUBE_SIZE * 2.0,
                    "{a:?} and {b:?} overlap at kickoff"
                );
            }
        }
    }

    #[test]
    fn everyone_starts_inside_the_field() {
        for team in [Team::Orange, Team::Blue] {
            for slot in 0..PLAYERS_PER_TEAM {
                let p = formation_position(team, slot);
                assert!(p.x.abs() <= FIELD_WIDTH / 2.0, "slot {slot} is off the end");
                assert!(p.z.abs() <= FIELD_DEPTH / 2.0, "slot {slot} is off the side");
            }
        }
    }
}
