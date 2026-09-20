use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::game::{
    GoalScoredEvent, Team, FIELD_WIDTH,
    REWARD_GOAL, REWARD_GOAL_AGAINST, REWARD_BALL_PROGRESS, REWARD_WIN, REWARD_LOSE,
    NEAR_GOAL_RADIUS, NEAR_GOAL_BONUS,
    CROWD_RADIUS, REWARD_TEAMMATE_CROWD,
};

#[derive(Default, Clone)]
pub struct RewardConfig {
    pub goal: f32,
    pub goal_against: f32,
    pub ball_progress: f32,
    pub win: f32,
    pub lose: f32,
}

impl RewardConfig {
    pub fn default_config() -> Self {
        Self {
            goal: REWARD_GOAL,
            goal_against: REWARD_GOAL_AGAINST,
            ball_progress: REWARD_BALL_PROGRESS,
            win: REWARD_WIN,
            lose: REWARD_LOSE,
        }
    }
}

#[derive(Resource)]
pub struct RewardCalculator {
    pub config: RewardConfig,
    pub prev_ball_pos: Vec3,
    pub shaping_weight: f32,
}

impl Default for RewardCalculator {
    fn default() -> Self {
        Self { config: RewardConfig::default_config(), prev_ball_pos: Vec3::ZERO, shaping_weight: 1.0 }
    }
}

impl RewardCalculator {
    pub fn new(config: RewardConfig) -> Self {
        Self {
            config,
            prev_ball_pos: Vec3::ZERO,
            shaping_weight: 1.0,
        }
    }

    pub fn update_state(&mut self, ball_transform: &Transform) {
        self.prev_ball_pos = ball_transform.translation;
    }

    pub fn reset(&mut self) {
        self.prev_ball_pos = Vec3::ZERO;
    }

    /// Reward shared equally by every agent on `team` this step:
    /// goal ±, win/lose, and potential-based ball-progress shaping.
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
        let prev = self.prev_ball_pos;
        let curr = ball_transform.translation;
        let prev_dist = ((prev.x - goal_x).powi(2) + prev.z.powi(2)).sqrt();
        let curr_dist = ((curr.x - goal_x).powi(2) + curr.z.powi(2)).sqrt();
        reward += self.shaping_weight * self.config.ball_progress * (prev_dist - curr_dist);

        // Finishing pull: a ramp potential that grows as the ball nears the goal mouth
        // (peaks at the net). Potential-based like the term above, so it telescopes —
        // approaching the goal is rewarded, but camping in the attacking third nets 0.
        let ramp = |d: f32| (1.0 - d / NEAR_GOAL_RADIUS).max(0.0);
        reward += self.shaping_weight * NEAR_GOAL_BONUS * (ramp(curr_dist) - ramp(prev_dist));

        reward
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
    /// `agent_reward = team_shared + crowding`.
    pub fn compute_all(
        &mut self,
        agents: &[(Team, usize, &Transform)],
        ball_transform: &Transform,
        _ball_velocity: &Velocity,
        goal_event: Option<&GoalScoredEvent>,
        game_over: bool,
        winner: Option<Team>,
        _holder: Option<(Team, usize)>,
    ) -> Vec<f32> {
        use crate::game::{agent_flat_index, NUM_AGENTS};

        let shared_orange = self.team_shared(Team::Orange, ball_transform, goal_event, game_over, winner);
        let shared_blue = self.team_shared(Team::Blue, ball_transform, goal_event, game_over, winner);

        let mut rewards = vec![0.0f32; NUM_AGENTS];
        for (team, index, transform) in agents {
            let shared = if *team == Team::Orange { shared_orange } else { shared_blue };
            let crowd = self.shaping_weight
                * Self::crowding_penalty(*team, *index, transform.translation, agents);
            rewards[agent_flat_index(*team, *index)] = shared + crowd;
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
    fn ball_progress_is_signed_and_telescopes() {
        let mut calc = RewardCalculator::default(); // shaping_weight = 1.0
        let center = tf(0.0, 1.0, 0.0);
        calc.update_state(&center); // prev = center
        let advanced = tf(5.0, 1.0, 0.0); // toward Orange's +x goal
        let o_fwd = calc.team_shared(Team::Orange, &advanced, None, false, None);
        let b_fwd = calc.team_shared(Team::Blue, &advanced, None, false, None);
        assert!(o_fwd > 0.0, "advancing toward Orange goal rewards Orange");
        assert!(b_fwd < 0.0, "same move penalizes Blue");
        calc.update_state(&advanced);
        let o_back = calc.team_shared(Team::Orange, &center, None, false, None);
        assert!((o_fwd + o_back).abs() < 1e-5, "back-and-forth nets ~0 (potential-based)");
    }

    #[test]
    fn shaping_weight_scales_dense_terms() {
        let mut calc = RewardCalculator::default();
        calc.shaping_weight = 0.0;
        let center = tf(0.0, 1.0, 0.0);
        calc.update_state(&center);
        let advanced = tf(5.0, 1.0, 0.0);
        assert_eq!(calc.team_shared(Team::Orange, &advanced, None, false, None), 0.0);
        let goal = GoalScoredEvent { scoring_team: Team::Orange };
        let r = calc.team_shared(Team::Orange, &advanced, Some(&goal), false, None);
        assert!((r - 30.0).abs() < 1e-4, "goal unaffected by shaping_weight, got {r}");
    }

    #[test]
    fn finishing_pull_rewards_approach_but_not_camping() {
        let goal_x = FIELD_WIDTH / 2.0; // Orange attacks +x
        // Ball just outside the ramp (dist > NEAR_GOAL_RADIUS) then just inside it.
        let outside = tf(goal_x - (NEAR_GOAL_RADIUS + 1.0), 1.0, 0.0);
        let inside = tf(goal_x - (NEAR_GOAL_RADIUS - 3.0), 1.0, 0.0);

        // Approaching the mouth: base progress + finishing ramp, both positive.
        let mut calc = RewardCalculator::default();
        calc.update_state(&outside);
        let approach = calc.team_shared(Team::Orange, &inside, None, false, None);

        // Same approach with the ramp disabled (bonus 0) — isolates the ramp's share.
        let base_only = REWARD_BALL_PROGRESS
            * ((goal_x - outside.translation.x) - (goal_x - inside.translation.x));
        assert!(approach > base_only + 1e-4,
            "finishing ramp adds reward on top of base progress ({approach} vs {base_only})");

        // Camping inside the zone (no movement) telescopes to ~0 — no farming.
        calc.update_state(&inside);
        let camp = calc.team_shared(Team::Orange, &inside, None, false, None);
        assert!(camp.abs() < 1e-5, "camping near the goal nets ~0, got {camp}");
    }
}
