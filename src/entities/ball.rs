use bevy::prelude::*;
use bevy::render::mesh::Meshable;
use bevy_rapier3d::prelude::*;
use crate::game::config::*;

// Ball collision filter: collides with everything except barriers
fn ball_collision_groups() -> CollisionGroups {
    CollisionGroups::new(
        BALL_GROUP,
        Group::ALL & !BARRIER_GROUP, // Collide with all except barriers
    )
}

#[derive(Component)]
pub struct Ball;

#[derive(Bundle)]
pub struct BallBundle {
    pub ball: Ball,
    pub pbr: PbrBundle,
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub collision_groups: CollisionGroups,
    pub velocity: Velocity,
    pub friction: Friction,
    pub restitution: Restitution,
    pub mass: ColliderMassProperties,
    pub damping: Damping,
    pub ccd: Ccd,
}

impl BallBundle {
    pub fn new(
        position: Vec3,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> Self {
        Self {
            ball: Ball,
            pbr: PbrBundle {
                mesh: meshes.add(Sphere::new(BALL_RADIUS).mesh().uv(32, 18)),
                material: materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    metallic: 0.1,
                    perceptual_roughness: 0.8,
                    ..default()
                }),
                transform: Transform::from_translation(position),
                ..default()
            },
            rigid_body: RigidBody::Dynamic,
            collider: Collider::ball(BALL_RADIUS),
            collision_groups: ball_collision_groups(),
            velocity: Velocity::default(),
            friction: Friction::coefficient(BALL_FRICTION),
            restitution: Restitution::coefficient(BALL_RESTITUTION),
            mass: ColliderMassProperties::Mass(BALL_MASS),
            damping: Damping {
                linear_damping: BALL_LINEAR_DAMPING,
                angular_damping: BALL_ANGULAR_DAMPING,
            },
            ccd: Ccd::disabled(), // CCD off for sim throughput; re-enable if the ball tunnels
        }
    }
}

pub fn spawn_ball(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn(BallBundle::new(
        get_ball_spawn_position(),
        &mut meshes,
        &mut materials,
    ));
}

/// Get the spawn position for the ball (center of field, slightly above)
pub fn get_ball_spawn_position() -> Vec3 {
    Vec3::new(0.0, FIELD_HEIGHT + BALL_RADIUS + 1.0, 0.0)
}
