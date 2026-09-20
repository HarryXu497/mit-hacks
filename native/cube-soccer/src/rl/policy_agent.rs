//! Drive one or both teams in a live Bevy match with the trained PPO policy.
//!
//! This is the seam that lets the coach's tactic actually steer the model: the
//! active [`TeamTactics`] flow straight into each agent's observation (see
//! [`get_observations`]), the policy reads them, and its actions become
//! `PlayerInput`. The same system powers the standalone self-play example and the
//! real coached match — only [`PolicyTeams`] differs.
//!
//! It fills the shared [`AIActions`] resource; schedule the existing
//! [`apply_ai_actions`](crate::input::apply_ai_actions) after it to turn those
//! actions into `PlayerInput`.

use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;
use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::entities::{Ball, CubePlayer};
use crate::game::{
    agent_flat_index, effective_goal_dist, GameState, Team, ACTION_SIZE, NUM_AGENTS,
    OBSERVATION_SIZE, PLAYERS_PER_TEAM,
};
use crate::input::AIActions;
use crate::rl::observation::get_observations;
use crate::rl::policy_net::PolicyNet;
use crate::systems::heuristic_ai::{ActiveRoster, TeamTactics};
use crate::systems::possession::Possession;
use crate::systems::superpowers::Superpower;

/// The loaded policy plus how it should act, held as a Bevy resource.
#[derive(Resource)]
pub struct PolicyController {
    pub net: PolicyNet,
    /// Sample `N(mean, exp(log_std))` when true; use the deterministic mean when
    /// false. The trained checkpoints are diffuse, so sampling looks livelier.
    pub sample: bool,
    /// Decisions per second. Training ran at 15 Hz (`ACTION_REPEAT=2` @ 30 Hz);
    /// the visual game renders faster, so we throttle and hold actions between
    /// decisions rather than re-deciding every frame.
    pub decision_hz: f32,
    accumulator: f32,
    rng: StdRng,
}

impl PolicyController {
    pub fn new(net: PolicyNet, sample: bool, decision_hz: f32) -> Self {
        Self {
            net,
            sample,
            decision_hz,
            accumulator: 0.0,
            rng: StdRng::from_entropy(),
        }
    }
}

/// Which teams the policy drives. Both, by default (self-play / two coached AIs).
/// Set one to `false` to leave that team to the heuristic AI or a human.
#[derive(Resource, Clone, Copy, Debug)]
pub struct PolicyTeams {
    pub orange: bool,
    pub blue: bool,
}

impl Default for PolicyTeams {
    fn default() -> Self {
        Self { orange: true, blue: true }
    }
}

/// A standard-normal draw via Box–Muller (no `rand_distr` dependency).
fn standard_normal(rng: &mut StdRng) -> f32 {
    let u1 = rng.gen::<f32>().max(1e-7);
    let u2 = rng.gen::<f32>();
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// Throttled system: build each policy-controlled team's 425-dim observation,
/// run the net, and write the result into [`AIActions`]. Between decisions the
/// last actions persist (held), reproducing the training-time action repeat.
///
/// Requires `Res<Time>`, `ResMut<AIActions>`, `Res<TeamTactics>`, `Res<GameState>`,
/// `Res<Possession>` and (optionally) `Res<ActiveRoster>` in the app.
pub fn apply_policy_actions(
    time: Res<Time>,
    mut ctrl: ResMut<PolicyController>,
    teams: Res<PolicyTeams>,
    mut ai_actions: ResMut<AIActions>,
    roster: Option<Res<ActiveRoster>>,
    tactics: Res<TeamTactics>,
    game_state: Res<GameState>,
    possession: Res<Possession>,
    player_query: Query<(Entity, &Transform, &Velocity, &CubePlayer, Option<&Superpower>)>,
    ball_query: Query<(&Transform, &Velocity), With<Ball>>,
) {
    ctrl.accumulator += time.delta_seconds();
    let period = 1.0 / ctrl.decision_hz.max(1.0);
    if ctrl.accumulator < period {
        return;
    }
    ctrl.accumulator = 0.0;

    let active = roster.map(|r| r.0).unwrap_or(PLAYERS_PER_TEAM);
    let goal_dist = effective_goal_dist(active);
    let Some(per_agent) =
        get_observations(&player_query, &ball_query, &game_state, &possession, &tactics, goal_dist)
    else {
        return; // players not spawned yet, or wrong agent count
    };

    let mut flat = vec![0.0f32; NUM_AGENTS * ACTION_SIZE];
    // Disjoint mutable borrows of the controller's fields.
    let PolicyController { net, sample, rng, .. } = &mut *ctrl;

    for team in [Team::Orange, Team::Blue] {
        let enabled = match team {
            Team::Orange => teams.orange,
            Team::Blue => teams.blue,
        };
        if !enabled {
            continue;
        }

        // Team observation = this team's 5 agents' per-agent obs, concatenated.
        let mut obs = Vec::with_capacity(PLAYERS_PER_TEAM * OBSERVATION_SIZE);
        for i in 0..PLAYERS_PER_TEAM {
            obs.extend_from_slice(&per_agent[agent_flat_index(team, i)]);
        }

        let action = if *sample {
            net.sample(&obs, || standard_normal(rng))
        } else {
            net.mean(&obs)
        };

        // Blue observes an X-mirrored world (team-symmetric obs), so its
        // policy-frame move_x is negated back into world space. move_z, jump and
        // fire are frame-independent.
        let flip = if team == Team::Blue { -1.0 } else { 1.0 };
        for i in 0..PLAYERS_PER_TEAM {
            let dst = agent_flat_index(team, i) * ACTION_SIZE;
            let src = i * ACTION_SIZE;
            flat[dst] = (action[src] * flip).clamp(-1.0, 1.0);
            flat[dst + 1] = action[src + 1];
            flat[dst + 2] = action[src + 2];
            flat[dst + 3] = action[src + 3];
        }
    }

    *ai_actions = AIActions::from_slice(&flat);
}
