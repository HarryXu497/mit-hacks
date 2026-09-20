//! The jungle palette the in-match HUD is drawn in.
//!
//! Matched to the menus' so the HUD and the menus look like one game. It lives in its own module
//! because more than one overlay is drawn in it -- the superpower badges and the goal banner -- and
//! two copies of a palette drift the moment either is touched.

use bevy::prelude::Color;

pub const SLAB: Color = Color::rgba(0.078, 0.102, 0.086, 0.88);
pub const SLAB_READY: Color = Color::rgba(0.16, 0.24, 0.16, 0.92);
pub const EDGE_READY: Color = Color::rgb(0.886, 0.667, 0.251);
pub const EDGE_COOLING: Color = Color::rgba(0.29, 0.36, 0.30, 1.0);
/// Laid over a badge while cooling; shrinks away as the power recharges.
pub const CHILL: Color = Color::rgba(0.04, 0.06, 0.05, 0.82);
pub const CLOTH: Color = Color::rgb(0.941, 0.918, 0.839);
pub const CLOTH_DIM: Color = Color::rgb(0.69, 0.72, 0.65);
