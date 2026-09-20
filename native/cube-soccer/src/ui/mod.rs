//! User interface components.
//!
//! This module provides the in-game UI elements including the HUD,
//! scoreboard overlay, and timer display.
//!
//! # Modules
//!
//! - [`hud`]: Main HUD setup and layout
//! - [`powers`]: Superpower badges and their cooldowns
//! - [`scoreboard`]: Score display and updates
//! - [`timer`]: Round and match timer display

pub mod hud;
pub mod powers;
pub mod scoreboard;
pub mod timer;
pub mod versus;

pub use hud::*;
pub use powers::*;
pub use scoreboard::*;
pub use timer::*;
pub use versus::*;
