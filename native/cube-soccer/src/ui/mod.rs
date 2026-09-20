//! User interface components.
//!
//! This module provides the in-game UI elements including the HUD,
//! scoreboard overlay, and timer display.
//!
//! # Modules
//!
//! - [`goal_banner`]: The banner a goal raises, carrying the scoring side's own character
//! - [`hud`]: Main HUD setup and layout
//! - [`ink`]: The palette the whole in-match HUD is drawn in
//! - [`powers`]: Superpower badges and their cooldowns
//! - [`scoreboard`]: Score display and updates
//! - [`timer`]: Round and match timer display

pub mod goal_banner;
pub mod hud;
pub mod ink;
pub mod powers;
pub mod scoreboard;
pub mod timer;

pub use goal_banner::*;
pub use hud::*;
pub use powers::*;
pub use scoreboard::*;
pub use timer::*;
