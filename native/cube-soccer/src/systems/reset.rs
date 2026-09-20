use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{Ball, CubePlayer, get_spawn_position, get_ball_spawn_position};
use crate::game::{GameState, MatchState, Team, KICKOFF_SPEED, RESET_DELAY_SECS};
use super::effects::spawn_decomposition;

/// The velocity the ball is restarted with: a kickoff taken by the side that conceded.
///
/// Restarting a dead-still ball on the centre spot from fixed spawn positions means every
/// restart is the same position, and the play that follows it is the same play. Left alone,
/// a match becomes one goal on a loop -- measured at twenty-nine identical goals in two
/// minutes of simulation. Handing the ball to the side that just conceded is both what soccer
/// does and enough to break that: the advantage changes hands with every goal. The lateral
/// component alternates with the score so that consecutive restarts down the same end still
/// differ, and it is drawn from the score rather than from a random number so a match stays
/// reproducible.
///
/// `conceding` is `None` at a round boundary, where nobody has just scored; the ball is then
/// pushed toward whichever side the parity picks, rather than left dead.
pub fn kickoff_velocity(conceding: Option<Team>, goals_scored: u32) -> Vec3 {
    let forward = match conceding {
        Some(Team::Orange) => 1.0,
        Some(Team::Blue) => -1.0,
        None if goals_scored % 2 == 0 => 1.0,
        None => -1.0,
    };
    let side = if goals_scored % 2 == 0 { 1.0 } else { -1.0 };
    Vec3::new(forward * KICKOFF_SPEED, 0.0, side * KICKOFF_SPEED * 0.55)
}

/// Who kicks off: the side that did not score the last goal.
fn conceding_side(state: &GameState) -> Option<Team> {
    state.last_scorer.map(|scorer| scorer.opponent())
}

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

    // Restart the ball on the centre spot, moving: the side that conceded kicks off.
    if let Ok((mut transform, mut velocity)) = ball_query.get_single_mut() {
        transform.translation = get_ball_spawn_position();
        velocity.linvel = kickoff_velocity(conceding_side(&game_state), game_state.goals_scored());
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

    // Restart the ball on the centre spot, moving, so a round boundary does not hand both
    // sides the same dead ball from the same positions every time.
    if let Ok((mut transform, mut velocity)) = ball_query.get_single_mut() {
        transform.translation = get_ball_spawn_position();
        velocity.linvel = kickoff_velocity(None, game_state.goals_scored());
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

#[cfg(test)]
mod kickoff_tests {
    use super::*;

    #[test]
    fn the_side_that_conceded_gets_the_ball() {
        // Orange attacks +x, so a kickoff taken by Orange sets off that way.
        let orange = kickoff_velocity(Some(Team::Orange), 1);
        assert!(orange.x > 0.0, "orange should restart toward the +x goal, got {orange:?}");
        let blue = kickoff_velocity(Some(Team::Blue), 1);
        assert!(blue.x < 0.0, "blue should restart toward the -x goal, got {blue:?}");
    }

    /// Consecutive restarts must not be the same restart.
    ///
    /// Every player is put back on a fixed spawn and the ball on the centre spot, so a dead
    /// ball means identical openings, and identical openings play out identically: a match
    /// became one goal on a loop, twenty-nine of them in two minutes of simulation.
    #[test]
    fn consecutive_restarts_differ_from_one_another() {
        let first = kickoff_velocity(Some(Team::Orange), 1);
        let second = kickoff_velocity(Some(Team::Orange), 2);
        assert_ne!(first, second, "the same side twice running must not get the same kickoff");
    }

    #[test]
    fn a_round_boundary_still_puts_the_ball_into_play() {
        for goals in 0..4 {
            let velocity = kickoff_velocity(None, goals);
            assert!(
                velocity.length() > 1.0,
                "a round restart with nobody having scored should still roll the ball"
            );
        }
    }

    #[test]
    fn a_kickoff_is_a_roll_not_a_shot() {
        use crate::game::config::SHOT_SPEED;
        let velocity = kickoff_velocity(Some(Team::Blue), 3);
        assert!(
            velocity.length() < SHOT_SPEED / 2.0,
            "a kickoff should not be struck at anything like shot pace, got {velocity:?}"
        );
        assert_eq!(velocity.y, 0.0, "and it stays on the ground");
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
