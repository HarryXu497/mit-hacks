use bevy::prelude::*;

use crate::entities::{CubePlayer, PlayerInput};
use crate::game::{GameState, Team, MAX_EPISODE_STEPS, GOALS_TO_WIN};
use crate::input::AIActions;

use super::reward::RewardCalculator;

#[derive(Clone)]
pub struct EnvConfig {
    pub headless: bool,
    pub render_mode: Option<String>,
    pub max_episode_steps: u32,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            headless: true,
            render_mode: None,
            max_episode_steps: MAX_EPISODE_STEPS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub observations: [f32; 44],
    pub rewards: [f32; 2],
    pub done: bool,
    pub truncated: bool,
    pub info: StepInfo,
}

#[derive(Debug, Clone, Default)]
pub struct StepInfo {
    pub score: [u32; 2],
    pub time_remaining: f32,
    pub winner: Option<Team>,
}

/// Simplified RL environment
/// Note: Full integration with Bevy requires more careful handling
/// This is a placeholder for the full implementation
pub struct CubeSoccerEnv {
    config: EnvConfig,
    current_step: u32,
    episode_reward: [f32; 2],
    reward_calc: RewardCalculator,
    game_state: GameState,
    initialized: bool,
}

impl CubeSoccerEnv {
    pub fn new(config: EnvConfig) -> Self {
        Self {
            config,
            current_step: 0,
            episode_reward: [0.0, 0.0],
            reward_calc: RewardCalculator::default(),
            game_state: GameState::default(),
            initialized: false,
        }
    }

    pub fn reset(&mut self, _seed: Option<u64>) -> [f32; 44] {
        self.game_state.reset();
        self.current_step = 0;
        self.episode_reward = [0.0, 0.0];
        self.reward_calc.reset();
        self.initialized = true;

        // Return zero observations for now
        [0.0; 44]
    }

    pub fn step(&mut self, _actions: &[f32; 8]) -> StepResult {
        self.current_step += 1;

        let done = self.game_state.score[0] >= GOALS_TO_WIN
            || self.game_state.score[1] >= GOALS_TO_WIN
            || self.game_state.time_remaining <= 0.0;

        let truncated = self.current_step >= self.config.max_episode_steps;

        StepResult {
            observations: [0.0; 44],
            rewards: [0.0, 0.0],
            done,
            truncated,
            info: StepInfo {
                score: self.game_state.score,
                time_remaining: self.game_state.time_remaining,
                winner: self.game_state.winner,
            },
        }
    }

    pub fn render(&mut self) {
        // No-op for now
    }

    pub fn get_observation_space(&self) -> (Vec<f32>, Vec<f32>, Vec<usize>) {
        let low = vec![f32::NEG_INFINITY; 44];
        let high = vec![f32::INFINITY; 44];
        let shape = vec![44];
        (low, high, shape)
    }

    pub fn get_action_space(&self) -> (Vec<f32>, Vec<f32>, Vec<usize>) {
        let low = vec![-1.0; 8];
        let high = vec![1.0; 8];
        let shape = vec![8];
        (low, high, shape)
    }
}

#[allow(dead_code)]
fn apply_ai_actions(
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
