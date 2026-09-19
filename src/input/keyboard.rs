use bevy::prelude::*;
use crate::entities::{CubePlayer, PlayerInput};
use crate::game::Team;

/// Only the first player on each team (index 0) is driven by the keyboard.
/// Teammates (index >= 1) are driven by the built-in heuristic AI.
pub fn is_human_controlled(index: usize) -> bool {
    index == 0
}

/// Keyboard shoot key per team (only the human, index 0, uses it).
pub fn shoot_key(team: Team) -> KeyCode {
    match team {
        Team::Orange => KeyCode::ShiftLeft,
        Team::Blue => KeyCode::ControlRight,
    }
}

pub fn keyboard_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut PlayerInput, &CubePlayer)>,
) {
    for (mut input, player) in query.iter_mut() {
        if !is_human_controlled(player.index) {
            continue;
        }
        input.movement = Vec2::ZERO;
        input.jump = false;
        input.shoot = 0.0;

        match player.team {
            Team::Orange => {
                // WASD + Space
                if keyboard.pressed(KeyCode::KeyW) { input.movement.y -= 1.0; }
                if keyboard.pressed(KeyCode::KeyS) { input.movement.y += 1.0; }
                if keyboard.pressed(KeyCode::KeyA) { input.movement.x -= 1.0; }
                if keyboard.pressed(KeyCode::KeyD) { input.movement.x += 1.0; }
                if keyboard.just_pressed(KeyCode::Space) { input.jump = true; }
            }
            Team::Blue => {
                // Arrow keys + Enter
                if keyboard.pressed(KeyCode::ArrowUp) { input.movement.y -= 1.0; }
                if keyboard.pressed(KeyCode::ArrowDown) { input.movement.y += 1.0; }
                if keyboard.pressed(KeyCode::ArrowLeft) { input.movement.x -= 1.0; }
                if keyboard.pressed(KeyCode::ArrowRight) { input.movement.x += 1.0; }
                if keyboard.just_pressed(KeyCode::Enter) { input.jump = true; }
            }
        }

        if keyboard.pressed(shoot_key(player.team)) {
            input.shoot = 1.0; // humans fire at full power
        }

        // Normalize diagonal movement
        input.movement = input.movement.normalize_or_zero();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_index_zero_is_human_controlled() {
        assert!(is_human_controlled(0));
        assert!(!is_human_controlled(1));
        assert!(!is_human_controlled(2));
    }

    #[test]
    fn shoot_key_for_team_is_defined() {
        assert_eq!(shoot_key(Team::Orange), KeyCode::ShiftLeft);
        assert_eq!(shoot_key(Team::Blue), KeyCode::ControlRight);
    }
}
