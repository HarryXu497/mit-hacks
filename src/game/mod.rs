//! Core game logic and configuration.
//!
//! This module contains the main game systems, state management, events,
//! and configuration constants for the Cube Soccer game.
//!
//! # Modules
//!
//! - [`config`]: Game constants (field dimensions, physics parameters, rewards)
//! - [`events`]: Game events (goal scored, game over, ball touched)
//! - [`plugin`]: Main Bevy plugin that initializes the game
//! - [`state`]: Game state and match state management

pub mod config;
pub mod events;
pub mod plugin;
pub mod state;

pub use config::*;
pub use events::*;
pub use plugin::CubeSoccerPlugin;
pub use state::*;
