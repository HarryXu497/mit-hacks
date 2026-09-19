//! Rendering and visual setup.
//!
//! This module handles graphics configuration including lighting,
//! materials, and post-processing effects.
//!
//! # Modules
//!
//! - [`lighting`]: Scene lighting setup (ambient, directional)
//! - [`materials`]: Shared material definitions
//! - [`post_process`]: Post-processing effects (bloom, etc.)

pub mod lighting;
pub mod materials;
pub mod post_process;

pub use lighting::*;
pub use materials::*;
pub use post_process::*;
