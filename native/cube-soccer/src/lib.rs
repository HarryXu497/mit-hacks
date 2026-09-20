//! Cube Soccer 3D - A minimalist 3D soccer game for Reinforcement Learning
//!
//! This crate provides a fast, Rust-based 3D soccer environment where two cube players
//! compete to score goals. It's designed for RL training with:
//! - Fast physics simulation (Bevy + Rapier3D)
//! - Gymnasium-compatible Python bindings
//! - Headless mode for training
//! - Visual mode for debugging and evaluation

pub mod entities;
pub mod game;
pub mod input;
pub mod rendering;
pub mod rl;
pub mod systems;
pub mod ui;
pub mod jungle;
pub mod assets;
pub mod creation;
pub mod tactics;

#[cfg(feature = "python")]
pub mod python;

// Re-export main types
pub use game::{CubeSoccerPlugin, GameState, MatchState, Team};
pub use rl::{CubeSoccerEnv, EnvConfig, StepResult};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_config_default() {
        let config = EnvConfig::default();
        assert!(config.headless);
        assert!(config.render_mode.is_none());
    }

    #[test]
    fn test_team_opponent() {
        assert_eq!(Team::Orange.opponent(), Team::Blue);
        assert_eq!(Team::Blue.opponent(), Team::Orange);
    }
}
