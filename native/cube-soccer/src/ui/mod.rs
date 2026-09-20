//! User interface components.
//!
//! This module provides the in-game UI elements including the HUD,
//! scoreboard overlay, and timer display.
//!
//! # Modules
//!
//! - [`hud`]: Main HUD setup and layout
//! - [`scoreboard`]: Score display and updates
//! - [`timer`]: Round and match timer display

pub mod hud;
pub mod scoreboard;
pub mod timer;

pub use hud::*;
pub use scoreboard::*;
pub use timer::*;
