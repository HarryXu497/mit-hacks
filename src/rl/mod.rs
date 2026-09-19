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
//! Each agent observes `13 + 12 * PLAYERS_PER_TEAM` features (per-agent), and
//! the environment exposes one observation per agent (`NUM_AGENTS` total).
//! Each agent sees: its own pos/vel, each teammate's relative pos/vel, each
//! opponent's relative pos/vel, the ball's relative pos/vel, its distances to
//! both goals, the score difference, time remaining, and 3 possession flags
//! (self / teammate / opponent has the ball).
//!
//! # Action Space
//!
//! Each agent has 4 continuous actions [-1, 1] (move_x, move_z, jump, reserved).
//! The environment takes `NUM_AGENTS * 4` actions total, ordered Orange[0..N]
//! then Blue[0..N].
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
pub mod sim;

pub use action::*;
pub use environment::*;
pub use observation::*;
pub use reward::*;
