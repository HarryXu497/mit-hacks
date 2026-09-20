use bevy::prelude::*;
use super::config::Team;

#[derive(Event, Debug, Clone)]
pub struct GoalScoredEvent {
    pub scoring_team: Team,
}

#[derive(Event, Debug, Clone)]
pub struct GameOverEvent {
    pub winner: Option<Team>,
}

#[derive(Event, Debug, Clone)]
pub struct ResetGameEvent;

#[derive(Event, Debug, Clone)]
pub struct BallTouchedEvent {
    pub team: Team,
}
