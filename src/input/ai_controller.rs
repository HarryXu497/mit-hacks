use bevy::prelude::*;
use crate::entities::{CubePlayer, PlayerInput};
use crate::game::Team;

/// Structure for AI actions (4 continuous values per player)
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerAction {
    pub move_x: f32,    // [-1, 1] Left/Right
    pub move_z: f32,    // [-1, 1] Forward/Backward
    pub jump: f32,      // [-1, 1] > 0.5 = jump
    pub _unused: f32,   // Reserved for extension (dash, etc.)
}

impl PlayerAction {
    pub fn from_slice(slice: &[f32]) -> Self {
        Self {
            move_x: slice.get(0).copied().unwrap_or(0.0).clamp(-1.0, 1.0),
            move_z: slice.get(1).copied().unwrap_or(0.0).clamp(-1.0, 1.0),
            jump: slice.get(2).copied().unwrap_or(0.0).clamp(-1.0, 1.0),
            _unused: slice.get(3).copied().unwrap_or(0.0),
        }
    }

    pub fn to_array(&self) -> [f32; 4] {
        [self.move_x, self.move_z, self.jump, self._unused]
    }
}

/// Resource to hold AI actions for both players
#[derive(Resource, Default)]
pub struct AIActions {
    pub orange: PlayerAction,
    pub blue: PlayerAction,
}

impl AIActions {
    pub fn from_array(actions: &[f32; 8]) -> Self {
        Self {
            orange: PlayerAction::from_slice(&actions[0..4]),
            blue: PlayerAction::from_slice(&actions[4..8]),
        }
    }

    pub fn to_array(&self) -> [f32; 8] {
        let mut result = [0.0; 8];
        result[0..4].copy_from_slice(&self.orange.to_array());
        result[4..8].copy_from_slice(&self.blue.to_array());
        result
    }
}

/// System to apply AI actions to player inputs
pub fn apply_ai_actions(
    ai_actions: Res<AIActions>,
    mut query: Query<(&mut PlayerInput, &CubePlayer)>,
) {
    for (mut input, player) in query.iter_mut() {
        let action = match player.team {
            Team::Orange => &ai_actions.orange,
            Team::Blue => &ai_actions.blue,
        };

        input.movement = Vec2::new(action.move_x, action.move_z);
        input.jump = action.jump > 0.5;
    }
}
