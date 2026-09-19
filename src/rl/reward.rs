use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::CubePlayer;
use crate::game::{
    GoalScoredEvent, Team,
    FIELD_WIDTH, CUBE_SIZE, BALL_RADIUS,
    REWARD_GOAL, REWARD_GOAL_AGAINST, REWARD_BALL_TO_GOAL,
    REWARD_TOUCH_BALL, REWARD_WIN, REWARD_LOSE,
};

#[derive(Default, Clone)]
pub struct RewardConfig {
    pub goal: f32,
    pub goal_against: f32,
    pub ball_to_goal: f32,
    pub touch_ball: f32,
    pub win: f32,
    pub lose: f32,
}

impl RewardConfig {
    pub fn default_config() -> Self {
        Self {
            goal: REWARD_GOAL,
            goal_against: REWARD_GOAL_AGAINST,
            ball_to_goal: REWARD_BALL_TO_GOAL,
            touch_ball: REWARD_TOUCH_BALL,
            win: REWARD_WIN,
            lose: REWARD_LOSE,
        }
    }
}

#[derive(Resource)]
pub struct RewardCalculator {
    pub config: RewardConfig,
    pub prev_ball_pos: Vec3,
    pub prev_player_touched_ball: [bool; 2],
}

impl Default for RewardCalculator {
    fn default() -> Self {
        Self {
            config: RewardConfig::default_config(),
            prev_ball_pos: Vec3::ZERO,
            prev_player_touched_ball: [false, false],
        }
    }
}

impl RewardCalculator {
    pub fn new(config: RewardConfig) -> Self {
        Self {
            config,
            prev_ball_pos: Vec3::ZERO,
            prev_player_touched_ball: [false, false],
        }
    }

    pub fn compute(
        &mut self,
        team: Team,
        player_transform: &Transform,
        ball_transform: &Transform,
        ball_velocity: &Velocity,
        goal_event: Option<&GoalScoredEvent>,
        game_over: bool,
        winner: Option<Team>,
    ) -> f32 {
        let mut reward = 0.0;

        // === Goal reward ===
        if let Some(event) = goal_event {
            if event.scoring_team == team {
                reward += self.config.goal;
            } else {
                reward += self.config.goal_against;
            }
        }

        // === Win/Lose reward ===
        if game_over {
            if let Some(winner_team) = winner {
                if winner_team == team {
                    reward += self.config.win;
                } else {
                    reward += self.config.lose;
                }
            }
        }

        // === Reward shaping: ball towards opponent goal ===
        let goal_x = if team == Team::Orange {
            FIELD_WIDTH / 2.0  // Blue goal
        } else {
            -FIELD_WIDTH / 2.0  // Orange goal
        };

        let prev_dist = (self.prev_ball_pos.x - goal_x).abs();
        let curr_dist = (ball_transform.translation.x - goal_x).abs();

        if curr_dist < prev_dist {
            reward += self.config.ball_to_goal;
        }

        // === Reward for touching the ball ===
        let dist_to_ball = player_transform.translation.distance(ball_transform.translation);
        if dist_to_ball < CUBE_SIZE / 2.0 + BALL_RADIUS + 0.5 {
            if ball_velocity.linvel.length() > 1.0 {  // Ball is moving
                reward += self.config.touch_ball;
            }
        }

        reward
    }

    pub fn update_state(&mut self, ball_transform: &Transform) {
        self.prev_ball_pos = ball_transform.translation;
    }

    pub fn reset(&mut self) {
        self.prev_ball_pos = Vec3::ZERO;
        self.prev_player_touched_ball = [false, false];
    }
}

/// Compute rewards for both players
pub fn compute_rewards(
    reward_calc: &mut RewardCalculator,
    players: &[(&Transform, &CubePlayer)],
    ball_transform: &Transform,
    ball_velocity: &Velocity,
    goal_event: Option<&GoalScoredEvent>,
    game_over: bool,
    winner: Option<Team>,
) -> [f32; 2] {
    let mut rewards = [0.0f32; 2];

    for (transform, player) in players {
        let reward = reward_calc.compute(
            player.team,
            transform,
            ball_transform,
            ball_velocity,
            goal_event,
            game_over,
            winner,
        );

        match player.team {
            Team::Orange => rewards[0] = reward,
            Team::Blue => rewards[1] = reward,
        }
    }

    reward_calc.update_state(ball_transform);

    rewards
}
