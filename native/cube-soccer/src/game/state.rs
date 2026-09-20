use bevy::prelude::*;
use super::config::{Team, MATCH_DURATION_SECS, ROUND_DURATION_SECS};

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum MatchState {
    #[default]
    Playing,
    GoalScored,
    RoundOver,  // When round timer expires
    Paused,
    GameOver,
}

#[derive(Resource, Debug, Clone)]
pub struct GameState {
    pub score: [u32; 2],  // [Orange, Blue]
    pub time_remaining: f32,
    pub round_timer: f32,
    pub winner: Option<Team>,
    pub current_step: u32,
    /// Who scored the most recent goal, so the restart can give the kickoff to the side that
    /// conceded. `None` before the first goal of a match.
    pub last_scorer: Option<Team>,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            score: [0, 0],
            time_remaining: MATCH_DURATION_SECS,
            round_timer: ROUND_DURATION_SECS,
            winner: None,
            current_step: 0,
            last_scorer: None,
        }
    }
}

impl GameState {
    pub fn reset(&mut self) {
        self.score = [0, 0];
        self.time_remaining = MATCH_DURATION_SECS;
        self.round_timer = ROUND_DURATION_SECS;
        self.winner = None;
        self.current_step = 0;
        self.last_scorer = None;
    }

    pub fn reset_round(&mut self) {
        self.round_timer = ROUND_DURATION_SECS;
    }

    pub fn add_goal(&mut self, team: Team) {
        match team {
            Team::Orange => self.score[0] += 1,
            Team::Blue => self.score[1] += 1,
        }
        self.last_scorer = Some(team);
    }

    /// Total goals scored by both sides.
    pub fn goals_scored(&self) -> u32 {
        self.score[0] + self.score[1]
    }

    pub fn get_score(&self, team: Team) -> u32 {
        match team {
            Team::Orange => self.score[0],
            Team::Blue => self.score[1],
        }
    }

    pub fn score_diff(&self, team: Team) -> i32 {
        match team {
            Team::Orange => self.score[0] as i32 - self.score[1] as i32,
            Team::Blue => self.score[1] as i32 - self.score[0] as i32,
        }
    }
}
