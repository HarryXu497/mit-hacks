use crate::input::PlayerAction;
use crate::game::{agent_flat_index, Team, ACTION_SIZE, NUM_AGENTS};

/// Action space definition
/// Total action space: Box([-1, 1], shape=(NUM_AGENTS * ACTION_SIZE,))
/// 3 actions per agent: move_x, move_z, jump.
pub const ACTION_SIZE_PER_PLAYER: usize = ACTION_SIZE;
pub const TOTAL_ACTION_SIZE: usize = NUM_AGENTS * ACTION_SIZE;

#[derive(Debug, Clone, Default)]
pub struct GameActions {
    /// One `PlayerAction` per agent, ordered by `agent_flat_index`.
    pub actions: Vec<PlayerAction>,
}

impl GameActions {
    /// Build from a flat slice of length `TOTAL_ACTION_SIZE`. Shorter slices
    /// zero-fill trailing agents (via `PlayerAction::from_slice`).
    pub fn from_slice(flat: &[f32]) -> Self {
        let mut actions = Vec::with_capacity(NUM_AGENTS);
        for agent in 0..NUM_AGENTS {
            let start = agent * ACTION_SIZE;
            let end = (start + ACTION_SIZE).min(flat.len());
            let chunk = if start < flat.len() { &flat[start..end] } else { &[][..] };
            actions.push(PlayerAction::from_slice(chunk));
        }
        Self { actions }
    }

    pub fn to_vec(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(TOTAL_ACTION_SIZE);
        for a in &self.actions {
            out.extend_from_slice(&a.to_array());
        }
        out
    }

    /// The action for a specific `(team, index)` agent.
    pub fn for_agent(&self, team: Team, index: usize) -> PlayerAction {
        self.actions[agent_flat_index(team, index)]
    }

    /// Random actions for testing/exploration.
    pub fn random() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let flat: Vec<f32> = (0..TOTAL_ACTION_SIZE).map(|_| rng.gen_range(-1.0..=1.0)).collect();
        Self::from_slice(&flat)
    }

    /// All-zero actions (no movement).
    pub fn zero() -> Self {
        Self::from_slice(&vec![0.0; TOTAL_ACTION_SIZE])
    }
}

/// Clamp a flat action buffer to the valid range in place.
pub fn clamp_actions(actions: &mut [f32]) {
    for action in actions.iter_mut() {
        *action = action.clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Team, agent_flat_index, NUM_AGENTS};

    fn ramp() -> Vec<f32> {
        // distinct in-range values: agent k's move_x = k*0.1, move_z = k*0.1+0.01
        (0..TOTAL_ACTION_SIZE).map(|i| (i as f32) * 0.01).collect()
    }

    #[test]
    fn from_slice_populates_all_agents() {
        let ga = GameActions::from_slice(&ramp());
        assert_eq!(ga.actions.len(), NUM_AGENTS);
        assert!((ga.actions[0].move_x - 0.0).abs() < 1e-6);
        assert!((ga.actions[0].move_z - 0.01).abs() < 1e-6);
        let blue0 = agent_flat_index(Team::Blue, 0);
        let expected = (blue0 * ACTION_SIZE) as f32 * 0.01;
        assert!((ga.actions[blue0].move_x - expected).abs() < 1e-6);
    }

    #[test]
    fn for_agent_indexes_correctly() {
        let ga = GameActions::from_slice(&ramp());
        let a = ga.for_agent(Team::Blue, 0);
        let expected = (agent_flat_index(Team::Blue, 0) * ACTION_SIZE) as f32 * 0.01;
        assert!((a.move_x - expected).abs() < 1e-6);
    }
}
