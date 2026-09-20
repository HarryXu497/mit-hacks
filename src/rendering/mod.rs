//! Rendering and visual setup.
//!
//! This module handles graphics configuration including lighting,
//! materials, and post-processing effects.
//!
//! # Modules
//!
//! - [`batching`]: One-time merge of static props into shared draws
//! - [`lighting`]: Scene lighting setup (ambient, directional)
//! - [`materials`]: Shared material definitions
//! - [`post_process`]: Post-processing effects (bloom, etc.)

pub mod batching;
pub mod lighting;
pub mod materials;
pub mod post_process;
pub mod stylized;

pub use lighting::*;
pub use materials::*;
pub use post_process::*;
