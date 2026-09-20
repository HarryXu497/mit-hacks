use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{Ball, CubePlayer, get_spawn_position, get_ball_spawn_position};
use crate::game::{GameState, MatchState, RESET_DELAY_SECS};
use super::effects::spawn_decomposition;

#[derive(Resource)]
pub struct ResetTimer(pub Timer);

impl Default for ResetTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(RESET_DELAY_SECS, TimerMode::Once))
    }
}

/// Reset positions after a goal (with delay timer)
pub fn reset_after_goal(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ball_query: Query<(&mut Transform, &mut Velocity), (With<Ball>, Without<CubePlayer>)>,
    mut player_query: Query<(&mut Transform, &mut Velocity, &CubePlayer), Without<Ball>>,
    mut game_state: ResMut<GameState>,
) {
    // Spawn decomposition effects for players before resetting
    for (transform, _, player) in player_query.iter() {
        spawn_decomposition(
            &mut commands,
            &mut meshes,
            &mut materials,
            transform.translation,
            player.team.color(),
        );
    }

    // Reset ball position and velocity
    if let Ok((mut transform, mut velocity)) = ball_query.get_single_mut() {
        transform.translation = get_ball_spawn_position();
        velocity.linvel = Vec3::ZERO;
        velocity.angvel = Vec3::ZERO;
    }

    // Reset player positions and velocities
    for (mut transform, mut velocity, player) in player_query.iter_mut() {
        transform.translation = get_spawn_position(player.team, player.index);
        velocity.linvel = Vec3::ZERO;
        velocity.angvel = Vec3::ZERO;
    }

    // Reset the round timer
    game_state.reset_round();

    // Add a timer to return to playing state (1 second delay)
    commands.insert_resource(ResetTimer::default());
}

/// Reset positions when round timer expires (immediate)
pub fn reset_after_round(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ball_query: Query<(&mut Transform, &mut Velocity), (With<Ball>, Without<CubePlayer>)>,
    mut player_query: Query<(&mut Transform, &mut Velocity, &CubePlayer), Without<Ball>>,
    mut game_state: ResMut<GameState>,
    mut next_state: ResMut<NextState<MatchState>>,
) {
    // Spawn decomposition effects for players before resetting
    for (transform, _, player) in player_query.iter() {
        spawn_decomposition(
            &mut commands,
            &mut meshes,
            &mut materials,
            transform.translation,
            player.team.color(),
        );
    }

    // Reset ball position and velocity
    if let Ok((mut transform, mut velocity)) = ball_query.get_single_mut() {
        transform.translation = get_ball_spawn_position();
        velocity.linvel = Vec3::ZERO;
        velocity.angvel = Vec3::ZERO;
    }

    // Reset player positions and velocities
    for (mut transform, mut velocity, player) in player_query.iter_mut() {
        transform.translation = get_spawn_position(player.team, player.index);
        velocity.linvel = Vec3::ZERO;
        velocity.angvel = Vec3::ZERO;
    }

    // Reset the round timer
    game_state.reset_round();

    // Immediately go back to playing (no delay for round timeout)
    next_state.set(MatchState::Playing);
}

pub fn check_reset_timer(
    time: Res<Time>,
    mut timer: ResMut<ResetTimer>,
    mut next_state: ResMut<NextState<MatchState>>,
) {
    timer.0.tick(time.delta());

    if timer.0.finished() {
        next_state.set(MatchState::Playing);
    }
}

pub fn reset_game(
    mut ball_query: Query<(&mut Transform, &mut Velocity), (With<Ball>, Without<CubePlayer>)>,
    mut player_query: Query<(&mut Transform, &mut Velocity, &CubePlayer), Without<Ball>>,
    mut game_state: ResMut<GameState>,
) {
    // Reset game state
    game_state.reset();

    // Reset ball
    if let Ok((mut transform, mut velocity)) = ball_query.get_single_mut() {
        transform.translation = get_ball_spawn_position();
        velocity.linvel = Vec3::ZERO;
        velocity.angvel = Vec3::ZERO;
    }

    // Reset players
    for (mut transform, mut velocity, player) in player_query.iter_mut() {
        transform.translation = get_spawn_position(player.team, player.index);
        velocity.linvel = Vec3::ZERO;
        velocity.angvel = Vec3::ZERO;
    }
}
