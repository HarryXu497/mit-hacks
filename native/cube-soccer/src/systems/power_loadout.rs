//! Who walks onto the pitch holding a power.
//!
//! The powers, their effects and their art were all in place, but nothing in a native match ever
//! attached a `Superpower` to a cube - only the RL environment did. The whole feature was therefore
//! dead on the pitch: correct, wired, and impossible to trigger.
//!
//! This deals one out per player so a match actually has powers in it. It is a default, not a
//! verdict: when the forge starts handing each character the power it drew, it should overwrite
//! what is dealt here, and `CANOPY_NO_POWERS` turns this off entirely for a match that wants none.

use bevy::prelude::*;

use crate::entities::{CubePlayer, PlayerInput};
use crate::game::config::PLAYERS_PER_TEAM;
use crate::input::keyboard::is_human_controlled;
use crate::systems::superpowers::{Superpower, SuperpowerKind};

/// The four powers dealt round-robin down a team, so a five-a-side side fields one of each and
/// the fifth doubles up on the blast.
pub fn power_for_index(index: usize) -> SuperpowerKind {
    SuperpowerKind::ALL[index % SuperpowerKind::ALL.len()]
}

/// Deal a power to every cube once the roster exists. Skips anyone already holding one, so a
/// forged loadout inserted earlier wins.
pub fn deal_superpowers(
    mut commands: Commands,
    players: Query<(Entity, &CubePlayer), Without<Superpower>>,
) {
    if std::env::var("CANOPY_NO_POWERS").is_ok() {
        return;
    }
    for (entity, player) in players.iter() {
        commands
            .entity(entity)
            .insert(Superpower::new(power_for_index(player.index)));
    }
}

/// Keys 1-4 swap the keyboard-driven cube's power and fire it, so one player can show all four
/// without a substitution. `PlayerInput::fire` is a request for that frame, not a held state.
pub fn superpower_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut players: Query<(&CubePlayer, &mut PlayerInput, &mut Superpower)>,
) {
    let chosen = [
        (KeyCode::Digit1, SuperpowerKind::BeamBlast),
        (KeyCode::Digit2, SuperpowerKind::FreezeRay),
        (KeyCode::Digit3, SuperpowerKind::Boost),
        (KeyCode::Digit4, SuperpowerKind::Slow),
    ]
    .into_iter()
    .find(|(key, _)| keyboard.just_pressed(*key))
    .map(|(_, kind)| kind);

    for (player, mut input, mut power) in players.iter_mut() {
        if !is_human_controlled(player.index) {
            continue;
        }
        match chosen {
            Some(kind) => {
                power.kind = kind;
                input.fire = true;
            }
            None => input.fire = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_team_fields_every_power() {
        let dealt: Vec<SuperpowerKind> = (0..PLAYERS_PER_TEAM).map(power_for_index).collect();
        for kind in SuperpowerKind::ALL {
            assert!(dealt.contains(&kind), "nobody carries {kind:?}");
        }
    }

    #[test]
    fn dealing_wraps_past_the_fourth_slot() {
        assert_eq!(power_for_index(4), SuperpowerKind::BeamBlast);
    }
}
