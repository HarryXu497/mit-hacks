//! Game entities and their spawn functions.
//!
//! This module defines all the physical entities in the game world:
//! players, ball, field, goals, and arena walls.
//!
//! # Modules
//!
//! - [`arena`]: Arena walls and scoreboard display
//! - [`ball`]: Ball entity with physics properties
//! - [`character`]: Generated character models worn by players, appearance only
//! - [`cube_player`]: Cube-shaped player entities for both teams
//! - [`field`]: Playing field with grid lines and borders
//! - [`goal`]: Goal posts, nets, and scoring sensors
//!
//! # Example
//!
//! ```ignore
//! // Spawn all entities in your Bevy app
//! app.add_systems(Startup, (
//!     spawn_arena,
//!     spawn_field,
//!     spawn_goals,
//!     spawn_players,
//!     spawn_ball,
//! ));
//! ```

pub mod arena;
pub mod ball;
pub mod character;
pub mod cube_player;
pub mod field;
pub mod goal;

pub use arena::{spawn_arena, spawn_wall_scoreboard, Arena};
pub use ball::{spawn_ball, get_ball_spawn_position, Ball, BallBundle};
pub use character::{skin_path, spawn_skin, CharacterSkin};
pub use cube_player::{spawn_players, get_spawn_position, CubePlayer, CubePlayerBundle, PlayerInput, GooglyPupil};
pub use field::{spawn_field, Field, FieldBorder};
pub use goal::{spawn_goals, Goal, GoalSensor};
