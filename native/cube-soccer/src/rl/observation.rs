use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{Ball, CubePlayer};
use crate::game::{
    agent_flat_index, GameState, Team,
    FIELD_WIDTH, FIELD_DEPTH, ARENA_HEIGHT, CUBE_MAX_SPEED, MATCH_DURATION_SECS,
    NUM_AGENTS, OBSERVATION_SIZE, PLAYERS_PER_TEAM,
};

/// A lightweight view of one agent's physical state, used to build observations
/// without a running Bevy app (so the core logic is unit-testable).
pub struct AgentView<'a> {
    pub team: Team,
    pub index: usize,
    pub transform: &'a Transform,
    pub velocity: &'a Velocity,
    pub cooldown_ready: f32,
    pub power_onehot: [f32; 4],
}

/// Build the per-agent observation vector for a single observer.
///
/// Layout (all coordinates flipped in x for Blue, so each team sees the field
/// from its own attacking direction):
///   self pos(3), self vel(3),
///   for each teammate by ascending index: rel pos(3), rel vel(3),
///   for each opponent by ascending index: rel pos(3), rel vel(3),
///   ball rel pos(3), ball rel vel(3),
///   dist_to_own_goal, dist_to_opponent_goal,
///   score_diff, time_remaining,
///   possession flags [self_has_ball, teammate_has_ball, opponent_has_ball]
fn extract_one(
    observer: &AgentView,
    teammates: &[&AgentView],
    opponents: &[&AgentView],
    ball_transform: &Transform,
    ball_velocity: &Velocity,
    game_state: &GameState,
    poss_flags: [f32; 3],
) -> [f32; OBSERVATION_SIZE] {
    let flip = if observer.team == Team::Blue { -1.0 } else { 1.0 };
    let flip_v = Vec3::new(flip, 1.0, 1.0);

    let norm_pos = |pos: Vec3| -> [f32; 3] {
        [
            pos.x / (FIELD_WIDTH / 2.0),
            pos.y / ARENA_HEIGHT,
            pos.z / (FIELD_DEPTH / 2.0),
        ]
    };
    let norm_vel = |vel: Vec3| -> [f32; 3] {
        [vel.x / CUBE_MAX_SPEED, vel.y / CUBE_MAX_SPEED, vel.z / CUBE_MAX_SPEED]
    };

    let self_pos = observer.transform.translation;
    let mut out = [0.0f32; OBSERVATION_SIZE];
    let mut i = 0;
    let push = |out: &mut [f32; OBSERVATION_SIZE], i: &mut usize, block: [f32; 3]| {
        out[*i] = block[0];
        out[*i + 1] = block[1];
        out[*i + 2] = block[2];
        *i += 3;
    };

    // self
    push(&mut out, &mut i, norm_pos(self_pos * flip_v));
    push(&mut out, &mut i, norm_vel(observer.velocity.linvel * flip_v));

    // teammates (relative), ascending index
    for tm in teammates {
        let rel = tm.transform.translation - self_pos;
        push(&mut out, &mut i, norm_pos(rel * flip_v));
        push(&mut out, &mut i, norm_vel(tm.velocity.linvel * flip_v));
    }

    // opponents (relative), ascending index
    for op in opponents {
        let rel = op.transform.translation - self_pos;
        push(&mut out, &mut i, norm_pos(rel * flip_v));
        push(&mut out, &mut i, norm_vel(op.velocity.linvel * flip_v));
    }

    // ball (relative)
    let ball_rel = ball_transform.translation - self_pos;
    push(&mut out, &mut i, norm_pos(ball_rel * flip_v));
    push(&mut out, &mut i, norm_vel(ball_velocity.linvel * flip_v));

    // goal distances
    out[i] = (self_pos.x * flip + FIELD_WIDTH / 2.0) / FIELD_WIDTH; // own goal
    out[i + 1] = (FIELD_WIDTH / 2.0 - self_pos.x * flip) / FIELD_WIDTH; // opponent goal
    i += 2;

    // match state
    out[i] = game_state.score_diff(observer.team) as f32 / 10.0;
    out[i + 1] = game_state.time_remaining / MATCH_DURATION_SECS;
    out[i + 2] = observer.cooldown_ready;

    // own superpower one-hot [blast, freeze, boost, slow]; all-zero = none
    out[i + 3] = observer.power_onehot[0];
    out[i + 4] = observer.power_onehot[1];
    out[i + 5] = observer.power_onehot[2];
    out[i + 6] = observer.power_onehot[3];

    // possession flags [self, teammate, opponent]  (kept last-3)
    out[i + 7] = poss_flags[0];
    out[i + 8] = poss_flags[1];
    out[i + 9] = poss_flags[2];

    out
}

/// Build observations for every agent, ordered by `agent_flat_index`
/// (Orange `[0..N)` then Blue `[0..N)`). Returns `None` unless exactly
/// `NUM_AGENTS` agents are supplied.
pub fn compute_observations(
    agents: &[AgentView],
    ball_transform: &Transform,
    ball_velocity: &Velocity,
    game_state: &GameState,
    holder: Option<(Team, usize)>,
) -> Option<Vec<[f32; OBSERVATION_SIZE]>> {
    if agents.len() != NUM_AGENTS {
        return None;
    }

    let mut result: Vec<[f32; OBSERVATION_SIZE]> = vec![[0.0; OBSERVATION_SIZE]; NUM_AGENTS];

    for observer in agents {
        let mut teammates: Vec<&AgentView> = agents
            .iter()
            .filter(|a| a.team == observer.team && a.index != observer.index)
            .collect();
        teammates.sort_by_key(|a| a.index);

        let mut opponents: Vec<&AgentView> = agents
            .iter()
            .filter(|a| a.team != observer.team)
            .collect();
        opponents.sort_by_key(|a| a.index);

        debug_assert_eq!(teammates.len(), PLAYERS_PER_TEAM - 1);
        debug_assert_eq!(opponents.len(), PLAYERS_PER_TEAM);

        let poss_flags = match holder {
            Some((ht, hi)) if ht == observer.team && hi == observer.index => [1.0, 0.0, 0.0],
            Some((ht, _)) if ht == observer.team => [0.0, 1.0, 0.0],
            Some(_) => [0.0, 0.0, 1.0],
            None => [0.0, 0.0, 0.0],
        };

        let obs = extract_one(observer, &teammates, &opponents, ball_transform, ball_velocity, game_state, poss_flags);
        result[agent_flat_index(observer.team, observer.index)] = obs;
    }

    Some(result)
}

/// Bevy adapter: pull agent state from the ECS and build observations.
pub fn get_observations(
    player_query: &Query<(Entity, &Transform, &Velocity, &CubePlayer, Option<&crate::systems::superpowers::Superpower>)>,
    ball_query: &Query<(&Transform, &Velocity), With<Ball>>,
    game_state: &GameState,
    possession: &crate::systems::possession::Possession,
) -> Option<Vec<[f32; OBSERVATION_SIZE]>> {
    let agents: Vec<AgentView> = player_query
        .iter()
        .map(|(_, transform, velocity, player, power)| {
            let mut oh = [0.0f32; 4];
            if let Some(p) = power {
                oh[p.kind.onehot_index()] = 1.0;
            }
            AgentView {
                team: player.team,
                index: player.index,
                transform,
                velocity,
                cooldown_ready: power.map(|p| p.ready_fraction()).unwrap_or(1.0),
                power_onehot: oh,
            }
        })
        .collect();

    let holder = possession.holder.and_then(|held| {
        player_query
            .iter()
            .find(|(entity, _, _, _, _)| *entity == held)
            .map(|(_, _, _, player, _)| (player.team, player.index))
    });

    let (ball_transform, ball_velocity) = ball_query.get_single().ok()?;
    compute_observations(&agents, ball_transform, ball_velocity, game_state, holder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_rapier3d::prelude::Velocity;

    fn t(x: f32, y: f32, z: f32) -> Transform {
        Transform::from_xyz(x, y, z)
    }
    fn v(x: f32, y: f32, z: f32) -> Velocity {
        Velocity { linvel: Vec3::new(x, y, z), angvel: Vec3::ZERO }
    }

    #[test]
    fn each_agent_observation_has_correct_length() {
        let transforms: Vec<Transform> =
            (0..NUM_AGENTS).map(|i| t(i as f32, 1.0, 0.0)).collect();
        let vels: Vec<Velocity> = (0..NUM_AGENTS).map(|_| v(0.0, 0.0, 0.0)).collect();

        let agents: Vec<AgentView> = (0..NUM_AGENTS)
            .map(|i| {
                let team = if i < PLAYERS_PER_TEAM { Team::Orange } else { Team::Blue };
                let index = i % PLAYERS_PER_TEAM;
                AgentView { team, index, transform: &transforms[i], velocity: &vels[i], cooldown_ready: 1.0, power_onehot: [0.0; 4] }
            })
            .collect();

        let ball_t = t(0.0, 1.0, 0.0);
        let ball_v = v(0.0, 0.0, 0.0);
        let gs = crate::game::GameState::default();

        let obs = compute_observations(&agents, &ball_t, &ball_v, &gs, None).unwrap();
        assert_eq!(obs.len(), NUM_AGENTS);
        for o in &obs {
            assert_eq!(o.len(), OBSERVATION_SIZE);
        }
    }

    #[test]
    fn self_position_block_is_normalized_self_pos() {
        let transforms: Vec<Transform> = (0..NUM_AGENTS)
            .map(|i| if i == 0 { t(-FIELD_WIDTH / 2.0, ARENA_HEIGHT, 0.0) } else { t(2.0, 1.0, 0.0) })
            .collect();
        let vels: Vec<Velocity> = (0..NUM_AGENTS).map(|_| v(0.0, 0.0, 0.0)).collect();
        let agents: Vec<AgentView> = (0..NUM_AGENTS)
            .map(|i| {
                let team = if i < PLAYERS_PER_TEAM { Team::Orange } else { Team::Blue };
                AgentView { team, index: i % PLAYERS_PER_TEAM, transform: &transforms[i], velocity: &vels[i], cooldown_ready: 1.0, power_onehot: [0.0; 4] }
            })
            .collect();
        let ball_t = t(0.0, 1.0, 0.0);
        let ball_v = v(0.0, 0.0, 0.0);
        let gs = crate::game::GameState::default();

        let obs = compute_observations(&agents, &ball_t, &ball_v, &gs, None).unwrap();
        assert!((obs[0][0] - (-1.0)).abs() < 1e-5, "self x should normalize to -1.0");
        assert!((obs[0][1] - (ARENA_HEIGHT / ARENA_HEIGHT)).abs() < 1e-5);
    }

    #[test]
    fn wrong_agent_count_returns_none() {
        let tr = t(0.0, 1.0, 0.0);
        let ve = v(0.0, 0.0, 0.0);
        let agents = vec![AgentView { team: Team::Orange, index: 0, transform: &tr, velocity: &ve, cooldown_ready: 1.0, power_onehot: [0.0; 4] }];
        let ball_t = t(0.0, 1.0, 0.0);
        let ball_v = v(0.0, 0.0, 0.0);
        let gs = crate::game::GameState::default();
        if NUM_AGENTS != 1 {
            assert!(compute_observations(&agents, &ball_t, &ball_v, &gs, None).is_none());
        }
    }

    #[test]
    fn possession_flags_are_set_for_holder_team() {
        let transforms: Vec<Transform> = (0..NUM_AGENTS).map(|i| t(i as f32, 1.0, 0.0)).collect();
        let vels: Vec<Velocity> = (0..NUM_AGENTS).map(|_| v(0.0, 0.0, 0.0)).collect();
        let agents: Vec<AgentView> = (0..NUM_AGENTS)
            .map(|i| {
                let team = if i < PLAYERS_PER_TEAM { Team::Orange } else { Team::Blue };
                AgentView { team, index: i % PLAYERS_PER_TEAM, transform: &transforms[i], velocity: &vels[i], cooldown_ready: 1.0, power_onehot: [0.0; 4] }
            })
            .collect();
        let ball_t = t(0.0, 1.0, 0.0);
        let ball_v = v(0.0, 0.0, 0.0);
        let gs = crate::game::GameState::default();

        let obs = compute_observations(&agents, &ball_t, &ball_v, &gs, Some((Team::Orange, 0))).unwrap();
        let last3 = |o: &[f32; OBSERVATION_SIZE]| [o[OBSERVATION_SIZE - 3], o[OBSERVATION_SIZE - 2], o[OBSERVATION_SIZE - 1]];

        let o0 = agent_flat_index(Team::Orange, 0);
        assert_eq!(last3(&obs[o0]), [1.0, 0.0, 0.0]);
        if PLAYERS_PER_TEAM >= 2 {
            let o1 = agent_flat_index(Team::Orange, 1);
            assert_eq!(last3(&obs[o1]), [0.0, 1.0, 0.0]);
        }
        let b0 = agent_flat_index(Team::Blue, 0);
        assert_eq!(last3(&obs[b0]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn possession_flags_all_zero_when_ball_free() {
        let transforms: Vec<Transform> = (0..NUM_AGENTS).map(|i| t(i as f32, 1.0, 0.0)).collect();
        let vels: Vec<Velocity> = (0..NUM_AGENTS).map(|_| v(0.0, 0.0, 0.0)).collect();
        let agents: Vec<AgentView> = (0..NUM_AGENTS)
            .map(|i| {
                let team = if i < PLAYERS_PER_TEAM { Team::Orange } else { Team::Blue };
                AgentView { team, index: i % PLAYERS_PER_TEAM, transform: &transforms[i], velocity: &vels[i], cooldown_ready: 1.0, power_onehot: [0.0; 4] }
            })
            .collect();
        let ball_t = t(0.0, 1.0, 0.0);
        let ball_v = v(0.0, 0.0, 0.0);
        let gs = crate::game::GameState::default();

        let obs = compute_observations(&agents, &ball_t, &ball_v, &gs, None).unwrap();
        for o in &obs {
            assert_eq!([o[OBSERVATION_SIZE - 3], o[OBSERVATION_SIZE - 2], o[OBSERVATION_SIZE - 1]], [0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn cooldown_and_power_onehot_placement() {
        let transforms: Vec<Transform> = (0..NUM_AGENTS).map(|i| t(i as f32, 1.0, 0.0)).collect();
        let vels: Vec<Velocity> = (0..NUM_AGENTS).map(|_| v(0.0, 0.0, 0.0)).collect();
        let mut oh = [0.0f32; 4];
        oh[2] = 1.0; // Boost slot
        let agents: Vec<AgentView> = (0..NUM_AGENTS)
            .map(|i| AgentView {
                team: if i < PLAYERS_PER_TEAM { Team::Orange } else { Team::Blue },
                index: i % PLAYERS_PER_TEAM,
                transform: &transforms[i],
                velocity: &vels[i],
                cooldown_ready: 0.25,
                power_onehot: oh,
            })
            .collect();
        let ball_t = t(0.0, 1.0, 0.0);
        let ball_v = v(0.0, 0.0, 0.0);
        let gs = crate::game::GameState::default();
        let obs = compute_observations(&agents, &ball_t, &ball_v, &gs, None).unwrap();
        assert!((obs[0][OBSERVATION_SIZE - 8] - 0.25).abs() < 1e-6, "cooldown at size-8");
        assert_eq!(
            [obs[0][OBSERVATION_SIZE - 7], obs[0][OBSERVATION_SIZE - 6], obs[0][OBSERVATION_SIZE - 5], obs[0][OBSERVATION_SIZE - 4]],
            [0.0, 0.0, 1.0, 0.0],
            "power one-hot (Boost) at size-7..size-4"
        );
    }
}
