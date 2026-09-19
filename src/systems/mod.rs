//! Bevy ECS systems for game logic.
//!
//! This module contains all the systems that run during the game loop,
//! handling physics, movement, scoring, and visual effects.
//!
//! # Modules
//!
//! - [`camera`]: Camera setup and positioning
//! - [`display`]: 7-segment scoreboard display updates
//! - [`effects`]: Visual effects (cube decomposition)
//! - [`eyes`]: Googly eyes animation
//! - [`movement`]: Player movement and jumping physics
//! - [`physics`]: Rapier physics configuration
//! - [`reset`]: Game reset after goals and rounds
//! - [`scoring`]: Goal detection and score tracking

pub mod camera;
pub mod display;
pub mod effects;
pub mod eyes;
pub mod movement;
pub mod physics;
pub mod reset;
pub mod scoring;
pub mod trail;

pub use camera::*;
pub use display::*;
pub use effects::*;
pub use trail::*;
pub use eyes::*;
pub use movement::*;
pub use physics::*;
pub use reset::*;
pub use scoring::*;
