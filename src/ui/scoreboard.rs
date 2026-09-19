use bevy::prelude::*;
use crate::game::GameState;
use super::hud::{ScoreText, TimerText, OrangeScoreText, BlueScoreText, WallScoreText, WallTimerText};

pub fn update_ui(
    game_state: Res<GameState>,
    mut timer_query: Query<&mut Text, (With<TimerText>, Without<ScoreText>, Without<OrangeScoreText>, Without<BlueScoreText>, Without<WallScoreText>, Without<WallTimerText>)>,
    mut score_query: Query<&mut Text, (With<ScoreText>, Without<TimerText>, Without<OrangeScoreText>, Without<BlueScoreText>, Without<WallScoreText>, Without<WallTimerText>)>,
    mut orange_query: Query<&mut Text, (With<OrangeScoreText>, Without<TimerText>, Without<ScoreText>, Without<BlueScoreText>, Without<WallScoreText>, Without<WallTimerText>)>,
    mut blue_query: Query<&mut Text, (With<BlueScoreText>, Without<TimerText>, Without<ScoreText>, Without<OrangeScoreText>, Without<WallScoreText>, Without<WallTimerText>)>,
    mut wall_timer_query: Query<&mut Text, (With<WallTimerText>, Without<TimerText>, Without<ScoreText>, Without<OrangeScoreText>, Without<BlueScoreText>, Without<WallScoreText>)>,
    mut wall_score_query: Query<&mut Text, (With<WallScoreText>, Without<TimerText>, Without<ScoreText>, Without<OrangeScoreText>, Without<BlueScoreText>, Without<WallTimerText>)>,
) {
    // Round timer (15 seconds countdown)
    let round_time_str = format!("{}", game_state.round_timer.ceil() as u32);

    // Update HUD timer (shows round timer)
    if let Ok(mut text) = timer_query.get_single_mut() {
        text.sections[0].value = round_time_str.clone();
    }

    // Update main score
    if let Ok(mut text) = score_query.get_single_mut() {
        text.sections[0].value = format!(
            "{} - {}",
            game_state.score[0],
            game_state.score[1]
        );
    }

    // Update orange side score
    if let Ok(mut text) = orange_query.get_single_mut() {
        text.sections[0].value = format!("{}", game_state.score[0]);
    }

    // Update blue side score
    if let Ok(mut text) = blue_query.get_single_mut() {
        text.sections[0].value = format!("{}", game_state.score[1]);
    }

    // Update wall timer (shows round timer)
    if let Ok(mut text) = wall_timer_query.get_single_mut() {
        text.sections[0].value = round_time_str;
    }

    // Update wall score
    if let Ok(mut text) = wall_score_query.get_single_mut() {
        text.sections[0].value = format!("{}", game_state.score[0]);
        text.sections[2].value = format!("{}", game_state.score[1]);
    }
}
