use bevy::prelude::*;
use crate::game::{GameState, MatchState, Team};

pub fn tick_timer(
    time: Res<Time>,
    mut game_state: ResMut<GameState>,
    mut next_state: ResMut<NextState<MatchState>>,
) {
    game_state.time_remaining -= time.delta_seconds();
    game_state.current_step += 1;

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
    }
}

pub fn format_time(seconds: f32) -> String {
    let mins = (seconds / 60.0) as u32;
    let secs = (seconds % 60.0) as u32;
    if mins > 0 {
        format!("{}:{:02}", mins, secs)
    } else {
        format!("{}", secs)
    }
}
