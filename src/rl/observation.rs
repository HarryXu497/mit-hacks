use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::entities::{Ball, CubePlayer};
use crate::game::{GameState, Team, FIELD_WIDTH, FIELD_DEPTH, ARENA_HEIGHT, CUBE_MAX_SPEED, MATCH_DURATION_SECS};

/// Observation for a single player (22 floats)
#[derive(Debug, Clone, Default)]
pub struct Observation {
    // Player position and velocity (normalized)
    pub player_pos: [f32; 3],       // x, y, z
    pub player_vel: [f32; 3],       // vx, vy, vz

    // Opponent position and velocity (relative)
    pub opponent_pos: [f32; 3],     // x, y, z relative
    pub opponent_vel: [f32; 3],     // vx, vy, vz

    // Ball position and velocity (relative)
    pub ball_pos: [f32; 3],         // x, y, z relative
    pub ball_vel: [f32; 3],         // vx, vy, vz

    // Distance to goals
    pub dist_to_own_goal: f32,
    pub dist_to_opponent_goal: f32,

    // Match state
    pub score_diff: f32,            // Our score - their score (normalized)
    pub time_remaining: f32,        // Remaining time normalized [0, 1]
}

impl Observation {
    pub const SIZE: usize = 22;

    pub fn extract(
        team: Team,
        player_transform: &Transform,
        player_velocity: &Velocity,
        opponent_transform: &Transform,
        opponent_velocity: &Velocity,
        ball_transform: &Transform,
        ball_velocity: &Velocity,
        game_state: &GameState,
    ) -> Self {
        // Normalize positions by field dimensions
        let norm_pos = |pos: Vec3| -> [f32; 3] {
            [
                pos.x / (FIELD_WIDTH / 2.0),
                pos.y / ARENA_HEIGHT,
                pos.z / (FIELD_DEPTH / 2.0),
            ]
        };

        // Normalize velocities
        let norm_vel = |vel: Vec3| -> [f32; 3] {
            [
                vel.x / CUBE_MAX_SPEED,
                vel.y / CUBE_MAX_SPEED,
                vel.z / CUBE_MAX_SPEED,
            ]
        };

        // Relative positions (from player's perspective)
        let player_pos = player_transform.translation;
        let opponent_rel = opponent_transform.translation - player_pos;
        let ball_rel = ball_transform.translation - player_pos;

        // Flip for blue player (symmetry)
        let flip = if team == Team::Blue { -1.0 } else { 1.0 };

        Self {
            player_pos: norm_pos(player_pos * Vec3::new(flip, 1.0, 1.0)),
            player_vel: norm_vel(player_velocity.linvel * Vec3::new(flip, 1.0, 1.0)),
            opponent_pos: norm_pos(opponent_rel * Vec3::new(flip, 1.0, 1.0)),
            opponent_vel: norm_vel(opponent_velocity.linvel * Vec3::new(flip, 1.0, 1.0)),
            ball_pos: norm_pos(ball_rel * Vec3::new(flip, 1.0, 1.0)),
            ball_vel: norm_vel(ball_velocity.linvel * Vec3::new(flip, 1.0, 1.0)),
            dist_to_own_goal: (player_pos.x * flip + FIELD_WIDTH / 2.0) / FIELD_WIDTH,
            dist_to_opponent_goal: (FIELD_WIDTH / 2.0 - player_pos.x * flip) / FIELD_WIDTH,
            score_diff: game_state.score_diff(team) as f32 / 10.0,
            time_remaining: game_state.time_remaining / MATCH_DURATION_SECS,
        }
    }

    pub fn to_array(&self) -> [f32; Self::SIZE] {
        [
            self.player_pos[0], self.player_pos[1], self.player_pos[2],
            self.player_vel[0], self.player_vel[1], self.player_vel[2],
            self.opponent_pos[0], self.opponent_pos[1], self.opponent_pos[2],
            self.opponent_vel[0], self.opponent_vel[1], self.opponent_vel[2],
            self.ball_pos[0], self.ball_pos[1], self.ball_pos[2],
            self.ball_vel[0], self.ball_vel[1], self.ball_vel[2],
            self.dist_to_own_goal,
            self.dist_to_opponent_goal,
            self.score_diff,
            self.time_remaining,
        ]
    }
}

/// Get observations for both players
pub fn get_observations(
    player_query: &Query<(&Transform, &Velocity, &CubePlayer)>,
    ball_query: &Query<(&Transform, &Velocity), With<Ball>>,
    game_state: &GameState,
) -> Option<[f32; 44]> {
    let mut players: Vec<_> = player_query.iter().collect();

    if players.len() != 2 {
        return None;
    }

    // Sort by team (Orange first)
    players.sort_by_key(|(_, _, p)| match p.team {
        Team::Orange => 0,
        Team::Blue => 1,
    });

    let (ball_transform, ball_velocity) = ball_query.get_single().ok()?;

    let (orange_transform, orange_velocity, orange_player) = players[0];
    let (blue_transform, blue_velocity, blue_player) = players[1];

    let orange_obs = Observation::extract(
        orange_player.team,
        orange_transform,
        orange_velocity,
        blue_transform,
        blue_velocity,
        ball_transform,
        ball_velocity,
        game_state,
    );

    let blue_obs = Observation::extract(
        blue_player.team,
        blue_transform,
        blue_velocity,
        orange_transform,
        orange_velocity,
        ball_transform,
        ball_velocity,
        game_state,
    );

    let mut result = [0.0f32; 44];
    result[0..22].copy_from_slice(&orange_obs.to_array());
    result[22..44].copy_from_slice(&blue_obs.to_array());

    Some(result)
}
