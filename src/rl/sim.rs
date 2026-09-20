use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::transform::TransformPlugin;
use bevy::hierarchy::HierarchyPlugin;
use bevy::asset::AssetPlugin;
use bevy::scene::ScenePlugin;
use bevy::core::{TaskPoolPlugin, TaskPoolOptions};
use bevy::ecs::schedule::{ExecutorKind, ScheduleLabel};
use bevy_rapier3d::prelude::*;
use std::time::Duration;

use crate::game::PHYSICS_TIMESTEP;
use crate::systems::physics::configure_physics;
use crate::entities::{spawn_arena, spawn_field, spawn_goals, spawn_players, spawn_ball, Ball, CubePlayer};
use crate::input::{AIActions, apply_ai_actions};
use crate::game::{GameState, GoalScoredEvent, BallTouchedEvent};
use crate::systems::movement::{apply_player_movement, clamp_velocities};
use crate::systems::status_effects::{tick_status_effects, apply_status_forces, ImpulseEvent};
use crate::systems::superpowers::{tick_superpower_cooldowns, activate_superpowers};
use crate::systems::possession::{tick_cooldowns, update_possession, Possession};
use crate::systems::scoring::{detect_goals_by_position, GoalHalfWidth};
use crate::systems::heuristic_ai::{apply_heuristic_ai, apply_roster_gating, freeze_inactive_players, AiControlled, TeamTactics, HeuristicDifficulty, ActiveRoster};
use crate::game::Team;
use crate::rl::observation::get_observations;
use crate::rl::reward::RewardCalculator;

/// Set to true when a goal ends the current episode (headless training).
#[derive(Resource, Default)]
pub struct EpisodeDone(pub bool);

/// Headless replacement for `handle_goal_scored`: bump the score and mark the
/// episode done, WITHOUT touching MatchState or triggering a reset.
fn handle_goal_headless(
    mut goal_events: EventReader<GoalScoredEvent>,
    mut game_state: ResMut<GameState>,
    mut done: ResMut<EpisodeDone>,
) {
    for event in goal_events.read() {
        game_state.add_goal(event.scoring_team);
        done.0 = true;
    }
}

/// Latest flat per-agent observation (NUM_AGENTS * OBSERVATION_SIZE).
#[derive(Resource, Default)]
pub struct LatestObs(pub Vec<f32>);

/// Latest per-agent reward for the most recent tick (NUM_AGENTS).
#[derive(Resource, Default)]
pub struct LatestRewards(pub Vec<f32>);

/// Build the flat observation vector into `LatestObs` after the sim advances.
fn extract_observations(
    mut latest: ResMut<LatestObs>,
    player_query: Query<(Entity, &Transform, &Velocity, &CubePlayer, Option<&crate::systems::superpowers::Superpower>)>,
    ball_query: Query<(&Transform, &Velocity), With<Ball>>,
    game_state: Res<GameState>,
    possession: Res<Possession>,
) {
    if let Some(per_agent) = get_observations(&player_query, &ball_query, &game_state, &possession) {
        let mut flat = Vec::with_capacity(per_agent.len() * crate::game::OBSERVATION_SIZE);
        for obs in &per_agent {
            flat.extend_from_slice(obs);
        }
        latest.0 = flat;
    }
}

/// Compute per-agent rewards for the tick into `LatestRewards`.
fn compute_step_rewards(
    mut calc: ResMut<RewardCalculator>,
    mut latest: ResMut<LatestRewards>,
    possession: Res<Possession>,
    player_query: Query<(Entity, &Transform, &CubePlayer)>,
    ball_query: Query<(&Transform, &Velocity), With<Ball>>,
    mut goal_events: EventReader<GoalScoredEvent>,
) {
    let Ok((ball_transform, ball_velocity)) = ball_query.get_single() else { return; };
    let goal_event = goal_events.read().next();

    // Resolve the ball holder entity to (team, index) for the possession bonus.
    let holder = possession.holder.and_then(|held| {
        player_query
            .iter()
            .find(|(e, _, _)| *e == held)
            .map(|(_, _, p)| (p.team, p.index))
    });

    let agents: Vec<(crate::game::Team, usize, &Transform)> = player_query
        .iter()
        .map(|(_, t, p)| (p.team, p.index, t))
        .collect();

    latest.0 = calc.compute_all(&agents, ball_transform, ball_velocity, goal_event, false, None, holder);
}

/// Force a fixed physics timestep so each `app.update()` is one deterministic tick.
fn set_fixed_timestep(mut config: ResMut<RapierConfiguration>) {
    config.timestep_mode = TimestepMode::Fixed {
        dt: PHYSICS_TIMESTEP,
        substeps: 1,
    };
}

/// Headless-only: fewer solver iterations trades a little physics accuracy for
/// throughput. Our scene is simple enough that this is imperceptible.
fn reduce_solver_iterations(mut ctx: ResMut<RapierContext>) {
    ctx.integration_parameters.num_solver_iterations = std::num::NonZeroUsize::new(1).unwrap();
}

/// Tag the Blue team as AI-controlled so the built-in heuristic drives them
/// (the RL policy controls Orange; Blue is the fixed scripted opponent).
/// Runs in PostStartup so the spawned players exist.
fn tag_blue_as_ai(mut commands: Commands, query: Query<(Entity, &CubePlayer)>) {
    for (entity, player) in query.iter() {
        if player.team == Team::Blue {
            commands.entity(entity).insert(AiControlled);
        }
    }
}

/// Build a headless (no window) Bevy app that runs the soccer simulation and can
/// be advanced one tick at a time via `app.update()`.
pub fn build_headless_app() -> App {
    let mut app = App::new();

    // One OS thread per env: for this tiny scene the parallel scheduler's
    // task-pool coordination costs more than it saves, and one-thread-per-env
    // lets process-based vectorization (SubprocVecEnv) use cores cleanly.
    app.add_plugins(MinimalPlugins.set(TaskPoolPlugin {
            task_pool_options: TaskPoolOptions::with_num_threads(1),
        }))
        .add_plugins(TransformPlugin)
        .add_plugins(HierarchyPlugin)
        .add_plugins(AssetPlugin::default())
        .add_plugins(ScenePlugin)
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default());

    // Run the per-frame schedules single-threaded (no task-pool sync overhead).
    for label in [First.intern(), PreUpdate.intern(), Update.intern(), PostUpdate.intern(), Last.intern()] {
        app.edit_schedule(label, |schedule| {
            schedule.set_executor_kind(ExecutorKind::SingleThreaded);
        });
    }

    app.insert_resource(TimeUpdateStrategy::ManualDuration(
        Duration::from_secs_f32(PHYSICS_TIMESTEP),
    ));

    app.add_systems(
        Startup,
        (
            configure_physics,
            set_fixed_timestep,
            reduce_solver_iterations,
            spawn_arena,
            spawn_field,
            spawn_goals,
            spawn_players,
            spawn_ball,
        ),
    );

    // After spawns are flushed, mark Blue as heuristic-driven (fixed opponent).
    app.add_systems(PostStartup, tag_blue_as_ai);

    app.init_resource::<GameState>()
        .init_resource::<Possession>()
        .init_resource::<AIActions>()
        .init_resource::<EpisodeDone>()
        .init_resource::<LatestObs>()
        .init_resource::<LatestRewards>()
        .init_resource::<RewardCalculator>()
        .init_resource::<TeamTactics>()
        .init_resource::<HeuristicDifficulty>()
        .init_resource::<ActiveRoster>()
        .init_resource::<GoalHalfWidth>()
        .add_event::<GoalScoredEvent>()
        .add_event::<BallTouchedEvent>()
        .add_event::<ImpulseEvent>();

    app.add_systems(
        Update,
        (
            apply_ai_actions,      // Orange gets RL actions; Blue's slice is ignored...
            apply_heuristic_ai,    // ...then the heuristic overrides Blue's inputs.
            apply_roster_gating,     // curriculum: ghost benched players (both teams).
            freeze_inactive_players, // ...and zero their input+velocity so they're inert.
            tick_superpower_cooldowns,
            activate_superpowers,
            tick_status_effects,
            apply_player_movement,
            apply_status_forces,
            clamp_velocities,
            (tick_cooldowns, update_possession).chain(),
            detect_goals_by_position,
            handle_goal_headless,
            compute_step_rewards,
            extract_observations,
        )
            .chain(),
    );

    app
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::NUM_AGENTS;

    #[test]
    fn headless_app_spawns_and_steps_physics() {
        let mut app = build_headless_app();
        app.update(); // runs Startup spawns + first tick

        let mut players = app.world.query::<&CubePlayer>();
        assert_eq!(players.iter(&app.world).count(), NUM_AGENTS);

        let mut balls = app.world.query_filtered::<&Transform, With<Ball>>();
        let start_y = balls.iter(&app.world).next().expect("ball exists").translation.y;

        for _ in 0..30 {
            app.update();
        }
        let mut balls = app.world.query_filtered::<&Transform, With<Ball>>();
        let end_y = balls.iter(&app.world).next().unwrap().translation.y;
        assert!(end_y < start_y - 0.05, "ball should fall under gravity: {start_y} -> {end_y}");
    }

    #[test]
    fn forward_action_moves_players() {
        use crate::input::AIActions;
        use crate::rl::action::TOTAL_ACTION_SIZE;

        let mut app = build_headless_app();
        app.update();

        let start_x = {
            let mut q = app.world.query::<(&Transform, &CubePlayer)>();
            q.iter(&app.world)
                .find(|(_, p)| p.index == 0 && p.team == crate::game::Team::Orange)
                .unwrap().0.translation.x
        };

        let mut actions = vec![0.0f32; TOTAL_ACTION_SIZE];
        for a in 0..crate::game::NUM_AGENTS {
            actions[a * crate::game::ACTION_SIZE] = 1.0; // move_x = +1 for every agent
        }
        *app.world.resource_mut::<AIActions>() = AIActions::from_slice(&actions);

        for _ in 0..30 {
            app.update();
        }

        let end_x = {
            let mut q = app.world.query::<(&Transform, &CubePlayer)>();
            q.iter(&app.world)
                .find(|(_, p)| p.index == 0 && p.team == crate::game::Team::Orange)
                .unwrap().0.translation.x
        };
        assert!(end_x > start_x + 0.1, "player should move +X: {start_x} -> {end_x}");
    }

    #[test]
    fn blue_is_driven_by_heuristic_without_rl_actions() {
        use crate::game::Team;
        let mut app = build_headless_app();
        app.update(); // startup + posttartup tag + first tick

        let blue_pos = |app: &mut App| -> Vec3 {
            let mut q = app.world.query::<(&Transform, &CubePlayer)>();
            q.iter(&app.world)
                .find(|(_, p)| p.team == Team::Blue && p.index == 0)
                .unwrap().0.translation
        };

        let start = blue_pos(&mut app);
        // No AIActions written: Orange stays put, Blue should chase the ball.
        for _ in 0..30 {
            app.update();
        }
        let end = blue_pos(&mut app);
        assert!(start.distance(end) > 0.1, "blue should move under the heuristic AI: {start:?} -> {end:?}");
    }

    #[test]
    fn goal_event_sets_episode_done() {
        use crate::game::{GoalScoredEvent, Team};

        let mut app = build_headless_app();
        app.update();
        assert!(!app.world.resource::<EpisodeDone>().0);

        app.world.send_event(GoalScoredEvent { scoring_team: Team::Orange });
        app.update();
        assert!(app.world.resource::<EpisodeDone>().0, "a goal must end the episode");
        assert_eq!(app.world.resource::<crate::game::GameState>().score[0], 1);
    }

    #[test]
    fn extraction_populates_obs_and_rewards() {
        use crate::game::{NUM_AGENTS, OBSERVATION_SIZE};

        let mut app = build_headless_app();
        app.update();

        let obs = &app.world.resource::<LatestObs>().0;
        assert_eq!(obs.len(), NUM_AGENTS * OBSERVATION_SIZE);
        assert!(obs.iter().all(|x| x.is_finite()));
        assert!(obs.iter().any(|x| *x != 0.0), "obs should not be all zero");

        let rewards = &app.world.resource::<LatestRewards>().0;
        assert_eq!(rewards.len(), NUM_AGENTS);
        assert!(rewards.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn team_tactics_reaches_blue_heuristic() {
        use crate::game::Team;
        use crate::systems::heuristic_ai::{TeamDirective, TeamTactics, Tactic};

        // Tactics only reparametrize SUPPORT players; with <2 per team the lone cube
        // is always the ball-handler, so tactics are inert and this test is void.
        if crate::game::PLAYERS_PER_TEAM < 2 {
            return;
        }

        // Blue positions (sorted x) after N ticks under a given Blue directive.
        fn blue_xs(dir: TeamDirective) -> Vec<f32> {
            let mut app = build_headless_app();
            app.update(); // startup spawns + PostStartup tag + first tick
            app.world.resource_mut::<TeamTactics>().blue = dir;
            for _ in 0..25 {
                app.update();
            }
            let mut q = app.world.query::<(&Transform, &CubePlayer)>();
            let mut xs: Vec<f32> = q
                .iter(&app.world)
                .filter(|(_, p)| p.team == Team::Blue)
                .map(|(t, _)| t.translation.x)
                .collect();
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            xs
        }

        let balanced = blue_xs(TeamDirective::uniform(Tactic::Balanced.params()));
        let low_block = blue_xs(TeamDirective::uniform(Tactic::LowBlock.params()));

        // Low Block (deeper defenders, minimal push, narrower) must reshape Blue.
        let changed = balanced.iter().zip(&low_block).any(|(a, b)| (a - b).abs() > 1e-2);
        assert!(changed, "tactic must change Blue's shape: {balanced:?} vs {low_block:?}");
    }
}
