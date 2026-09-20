use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use crate::game::{
    GameState, Team, MAX_EPISODE_STEPS, NUM_AGENTS, OBSERVATION_SIZE, ACTION_SIZE,
    ACTION_REPEAT, RESET_POS_JITTER, RESET_BALL_JITTER,
};
use crate::entities::{Ball, CubePlayer, get_spawn_position, get_ball_spawn_position};
use crate::input::AIActions;
use crate::systems::possession::Possession;
use crate::systems::heuristic_ai::{TeamTactics, TacticParams, Tactic};
use crate::systems::superpowers::{Superpower, SuperpowerKind};
use crate::systems::status_effects::StatusEffects;
use crate::rl::reward::RewardCalculator;
use crate::rl::sim::{build_headless_app, EpisodeDone, LatestObs, LatestRewards};

#[derive(Clone)]
pub struct EnvConfig {
    pub headless: bool,
    pub render_mode: Option<String>,
    pub max_episode_steps: u32,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self { headless: true, render_mode: None, max_episode_steps: MAX_EPISODE_STEPS }
    }
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub observations: Vec<f32>,
    pub rewards: Vec<f32>,
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

/// RL environment backed by a headless Bevy simulation.
pub struct CubeSoccerEnv {
    app: App,
    config: EnvConfig,
    current_step: u32,
    initialized: bool,
}

impl CubeSoccerEnv {
    pub fn new(config: EnvConfig) -> Self {
        Self { app: build_headless_app(), config, current_step: 0, initialized: false }
    }

    pub fn reset(&mut self, seed: Option<u64>) -> Vec<f32> {
        if !self.initialized {
            self.app.update();
            self.initialized = true;
        }
        let mut rng = StdRng::seed_from_u64(seed.unwrap_or(0));

        {
            let world = &mut self.app.world;
            world.resource_mut::<GameState>().reset();
            {
                let mut poss = world.resource_mut::<Possession>();
                poss.holder = None;
                poss.cooldowns.clear();
                poss.steal_progress.clear();
            }
            world.resource_mut::<EpisodeDone>().0 = false;
            world.resource_mut::<RewardCalculator>().reset();
            *world.resource_mut::<AIActions>() = AIActions::from_slice(&[]);
        }

        {
            let world = &mut self.app.world;
            let mut state = world.query_filtered::<(&mut Transform, &mut Velocity, &CubePlayer), Without<Ball>>();
            for (mut t, mut v, player) in state.iter_mut(world) {
                let base = get_spawn_position(player.team, player.index);
                let jx = rng.gen_range(-RESET_POS_JITTER..=RESET_POS_JITTER);
                let jz = rng.gen_range(-RESET_POS_JITTER..=RESET_POS_JITTER);
                t.translation = base + Vec3::new(jx, 0.0, jz);
                v.linvel = Vec3::ZERO;
                v.angvel = Vec3::ZERO;
            }
        }

        {
            let world = &mut self.app.world;
            let mut state = world.query_filtered::<(&mut Transform, &mut Velocity), With<Ball>>();
            for (mut t, mut v) in state.iter_mut(world) {
                let ox = rng.gen_range(-RESET_BALL_JITTER..=RESET_BALL_JITTER);
                let oz = rng.gen_range(-RESET_BALL_JITTER..=RESET_BALL_JITTER);
                t.translation = get_ball_spawn_position() + Vec3::new(ox, 0.0, oz);
                v.linvel = Vec3::ZERO;
                v.angvel = Vec3::ZERO;
            }
        }

        // Randomize each cube's superpower loadout (both teams) from the seeded RNG,
        // in a stable (team, index) order so a given seed reproduces the loadout.
        {
            let world = &mut self.app.world;
            let mut cubes: Vec<(usize, usize, Entity)> = world
                .query::<(Entity, &CubePlayer)>()
                .iter(world)
                .map(|(e, p)| (p.team as usize, p.index, e))
                .collect();
            cubes.sort_by_key(|(team, index, _)| (*team, *index));
            const KINDS: [SuperpowerKind; 4] = [
                SuperpowerKind::BeamBlast,
                SuperpowerKind::FreezeRay,
                SuperpowerKind::Boost,
                SuperpowerKind::Slow,
            ];
            for (_, _, e) in cubes {
                let r = rng.gen_range(0..5);
                let mut em = world.entity_mut(e);
                // Clear any residual status effects from the previous episode so
                // resets are clean/reproducible (freezes/boosts don't leak across).
                em.insert(StatusEffects::default());
                if r == 0 {
                    em.remove::<Superpower>();
                } else {
                    em.insert(Superpower::new(KINDS[r - 1]));
                }
            }
        }

        self.current_step = 0;
        self.app.update();
        self.app.world.resource::<LatestObs>().0.clone()
    }

    pub fn step(&mut self, actions: &[f32]) -> StepResult {
        *self.app.world.resource_mut::<AIActions>() = AIActions::from_slice(actions);

        let mut reward_accum = vec![0.0f32; NUM_AGENTS];
        let mut terminated = false;
        for _ in 0..ACTION_REPEAT {
            self.app.update();
            {
                let rewards = &self.app.world.resource::<LatestRewards>().0;
                for (acc, r) in reward_accum.iter_mut().zip(rewards.iter()) {
                    *acc += *r;
                }
            }
            if self.app.world.resource::<EpisodeDone>().0 {
                terminated = true;
                break;
            }
        }

        self.current_step += 1;
        let truncated = self.current_step >= self.config.max_episode_steps;

        let observations = self.app.world.resource::<LatestObs>().0.clone();
        let (score, time_remaining, winner) = {
            let gs = self.app.world.resource::<GameState>();
            (gs.score, gs.time_remaining, gs.winner)
        };

        StepResult {
            observations,
            rewards: reward_accum,
            done: terminated,
            truncated,
            info: StepInfo { score, time_remaining, winner },
        }
    }

    pub fn render(&mut self) {}

    /// Set a team's whole-team tactic from a named preset.
    pub fn set_team_preset(&mut self, team: Team, tactic: Tactic) {
        self.set_team_params(team, tactic.params());
    }

    /// Set a team's whole-team (base) tactic params directly.
    pub fn set_team_params(&mut self, team: Team, p: TacticParams) {
        let mut tt = self.app.world.resource_mut::<TeamTactics>();
        match team {
            Team::Orange => tt.orange.set_base(p),
            Team::Blue => tt.blue.set_base(p),
        }
    }

    /// Set a team's base tactic to a weighted blend of param sets.
    pub fn set_team_blend(&mut self, team: Team, parts: &[(TacticParams, f32)]) {
        self.set_team_params(team, TacticParams::blend(parts));
    }

    /// Override a single player's (by `index`) tactic params.
    pub fn set_player_params(&mut self, team: Team, index: usize, p: TacticParams) {
        let mut tt = self.app.world.resource_mut::<TeamTactics>();
        match team {
            Team::Orange => tt.orange.set_player(index, p),
            Team::Blue => tt.blue.set_player(index, p),
        }
    }

    /// Remove all per-player overrides for a team (back to the base).
    pub fn clear_player_overrides(&mut self, team: Team) {
        let mut tt = self.app.world.resource_mut::<TeamTactics>();
        match team {
            Team::Orange => tt.orange.clear_overrides(),
            Team::Blue => tt.blue.clear_overrides(),
        }
    }

    /// Set the dense-shaping weight (1.0 = full shaping, 0.0 = pure goal objective).
    /// Driven by the training loop to anneal shaping over the run.
    pub fn set_shaping_weight(&mut self, w: f32) {
        self.app.world.resource_mut::<RewardCalculator>().shaping_weight = w;
    }

    pub fn get_observation_space(&self) -> (Vec<f32>, Vec<f32>, Vec<usize>) {
        let n = NUM_AGENTS * OBSERVATION_SIZE;
        (vec![f32::NEG_INFINITY; n], vec![f32::INFINITY; n], vec![NUM_AGENTS, OBSERVATION_SIZE])
    }

    pub fn get_action_space(&self) -> (Vec<f32>, Vec<f32>, Vec<usize>) {
        let n = NUM_AGENTS * ACTION_SIZE;
        (vec![-1.0; n], vec![1.0; n], vec![NUM_AGENTS, ACTION_SIZE])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{NUM_AGENTS, OBSERVATION_SIZE};
    use crate::rl::TOTAL_ACTION_SIZE;

    #[test]
    fn set_team_preset_mutates_resource() {
        use crate::systems::heuristic_ai::{TeamTactics, Tactic};
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        env.reset(Some(0));
        env.set_team_preset(Team::Blue, Tactic::LowBlock);
        let tt = env.app.world.resource::<TeamTactics>();
        assert_eq!(tt.blue.params_for(0), Tactic::LowBlock.params());
        assert_eq!(tt.orange.params_for(0), Tactic::Balanced.params(), "orange untouched");
    }

    #[test]
    fn set_player_params_and_clear() {
        use crate::systems::heuristic_ai::{TeamTactics, TacticParams, Tactic};
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        env.reset(Some(0));
        let p = TacticParams { defender_depth: 0.9, attacker_push: 0.1, width: 0.5, spacing: 0.7, press: 0.3, line_height: -0.1, commitment: 0.5 };
        env.set_player_params(Team::Orange, 3, p);
        assert_eq!(env.app.world.resource::<TeamTactics>().orange.params_for(3), p);
        env.clear_player_overrides(Team::Orange);
        assert_eq!(env.app.world.resource::<TeamTactics>().orange.params_for(3), Tactic::Balanced.params());
    }

    #[test]
    fn set_team_blend_averages() {
        use crate::systems::heuristic_ai::{TeamTactics, Tactic};
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        env.reset(Some(0));
        env.set_team_blend(Team::Blue, &[(Tactic::Balanced.params(), 1.0), (Tactic::HighPress.params(), 1.0)]);
        let got = env.app.world.resource::<TeamTactics>().blue.params_for(0);
        let bal = Tactic::Balanced.params();
        let hp = Tactic::HighPress.params();
        assert!((got.attacker_push - (bal.attacker_push + hp.attacker_push) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn tactics_persist_across_reset() {
        // A coach's directive must survive an episode reset (spec: reset does NOT
        // touch TeamTactics).
        use crate::systems::heuristic_ai::{TeamTactics, Tactic};
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        env.reset(Some(0));
        env.set_team_preset(Team::Blue, Tactic::HighPress);
        env.reset(Some(1));
        assert_eq!(
            env.app.world.resource::<TeamTactics>().blue.params_for(0),
            Tactic::HighPress.params(),
            "tactic must persist across reset()"
        );
    }

    #[test]
    fn reset_returns_real_observation() {
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        let obs = env.reset(Some(1));
        assert_eq!(obs.len(), NUM_AGENTS * OBSERVATION_SIZE);
        assert!(obs.iter().all(|x| x.is_finite()));
        assert!(obs.iter().any(|x| *x != 0.0), "real sim obs should not be all zero");
    }

    #[test]
    fn step_changes_observation() {
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        let obs0 = env.reset(Some(1));
        let mut actions = vec![0.0f32; TOTAL_ACTION_SIZE];
        for a in 0..NUM_AGENTS {
            actions[a * ACTION_SIZE] = 1.0;
        }
        let result = env.step(&actions);
        assert_eq!(result.observations.len(), NUM_AGENTS * OBSERVATION_SIZE);
        assert_eq!(result.rewards.len(), NUM_AGENTS);
        assert!(result.observations != obs0, "observation should change after a step");
    }

    #[test]
    fn same_seed_is_deterministic() {
        let mut a = CubeSoccerEnv::new(EnvConfig::default());
        let mut b = CubeSoccerEnv::new(EnvConfig::default());
        assert_eq!(a.reset(Some(42)), b.reset(Some(42)));
    }

    #[test]
    fn reset_reassigns_loadout_and_starts_clean() {
        // After playing (powers fire, effects/cooldowns accumulate), a reset must
        // re-assign the loadout and not carry unbounded residual state. (Strict
        // byte-reproducibility on the same env is NOT guaranteed — the heuristic's
        // sticky-handler Local and Rapier's solver state persist — and isn't needed;
        // fresh-env determinism + reproducible loadout are the real guarantees.)
        use crate::systems::superpowers::Superpower;
        use crate::entities::CubePlayer;
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        env.reset(Some(0));
        let zero = vec![0.0f32; NUM_AGENTS * crate::game::ACTION_SIZE];
        for _ in 0..40 {
            let _ = env.step(&zero);
        }
        env.reset(Some(3));
        // Loadout was (re)assigned this episode: at least one cube has a power.
        let world = &mut env.app.world;
        let has_power = world
            .query::<(&CubePlayer, Option<&Superpower>)>()
            .iter(world)
            .any(|(_, sp)| sp.is_some());
        assert!(has_power, "reset should assign a fresh loadout");
    }

    #[test]
    fn reset_seed_gives_reproducible_loadout() {
        use crate::systems::superpowers::{Superpower, SuperpowerKind};
        use crate::entities::CubePlayer;

        fn loadout(env: &mut CubeSoccerEnv) -> Vec<(usize, usize, Option<SuperpowerKind>)> {
            let world = &mut env.app.world;
            let mut v: Vec<(usize, usize, Option<SuperpowerKind>)> = world
                .query::<(&CubePlayer, Option<&Superpower>)>()
                .iter(world)
                .map(|(p, sp)| (p.team as usize, p.index, sp.map(|s| s.kind)))
                .collect();
            v.sort_by_key(|(t, i, _)| (*t, *i));
            v
        }

        let mut a = CubeSoccerEnv::new(EnvConfig::default());
        let mut b = CubeSoccerEnv::new(EnvConfig::default());
        a.reset(Some(7));
        b.reset(Some(7));
        let la = loadout(&mut a);
        let lb = loadout(&mut b);
        assert_eq!(la, lb, "same seed must give the same loadout");
        assert!(la.iter().any(|(_, _, k)| k.is_some()), "expected some powers assigned for seed 7");
    }

    #[test]
    fn set_shaping_weight_updates_calculator() {
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        env.reset(Some(0));
        env.set_shaping_weight(0.0);
        assert_eq!(env.app.world.resource::<RewardCalculator>().shaping_weight, 0.0);
        env.set_shaping_weight(0.5);
        assert_eq!(env.app.world.resource::<RewardCalculator>().shaping_weight, 0.5);
    }

    #[test]
    fn truncates_at_max_steps() {
        let config = EnvConfig { max_episode_steps: 3, ..Default::default() };
        let mut env = CubeSoccerEnv::new(config);
        env.reset(Some(1));
        let zero = vec![0.0f32; TOTAL_ACTION_SIZE];
        let _ = env.step(&zero);
        let _ = env.step(&zero);
        let r = env.step(&zero);
        assert!(r.truncated, "should truncate at max_episode_steps");
    }
}
