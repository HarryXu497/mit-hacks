use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{Ball, GoalSensor};
use crate::game::{GoalScoredEvent, GameState, MatchState, Team, GOALS_TO_WIN};

pub fn detect_goals(
    mut goal_events: EventWriter<GoalScoredEvent>,
    ball_query: Query<Entity, With<Ball>>,
    goal_sensors: Query<(&GoalSensor, Entity)>,
    rapier_context: Res<RapierContext>,
) {
    let Ok(ball_entity) = ball_query.get_single() else {
        return;
    };

    for (goal_sensor, sensor_entity) in goal_sensors.iter() {
        if rapier_context.intersection_pair(ball_entity, sensor_entity) == Some(true) {
            // Goal scored AGAINST the team whose goal this is
            let scoring_team = goal_sensor.team.opponent();
            goal_events.send(GoalScoredEvent { scoring_team });
        }
    }
}

pub fn handle_goal_scored(
    mut goal_events: EventReader<GoalScoredEvent>,
    mut game_state: ResMut<GameState>,
    mut next_state: ResMut<NextState<MatchState>>,
) {
    for event in goal_events.read() {
        game_state.add_goal(event.scoring_team);

        info!(
            "Goal scored by {:?}! Score: {} - {}",
            event.scoring_team,
            game_state.score[0],
            game_state.score[1]
        );

        // Check if someone won
        if game_state.get_score(event.scoring_team) >= GOALS_TO_WIN {
            game_state.winner = Some(event.scoring_team);
            next_state.set(MatchState::GameOver);
        } else {
            // Goal scored - will reset after 1 second delay
            next_state.set(MatchState::GoalScored);
        }
    }
}

/// Update both match timer and round timer
pub fn update_timers(
    time: Res<Time>,
    mut game_state: ResMut<GameState>,
    mut next_state: ResMut<NextState<MatchState>>,
) {
    let delta = time.delta_seconds();

    // Update match timer
    game_state.time_remaining -= delta;

    if game_state.time_remaining <= 0.0 {
        game_state.time_remaining = 0.0;

        // Determine winner by score
        if game_state.score[0] > game_state.score[1] {
            game_state.winner = Some(Team::Orange);
        } else if game_state.score[1] > game_state.score[0] {
            game_state.winner = Some(Team::Blue);
        } else {
            game_state.winner = None; // Draw
        }

        next_state.set(MatchState::GameOver);
        return;
    }

    // Update round timer (15 seconds)
    game_state.round_timer -= delta;

    if game_state.round_timer <= 0.0 {
        // Round over - reset positions
        next_state.set(MatchState::RoundOver);
    }
}
