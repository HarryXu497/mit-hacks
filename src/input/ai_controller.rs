use bevy::prelude::*;
use crate::entities::{CubePlayer, PlayerInput};

/// Structure for AI actions (4 continuous values per player)
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerAction {
    pub move_x: f32,    // [-1, 1] Left/Right
    pub move_z: f32,    // [-1, 1] Forward/Backward
    pub jump: f32,      // [-1, 1] > 0.5 = jump
    pub fire: f32,      // [-1, 1] > 0.5 = fire superpower
}

impl PlayerAction {
    pub fn from_slice(slice: &[f32]) -> Self {
        Self {
            move_x: slice.get(0).copied().unwrap_or(0.0).clamp(-1.0, 1.0),
            move_z: slice.get(1).copied().unwrap_or(0.0).clamp(-1.0, 1.0),
            jump: slice.get(2).copied().unwrap_or(0.0).clamp(-1.0, 1.0),
            fire: slice.get(3).copied().unwrap_or(0.0).clamp(-1.0, 1.0),
        }
    }

    pub fn to_array(&self) -> [f32; 4] {
        [self.move_x, self.move_z, self.jump, self.fire]
    }
}

/// Resource holding one action per agent, ordered by `agent_flat_index`.
#[derive(Resource, Default)]
pub struct AIActions {
    pub actions: Vec<PlayerAction>,
}

impl AIActions {
    pub fn from_slice(flat: &[f32]) -> Self {
        use crate::game::{NUM_AGENTS, ACTION_SIZE};
        let mut actions = Vec::with_capacity(NUM_AGENTS);
        for agent in 0..NUM_AGENTS {
            let start = agent * ACTION_SIZE;
            let end = (start + ACTION_SIZE).min(flat.len());
            let chunk = if start < flat.len() { &flat[start..end] } else { &[][..] };
            actions.push(PlayerAction::from_slice(chunk));
        }
        Self { actions }
    }
}

/// System: apply per-agent AI actions to each player's input by `(team, index)`.
pub fn apply_ai_actions(
    ai_actions: Res<AIActions>,
    mut query: Query<(&mut PlayerInput, &CubePlayer)>,
) {
    use crate::game::agent_flat_index;
    for (mut input, player) in query.iter_mut() {
        let idx = agent_flat_index(player.team, player.index);
        if let Some(action) = ai_actions.actions.get(idx) {
            input.movement = Vec2::new(action.move_x, action.move_z);
            input.jump = action.jump > 0.5;
            input.fire = action.fire > 0.5;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_slice_reads_all_four() {
        let a = PlayerAction::from_slice(&[0.1, 0.2, 0.9, 0.8]);
        assert!((a.move_x - 0.1).abs() < 1e-6);
        assert!((a.jump - 0.9).abs() < 1e-6);
        assert!((a.fire - 0.8).abs() < 1e-6);
    }

    #[test]
    fn to_array_roundtrips_four_fields() {
        let a = PlayerAction::from_slice(&[0.3, -0.4, 0.6, -0.1]);
        assert_eq!(a.to_array(), [0.3, -0.4, 0.6, -0.1]);
    }
}
