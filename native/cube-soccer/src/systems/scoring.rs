use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{Ball, GoalSensor};
use crate::game::{
    GoalScoredEvent, GameState, MatchState, Team, GOALS_TO_WIN,
    FIELD_HEIGHT, GOAL_HEIGHT, GOAL_DEPTH, effective_goal_dist,
};
use crate::systems::heuristic_ai::ActiveRoster;

/// Runtime scorable half-width in Z (the goal-size curriculum knob). Regulation is
/// `GOAL_DEPTH/2 - 0.2` (matches the physical sensor); the curriculum starts this
/// wide (up to ~half the field) so crude pushes score, then narrows to regulation.
/// Only the headless [`detect_goals_by_position`] path honours it; a missing
/// resource = regulation width.
#[derive(Resource, Clone, Copy, Debug)]
pub struct GoalHalfWidth(pub f32);

impl GoalHalfWidth {
    pub fn regulation() -> f32 {
        GOAL_DEPTH / 2.0 - 0.2
    }
}

impl Default for GoalHalfWidth {
    fn default() -> Self {
        Self(Self::regulation())
    }
}

/// Headless goal detection by ball position, honouring [`GoalHalfWidth`]. A goal is
/// scored when the ball is past a goal line (`|x| >= FIELD_WIDTH/2`), within the
/// current scorable half-width in Z, and below the crossbar height. Used instead of
/// the sensor-based [`detect_goals`] so the scorable width can be widened/narrowed
/// during training (the physical posts/nets sit at `z = ±GOAL_DEPTH/2`, so wider
/// balls pass into the open end zone and still register).
pub fn detect_goals_by_position(
    mut goal_events: EventWriter<GoalScoredEvent>,
    ball_query: Query<&Transform, With<Ball>>,
    half_width: Option<Res<GoalHalfWidth>>,
    roster: Option<Res<ActiveRoster>>,
) {
    let Ok(ball) = ball_query.get_single() else {
        return;
    };
    let p = ball.translation;
    let hw = half_width.map(|h| h.0).unwrap_or_else(GoalHalfWidth::regulation);
    // Goal line scales with the active roster (field-size curriculum).
    let active = roster.map(|r| r.0).unwrap_or(crate::game::PLAYERS_PER_TEAM);
    let line = effective_goal_dist(active);
    let y_base = FIELD_HEIGHT;
    if p.z.abs() > hw || p.y < y_base || p.y > y_base + GOAL_HEIGHT {
        return;
    }
    if p.x >= line {
        // Past Blue's goal line (+x) -> Orange scores.
        goal_events.send(GoalScoredEvent { scoring_team: Team::Orange });
    } else if p.x <= -line {
        goal_events.send(GoalScoredEvent { scoring_team: Team::Blue });
    }
}

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
