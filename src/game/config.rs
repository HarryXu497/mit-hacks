use bevy::prelude::*;
use bevy_rapier3d::prelude::Group;

// === COLLISION GROUPS ===
// Ball: collides with everything except barriers
// Players: collide with everything including barriers
// Barriers: only collide with players
pub const BALL_GROUP: Group = Group::GROUP_1;
pub const PLAYER_GROUP: Group = Group::GROUP_2;
pub const BARRIER_GROUP: Group = Group::GROUP_3;

// === ARENA (White environment) ===
pub const ARENA_WIDTH: f32 = 42.0; // X - total width
pub const ARENA_DEPTH: f32 = 38.0; // Z - depth (must be > extended field)
pub const ARENA_HEIGHT: f32 = 15.0; // Y - wall height

pub const WALL_THICKNESS: f32 = 0.5;
pub const WALL_COLOR: Color = Color::rgb(0.95, 0.95, 0.95); // Off-white
pub const GRID_COLOR: Color = Color::rgb(0.85, 0.85, 0.85); // Grey lines

// === FIELD (Grey play area) ===
pub const FIELD_WIDTH: f32 = 36.0; // X
pub const FIELD_DEPTH: f32 = 24.0; // Z
pub const FIELD_HEIGHT: f32 = 1.0; // Y - platform thickness
pub const FIELD_COLOR: Color = Color::rgb(0.25, 0.25, 0.25); // Dark grey
pub const FIELD_GRID_SPACING: f32 = 2.0; // White grid spacing
pub const SIDE_EXTENSION: f32 = 5.0; // Side zone extension (same as goal side area)
pub const FLUORESCENT_COLOR: Color = Color::rgb(0.0, 1.0, 0.5); // Fluorescent green

// === GOALS ===
pub const GOAL_WIDTH: f32 = 0.5; // Back thickness (legacy, kept for compatibility)
pub const GOAL_HEIGHT: f32 = 5.0; // Height
pub const GOAL_DEPTH: f32 = 8.0; // Opening width (Z direction)
pub const GOAL_NET_DEPTH: f32 = 3.0; // How far the net extends behind the goal line
pub const GOAL_ORANGE: Color = Color::rgb(1.0, 0.6, 0.2);
pub const GOAL_BLUE: Color = Color::rgb(0.2, 0.6, 1.0);
pub const NET_COLOR: Color = Color::rgba(0.9, 0.9, 0.9, 0.4); // White semi-transparent

// === CUBE PLAYERS ===
pub const CUBE_SIZE: f32 = 1.5; // Cube size
pub const CUBE_MASS: f32 = 10.0;
pub const CUBE_FRICTION: f32 = 0.5;
pub const CUBE_RESTITUTION: f32 = 0.3; // Bounce

pub const CUBE_MAX_SPEED: f32 = 15.0; // Max speed
pub const CUBE_ACCELERATION: f32 = 50.0;
pub const CUBE_JUMP_FORCE: f32 = 60.0; // Small jump, can't jump over walls

pub const CUBE_ORANGE_COLOR: Color = Color::rgb(1.0, 0.6, 0.2);
pub const CUBE_BLUE_COLOR: Color = Color::rgb(0.2, 0.6, 1.0);

// === BALL ===
pub const BALL_RADIUS: f32 = 0.6;
pub const BALL_MASS: f32 = 1.0;
pub const BALL_FRICTION: f32 = 0.4;
pub const BALL_RESTITUTION: f32 = 0.85; // Very bouncy
pub const BALL_LINEAR_DAMPING: f32 = 0.3;
pub const BALL_ANGULAR_DAMPING: f32 = 0.5;

// === PHYSICS ===
pub const GRAVITY: f32 = -50.0; // Y gravity (snappy jump)
pub const PHYSICS_TIMESTEP: f32 = 1.0 / 60.0;

// === MATCH ===
pub const MATCH_DURATION_SECS: f32 = 300.0; // 5 minutes total
pub const ROUND_DURATION_SECS: f32 = 15.0; // 15 seconds per round
pub const GOALS_TO_WIN: u32 = 10;
pub const RESET_DELAY_SECS: f32 = 1.0; // 1 second pause after goal

// === RL PARAMETERS ===
pub const MAX_EPISODE_STEPS: u32 = 1000;
pub const OBSERVATION_SIZE: usize = 22; // Per player
pub const ACTION_SIZE: usize = 4; // Per player

// === REWARDS ===
pub const REWARD_GOAL: f32 = 10.0;
pub const REWARD_GOAL_AGAINST: f32 = -10.0;
pub const REWARD_BALL_TO_GOAL: f32 = 0.01; // Per step if ball approaches
pub const REWARD_TOUCH_BALL: f32 = 0.1;
pub const REWARD_WIN: f32 = 5.0;
pub const REWARD_LOSE: f32 = -5.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum Team {
    #[default]
    Orange,
    Blue,
}

impl Team {
    pub fn opponent(&self) -> Team {
        match self {
            Team::Orange => Team::Blue,
            Team::Blue => Team::Orange,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Team::Orange => CUBE_ORANGE_COLOR,
            Team::Blue => CUBE_BLUE_COLOR,
        }
    }

    pub fn goal_color(&self) -> Color {
        match self {
            Team::Orange => GOAL_ORANGE,
            Team::Blue => GOAL_BLUE,
        }
    }
}
