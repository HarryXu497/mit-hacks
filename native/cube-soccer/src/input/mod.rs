//! Input handling for players and AI agents.
//!
//! This module provides input systems for both human players (keyboard)
//! and AI agents (programmatic control).
//!
//! # Modules
//!
//! - [`keyboard`]: Keyboard input for human players
//!   - Orange team: WASD + Space (jump)
//!   - Blue team: Arrow keys + Enter (jump)
//! - [`ai_controller`]: AI agent input for reinforcement learning
//!
//! # Controls
//!
//! | Team   | Move          | Jump  |
//! |--------|---------------|-------|
//! | Orange | W/A/S/D       | Space |
//! | Blue   | Arrow Keys    | Enter |

pub mod ai_controller;
pub mod keyboard;

pub use ai_controller::*;
pub use keyboard::*;
