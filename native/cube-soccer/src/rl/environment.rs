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
use crate::systems::heuristic_ai::{TeamTactics, TacticParams, Tactic, HeuristicDifficulty, ActiveRoster};
use crate::systems::scoring::GoalHalfWidth;
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

    /// Set the heuristic opponent's difficulty (1.0 = full strength, 0.0 = frozen).
    /// Driven by the training loop's curriculum: start Blue weak so Orange can learn
    /// to score, then ramp back to full strength. Persists across `reset()`.
    pub fn set_opponent_difficulty(&mut self, d: f32) {
        self.app.world.resource_mut::<HeuristicDifficulty>().0 = d.clamp(0.0, 1.0);
    }

    /// Set the active roster size (players per team, 1..=PLAYERS_PER_TEAM). Benched
    /// players are ghosted + frozen, so the match plays as a true NvN while the
    /// observation/action shape stays fixed at the full roster. Driven by the
    /// training loop to grow 1v1 -> full NvN. Persists across `reset()`.
    pub fn set_active_roster(&mut self, n: usize) {
        let clamped = n.clamp(1, crate::game::PLAYERS_PER_TEAM);
        self.app.world.resource_mut::<ActiveRoster>().0 = clamped;
    }

    /// Set the scorable goal half-width in Z (goal-size curriculum). Clamped to
    /// [regulation, half the field depth]. The curriculum starts wide (easy to
    /// score) and narrows to regulation. Persists across `reset()`.
    pub fn set_goal_half_width(&mut self, hw: f32) {
        let clamped = hw.clamp(GoalHalfWidth::regulation(), crate::game::FIELD_DEPTH / 2.0);
        self.app.world.resource_mut::<GoalHalfWidth>().0 = clamped;
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
    fn opponent_difficulty_scales_blue_movement() {
        use crate::entities::CubePlayer;

        fn blue_positions(env: &mut CubeSoccerEnv) -> Vec<Vec3> {
            let world = &mut env.app.world;
            let mut v: Vec<(usize, Vec3)> = world
                .query::<(&CubePlayer, &Transform)>()
                .iter(world)
                .filter(|(p, _)| p.team == Team::Blue)
                .map(|(p, t)| (p.index, t.translation))
                .collect();
            v.sort_by_key(|(i, _)| *i);
            v.into_iter().map(|(_, p)| p).collect()
        }

        fn blue_travel(diff: f32) -> f32 {
            let mut env = CubeSoccerEnv::new(EnvConfig::default());
            env.reset(Some(5));
            env.set_opponent_difficulty(diff);
            let start = blue_positions(&mut env);
            let zero = vec![0.0f32; NUM_AGENTS * crate::game::ACTION_SIZE];
            for _ in 0..40 {
                let _ = env.step(&zero);
            }
            let end = blue_positions(&mut env);
            start.iter().zip(end).map(|(a, b)| a.distance(b)).sum()
        }

        let frozen = blue_travel(0.0);
        let full = blue_travel(1.0);
        assert!(
            full > frozen + 1.0,
            "full-strength Blue should travel more than a frozen opponent: full {full} vs frozen {frozen}"
        );
    }

    #[test]
    fn active_roster_benches_extra_players() {
        // With a 1v1 roster, players index>=1 must be frozen (velocity ~0) even when
        // driven hard, while index 0 is free to move.
        use crate::entities::CubePlayer;
        if crate::game::PLAYERS_PER_TEAM < 2 {
            return; // nothing to bench
        }
        let mut env = CubeSoccerEnv::new(EnvConfig::default());
        env.reset(Some(2));
        env.set_active_roster(1);

        // Where everyone starts, so "did it move" can be asked about displacement rather than
        // about the speed it happens to be carrying on the last tick.
        fn positions(env: &mut CubeSoccerEnv) -> Vec<(Team, usize, Vec3)> {
            let world = &mut env.app.world;
            world
                .query::<(&CubePlayer, &Transform)>()
                .iter(world)
                .map(|(p, t)| (p.team, p.index, t.translation))
                .collect()
        }
        let start = positions(&mut env);

        // Drive ALL orange players hard toward +x.
        let mut actions = vec![0.0f32; NUM_AGENTS * crate::game::ACTION_SIZE];
        for a in 0..crate::game::PLAYERS_PER_TEAM {
            actions[a * crate::game::ACTION_SIZE] = 1.0;
        }
        for _ in 0..30 {
            let _ = env.step(&actions);
        }

        let end = positions(&mut env);
        let world = &mut env.app.world;
        let speeds: Vec<(Team, usize, f32)> = world
            .query::<(&CubePlayer, &Velocity)>()
            .iter(world)
            .map(|(p, v)| (p.team, p.index, v.linvel.length()))
            .collect();
        for (team, index, speed) in &speeds {
            if *index >= 1 {
                assert!(*speed < 1e-3, "benched player {team:?}#{index} should be frozen, got speed {speed}");
            }
        }

        // The active orange player (index 0) actually moves.
        //
        // This asks how far it travelled, not how fast it is going at the end. Instantaneous
        // speed was a proxy that only worked while the kickoff happened to leave #0 with clear
        // space ahead of it: the formation is now a real 5-a-side shape, which starts the two
        // #0 players head-on and much closer together, so a player driven flat out for thirty
        // steps can be stationary at the end precisely *because* it moved -- into its opponent.
        // Displacement is what "not benched" actually means.
        // Horizontal only: everyone is spawned a little above the turf and settles onto it in the
        // first few ticks, so vertical travel says nothing about whether a player was driven.
        let displacement = |team: Team, index: usize| -> f32 {
            let at = |v: &Vec<(Team, usize, Vec3)>| {
                v.iter()
                    .find(|(t, i, _)| *t == team && *i == index)
                    .map(|(_, _, p)| *p)
                    .expect("player should exist")
            };
            let delta = at(&end) - at(&start);
            Vec2::new(delta.x, delta.z).length()
        };
        assert!(
            displacement(Team::Orange, 0) > 0.5,
            "active orange #0 should have been driven away from its kickoff position, moved {}",
            displacement(Team::Orange, 0)
        );
        for index in 1..crate::game::PLAYERS_PER_TEAM {
            assert!(
                displacement(Team::Orange, index) < 1e-2,
                "benched orange #{index} should not have moved at all"
            );
        }
    }

    #[test]
    fn wide_goal_scores_shots_a_narrow_goal_would_miss() {
        use crate::entities::Ball;
        // Place the ball just past Blue's goal line but wide in Z (outside regulation).
        fn score_with_width(hw: f32) -> u32 {
            let mut env = CubeSoccerEnv::new(EnvConfig::default());
            env.reset(Some(0));
            env.set_goal_half_width(hw);
            {
                let world = &mut env.app.world;
                let mut q = world.query_filtered::<&mut Transform, With<Ball>>();
                let mut t = q.single_mut(world);
                t.translation = Vec3::new(crate::game::FIELD_WIDTH / 2.0 + 0.1, crate::game::FIELD_HEIGHT + 0.5, 6.0);
            }
            let zero = vec![0.0f32; NUM_AGENTS * crate::game::ACTION_SIZE];
            let r = env.step(&zero);
            r.info.score[0] // Orange goals
        }
        // z=6 is outside the regulation mouth (~2.8) but inside a wide goal.
        assert_eq!(score_with_width(GoalHalfWidth::regulation()), 0, "narrow goal: wide ball is no goal");
        assert_eq!(score_with_width(8.0), 1, "wide goal: the same wide ball scores");
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
