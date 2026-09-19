use bevy::prelude::*;

#[derive(Component)]
pub struct ScoreText;

#[derive(Component)]
pub struct TimerText;

#[derive(Component)]
pub struct OrangeScoreText;

#[derive(Component)]
pub struct BlueScoreText;

// Wall-mounted scoreboard components
#[derive(Component)]
pub struct WallScoreText;

#[derive(Component)]
pub struct WallTimerText;

pub fn setup_ui(_commands: Commands) {
    // HUD removed - using 3D wall-mounted scoreboard instead
}
