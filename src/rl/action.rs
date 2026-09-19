use crate::input::PlayerAction;

/// Action space definition
/// Total action space: Box([-1, 1], shape=(8,), dtype=float32)
/// 4 actions per player:
/// - move_x: [-1, 1] Left/Right
/// - move_z: [-1, 1] Forward/Backward
/// - jump: [-1, 1] > 0.5 = jump
/// - unused: [-1, 1] Reserved

pub const ACTION_SIZE_PER_PLAYER: usize = 4;
pub const TOTAL_ACTION_SIZE: usize = 8;  // 4 * 2 players

#[derive(Debug, Clone, Default)]
pub struct GameActions {
    pub orange: PlayerAction,
    pub blue: PlayerAction,
}

impl GameActions {
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

    /// Create random actions for testing/exploration
    pub fn random() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut actions = [0.0f32; 8];
        for a in actions.iter_mut() {
            *a = rng.gen_range(-1.0..=1.0);
        }
        Self::from_array(&actions)
    }

    /// Create zero actions (no movement)
    pub fn zero() -> Self {
        Self::from_array(&[0.0; 8])
    }
}

/// Clamp action values to valid range
pub fn clamp_actions(actions: &mut [f32; 8]) {
    for action in actions.iter_mut() {
        *action = action.clamp(-1.0, 1.0);
    }
}
