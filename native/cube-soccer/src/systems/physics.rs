use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::game::config::*;

pub fn configure_physics(mut rapier_config: ResMut<RapierConfiguration>) {
    rapier_config.gravity = Vec3::new(0.0, GRAVITY, 0.0);
}

pub fn setup_physics_timestep(mut rapier_config: ResMut<RapierConfiguration>) {
    // Configure fixed timestep for deterministic physics
    rapier_config.timestep_mode = TimestepMode::Fixed {
        dt: PHYSICS_TIMESTEP,
        substeps: 1,
    };
}
