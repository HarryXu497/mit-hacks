//! Reinforcement learning environment interface.
//!
//! This module provides the RL environment implementation for training
//! AI agents to play Cube Soccer. It follows the Gymnasium API pattern.
//!
//! # Modules
//!
//! - [`action`]: Action space definition (movement, jump)
//! - [`environment`]: Main RL environment struct with step/reset
//! - [`observation`]: Observation space (player positions, ball state, etc.)
//! - [`reward`]: Reward calculation (goals, ball progress, touches)
//!
//! # Observation Space
//!
//! Each player observes 22 features (44 total for both players):
//! - Player position (x, y, z) - normalized
//! - Player velocity (vx, vy, vz) - normalized
//! - Opponent position (relative)
//! - Opponent velocity (relative)
//! - Ball position (relative)
//! - Ball velocity
//! - Distance to own goal
//! - Distance to opponent goal
//! - Score difference
//! - Time remaining
//!
//! # Action Space
//!
//! Each player has 4 continuous actions [-1, 1]:
//! - move_x: Left/right movement
//! - move_z: Forward/backward movement
//! - jump: Jump trigger (>0.5 activates)
//! - reserved: For future use
//!
//! # Rewards
//!
//! - Goal scored: +10
//! - Goal conceded: -10
//! - Ball moving toward opponent goal: +0.01/step
//! - Touching the ball: +0.1
//! - Match win: +5
//! - Match loss: -5

pub mod action;
pub mod environment;
pub mod observation;
pub mod reward;

pub use action::*;
pub use environment::*;
pub use observation::*;
pub use reward::*;
