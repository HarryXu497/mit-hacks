use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::game::{
    GoalScoredEvent, Team,
    FIELD_WIDTH, CUBE_SIZE, BALL_RADIUS,
    REWARD_GOAL, REWARD_GOAL_AGAINST, REWARD_BALL_TO_GOAL,
    REWARD_TOUCH_BALL, REWARD_WIN, REWARD_LOSE,
    CROWD_RADIUS, REWARD_TEAMMATE_CROWD, REWARD_POSSESSION,
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
}

impl Default for RewardCalculator {
    fn default() -> Self {
        Self {
            config: RewardConfig::default_config(),
            prev_ball_pos: Vec3::ZERO,
        }
    }
}

impl RewardCalculator {
    pub fn new(config: RewardConfig) -> Self {
        Self {
            config,
            prev_ball_pos: Vec3::ZERO,
        }
    }

    pub fn update_state(&mut self, ball_transform: &Transform) {
        self.prev_ball_pos = ball_transform.translation;
    }

    pub fn reset(&mut self) {
        self.prev_ball_pos = Vec3::ZERO;
    }

    /// Reward shared equally by every agent on `team` this step:
    /// goal ±, win/lose, and ball-toward-opponent-goal shaping.
    pub fn team_shared(
        &self,
        team: Team,
        ball_transform: &Transform,
        goal_event: Option<&GoalScoredEvent>,
        game_over: bool,
        winner: Option<Team>,
    ) -> f32 {
        let mut reward = 0.0;

        if let Some(event) = goal_event {
            if event.scoring_team == team {
                reward += self.config.goal;
            } else {
                reward += self.config.goal_against;
            }
        }

        if game_over {
            if let Some(winner_team) = winner {
                if winner_team == team {
                    reward += self.config.win;
                } else {
                    reward += self.config.lose;
                }
            }
        }

        let goal_x = if team == Team::Orange { FIELD_WIDTH / 2.0 } else { -FIELD_WIDTH / 2.0 };
        let prev_dist = (self.prev_ball_pos.x - goal_x).abs();
        let curr_dist = (ball_transform.translation.x - goal_x).abs();
        if curr_dist < prev_dist {
            reward += self.config.ball_to_goal;
        }

        reward
    }

    /// Individual shaping: reward for being on a moving ball.
    pub fn individual_touch(
        &self,
        player_transform: &Transform,
        ball_transform: &Transform,
        ball_velocity: &Velocity,
    ) -> f32 {
        let dist = player_transform.translation.distance(ball_transform.translation);
        if dist < CUBE_SIZE / 2.0 + BALL_RADIUS + 0.5 && ball_velocity.linvel.length() > 1.0 {
            self.config.touch_ball
        } else {
            0.0
        }
    }

    /// Shared per-team bonus while a teammate holds the ball (rewards keeping
    /// possession). `holder` is the ball-holder's `(team, index)`, if any.
    pub fn possession_bonus(team: Team, holder: Option<(Team, usize)>) -> f32 {
        match holder {
            Some((t, _)) if t == team => REWARD_POSSESSION,
            _ => 0.0,
        }
    }

    /// Individual anti-clump penalty: a small negative per teammate crowding this
    /// agent (within `CROWD_RADIUS`). Pushes the team to spread into positions.
    pub fn crowding_penalty(
        team: Team,
        index: usize,
        pos: Vec3,
        agents: &[(Team, usize, &Transform)],
    ) -> f32 {
        let crowd = agents
            .iter()
            .filter(|(t, i, tr)| {
                *t == team && *i != index && tr.translation.distance(pos) < CROWD_RADIUS
            })
            .count();
        crowd as f32 * REWARD_TEAMMATE_CROWD
    }

    /// Compute a per-agent reward vector (length `NUM_AGENTS`), ordered by
    /// `agent_flat_index`.
    /// `agent_reward = team_shared + possession_bonus + individual_touch + crowding`.
    pub fn compute_all(
        &mut self,
        agents: &[(Team, usize, &Transform)],
        ball_transform: &Transform,
        ball_velocity: &Velocity,
        goal_event: Option<&GoalScoredEvent>,
        game_over: bool,
        winner: Option<Team>,
        holder: Option<(Team, usize)>,
    ) -> Vec<f32> {
        use crate::game::{agent_flat_index, NUM_AGENTS};

        let shared_orange = self.team_shared(Team::Orange, ball_transform, goal_event, game_over, winner)
            + Self::possession_bonus(Team::Orange, holder);
        let shared_blue = self.team_shared(Team::Blue, ball_transform, goal_event, game_over, winner)
            + Self::possession_bonus(Team::Blue, holder);

        let mut rewards = vec![0.0f32; NUM_AGENTS];
        for (team, index, transform) in agents {
            let shared = if *team == Team::Orange { shared_orange } else { shared_blue };
            let touch = self.individual_touch(transform, ball_transform, ball_velocity);
            let crowd = Self::crowding_penalty(*team, *index, transform.translation, agents);
            rewards[agent_flat_index(*team, *index)] = shared + touch + crowd;
        }

        self.update_state(ball_transform);
        rewards
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{agent_flat_index, GoalScoredEvent, NUM_AGENTS, PLAYERS_PER_TEAM};

    fn tf(x: f32, y: f32, z: f32) -> Transform { Transform::from_xyz(x, y, z) }

    fn roster() -> Vec<(Team, usize, Transform)> {
        let mut v = Vec::new();
        for i in 0..PLAYERS_PER_TEAM { v.push((Team::Orange, i, tf(-5.0, 1.0, 0.0))); }
        for i in 0..PLAYERS_PER_TEAM { v.push((Team::Blue, i, tf(5.0, 1.0, 0.0))); }
        v
    }

    #[test]
    fn goal_reward_is_shared_across_teammates() {
        if PLAYERS_PER_TEAM < 2 { return; }
        let mut calc = RewardCalculator::default();
        let players = roster();
        let refs: Vec<(Team, usize, &Transform)> =
            players.iter().map(|(t, i, tr)| (*t, *i, tr)).collect();
        let ball = tf(0.0, 1.0, 0.0);
        let ball_v = Velocity { linvel: Vec3::ZERO, angvel: Vec3::ZERO };
        let goal = GoalScoredEvent { scoring_team: Team::Orange };

        let rewards = calc.compute_all(&refs, &ball, &ball_v, Some(&goal), false, None, None);
        assert_eq!(rewards.len(), NUM_AGENTS);
        let o0 = rewards[agent_flat_index(Team::Orange, 0)];
        let o1 = rewards[agent_flat_index(Team::Orange, 1)];
        assert!((o0 - o1).abs() < 1e-6, "teammates share the goal reward");
        assert!(o0 > 0.0, "scoring team gets positive reward");
        let b0 = rewards[agent_flat_index(Team::Blue, 0)];
        assert!(b0 < 0.0, "conceding team gets negative reward");
    }

    #[test]
    fn touch_reward_is_individual() {
        let mut calc = RewardCalculator::default();
        let mut players = roster();
        players[agent_flat_index(Team::Orange, 0)].2 = tf(0.0, 1.0, 0.0);
        let refs: Vec<(Team, usize, &Transform)> =
            players.iter().map(|(t, i, tr)| (*t, *i, tr)).collect();
        let ball = tf(0.0, 1.0, 0.0);
        let ball_v = Velocity { linvel: Vec3::new(5.0, 0.0, 0.0), angvel: Vec3::ZERO };

        let rewards = calc.compute_all(&refs, &ball, &ball_v, None, false, None, None);
        let toucher = rewards[agent_flat_index(Team::Orange, 0)];
        if PLAYERS_PER_TEAM >= 2 {
            let other = rewards[agent_flat_index(Team::Orange, 1)];
            assert!(toucher > other, "only the touching agent gets the touch reward");
        }
    }

    #[test]
    fn crowding_penalty_scales_with_nearby_teammates() {
        let players = vec![
            (Team::Orange, 0, tf(0.0, 1.0, 0.0)),
            (Team::Orange, 1, tf(0.5, 1.0, 0.0)),   // close to 0
            (Team::Orange, 2, tf(0.6, 1.0, 0.0)),   // close to 0
            (Team::Orange, 3, tf(20.0, 1.0, 0.0)),  // far away
        ];
        let refs: Vec<(Team, usize, &Transform)> =
            players.iter().map(|(t, i, tr)| (*t, *i, tr)).collect();

        // Agent 0 has 2 teammates within CROWD_RADIUS → penalty = 2 * per.
        let p0 = RewardCalculator::crowding_penalty(Team::Orange, 0, Vec3::new(0.0, 1.0, 0.0), &refs);
        assert!((p0 - 2.0 * REWARD_TEAMMATE_CROWD).abs() < 1e-6);
        assert!(p0 < 0.0, "crowding is a penalty");

        // Agent 3 is alone → no penalty.
        let p3 = RewardCalculator::crowding_penalty(Team::Orange, 3, Vec3::new(20.0, 1.0, 0.0), &refs);
        assert_eq!(p3, 0.0);
    }

    #[test]
    fn possession_bonus_only_for_holding_team() {
        assert_eq!(
            RewardCalculator::possession_bonus(Team::Orange, Some((Team::Orange, 2))),
            REWARD_POSSESSION
        );
        assert_eq!(RewardCalculator::possession_bonus(Team::Orange, Some((Team::Blue, 0))), 0.0);
        assert_eq!(RewardCalculator::possession_bonus(Team::Orange, None), 0.0);
    }
}
