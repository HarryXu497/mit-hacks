//! Bevy ECS systems for game logic.
//!
//! This module contains all the systems that run during the game loop,
//! handling physics, movement, scoring, and visual effects.
//!
//! # Modules
//!
//! - [`camera`]: Camera setup and positioning
//! - [`display`]: 7-segment scoreboard display updates
//! - [`eyes`]: Googly eyes animation
//! - [`movement`]: Player movement and jumping physics
//! - [`physics`]: Rapier physics configuration
//! - [`reset`]: Game reset after goals and rounds
//! - [`scoring`]: Goal detection and score tracking
//! - [`trail`]: Speed trail particles (built, but not scheduled by the game)

pub mod camera;
pub mod display;
pub mod eyes;
pub mod heuristic_ai;
pub mod kick;
pub mod movement;
pub mod physics;
pub mod possession;
pub mod power_tactics;
pub mod power_vfx;
pub mod reset;
pub mod scoring;
pub mod soccer_ai;
pub mod status_effects;
pub mod superpowers;
pub mod trail;

pub use camera::*;
pub use display::*;
pub use heuristic_ai::*;
// Named rather than globbed: `heuristic_ai` is re-exported wholesale just above, and these two
// modules share its tactic vocabulary, so a glob here would be ambiguous at every use site.
pub use kick::{apply_kicks, clear_kick_cooldowns, tick_kick_cooldowns, KickCooldowns};
pub use soccer_ai::{apply_soccer_ai, clear_play_memory, PlayMemory, PlayStyle};
pub use power_tactics::{bearer_slot, counter_to, should_fire, Actor, Cast};
pub use possession::*;
pub use trail::*;
pub use eyes::*;
pub use movement::*;
pub use physics::*;
pub use reset::*;
pub use scoring::*;
pub use status_effects::*;
pub use superpowers::*;
