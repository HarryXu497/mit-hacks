use bevy::prelude::*;
use crate::entities::{CubePlayer, PlayerInput};
use crate::game::Team;

pub fn keyboard_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut PlayerInput, &CubePlayer, Option<&crate::entities::roster::FormationSlot>)>,
) {
    for (mut input, player, slot) in query.iter_mut() {
        if slot.is_some_and(|slot| slot.0 != 0) { continue; }
        input.movement = Vec2::ZERO;
        input.jump = false;

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

        // Normalize diagonal movement
        input.movement = input.movement.normalize_or_zero();
    }
}
