//! Python bindings for the Cube Soccer environment.
//!
//! This module provides PyO3 bindings to expose the RL environment
//! to Python, enabling training with frameworks like Stable-Baselines3.
//!
//! # Usage
//!
//! Build with maturin:
//! ```bash
//! maturin develop --release
//! ```
//!
//! Then use in Python:
//! ```python
//! from cube_soccer import CubeSoccerEnv
//!
//! env = CubeSoccerEnv()
//! obs = env.reset()
//! obs, reward, done, truncated, info = env.step(action)
//! ```
//!
//! # Features
//!
//! Enable the `python` feature to build Python bindings:
//! ```toml
//! [dependencies]
//! cube-soccer = { features = ["python"] }
//! ```

pub mod bindings;

#[cfg(feature = "python")]
pub use bindings::*;
