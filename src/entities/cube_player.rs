use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::game::config::*;

// Player collision filter: collides with everything
fn player_collision_groups() -> CollisionGroups {
    CollisionGroups::new(
        PLAYER_GROUP,
        Group::ALL, // Collide with everything
    )
}

#[derive(Component)]
pub struct CubePlayer {
    pub team: Team,
    pub can_jump: bool,
}

/// Component for googly eye pupils that move with velocity
#[derive(Component)]
pub struct GooglyPupil {
    pub base_offset: Vec3,  // Original position relative to eye center
}

#[derive(Component, Default)]
pub struct PlayerInput {
    pub movement: Vec2,  // X, Z
    pub jump: bool,
}

#[derive(Bundle)]
pub struct CubePlayerBundle {
    pub player: CubePlayer,
    pub input: PlayerInput,
    pub pbr: PbrBundle,
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub collision_groups: CollisionGroups,
    pub velocity: Velocity,
    pub friction: Friction,
    pub restitution: Restitution,
    pub mass: ColliderMassProperties,
    pub locked_axes: LockedAxes,
    pub damping: Damping,
    pub ccd: Ccd,
}

impl CubePlayerBundle {
    pub fn new(
        team: Team,
        position: Vec3,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> Self {
        let color = team.color();

        Self {
            player: CubePlayer { team, can_jump: true },
            input: PlayerInput::default(),
            pbr: PbrBundle {
                mesh: meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE)),
                material: materials.add(StandardMaterial {
                    base_color: color,
                    metallic: 0.5,
                    perceptual_roughness: 0.4,
                    ..default()
                }),
                transform: Transform::from_translation(position),
                ..default()
            },
            rigid_body: RigidBody::Dynamic,
            collider: Collider::cuboid(CUBE_SIZE / 2.0, CUBE_SIZE / 2.0, CUBE_SIZE / 2.0),
            collision_groups: player_collision_groups(),
            velocity: Velocity::default(),
            friction: Friction::coefficient(CUBE_FRICTION),
            restitution: Restitution::coefficient(CUBE_RESTITUTION),
            mass: ColliderMassProperties::Mass(CUBE_MASS),
            locked_axes: LockedAxes::ROTATION_LOCKED_X | LockedAxes::ROTATION_LOCKED_Z,  // Allow Y rotation only
            damping: Damping {
                linear_damping: 2.0,
                angular_damping: 0.0,
            },
            ccd: Ccd::enabled(),
        }
    }
}

pub fn spawn_players(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Eye materials (shared)
    let eye_white = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        ..default()
    });
    let pupil_black = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        ..default()
    });
    let eye_mesh = meshes.add(Sphere::new(0.2).mesh().uv(16, 8));
    let pupil_mesh = meshes.add(Sphere::new(0.12).mesh().uv(12, 6));

    // Orange player (left)
    spawn_player_with_eyes(
        &mut commands,
        &mut meshes,
        &mut materials,
        Team::Orange,
        Vec3::new(-FIELD_WIDTH / 4.0, FIELD_HEIGHT + CUBE_SIZE, 0.0),
        eye_white.clone(),
        pupil_black.clone(),
        eye_mesh.clone(),
        pupil_mesh.clone(),
    );

    // Blue player (right)
    spawn_player_with_eyes(
        &mut commands,
        &mut meshes,
        &mut materials,
        Team::Blue,
        Vec3::new(FIELD_WIDTH / 4.0, FIELD_HEIGHT + CUBE_SIZE, 0.0),
        eye_white,
        pupil_black,
        eye_mesh,
        pupil_mesh,
    );
}

fn spawn_player_with_eyes(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    team: Team,
    position: Vec3,
    eye_material: Handle<StandardMaterial>,
    pupil_material: Handle<StandardMaterial>,
    eye_mesh: Handle<Mesh>,
    pupil_mesh: Handle<Mesh>,
) {
    let half_size = CUBE_SIZE / 2.0;
    let eye_z = half_size + 0.05;  // Slightly in front of cube face
    let eye_y = 0.15;  // Slightly above center
    let eye_spacing = 0.3;  // Distance between eyes

    commands.spawn(CubePlayerBundle::new(team, position, meshes, materials))
        .with_children(|parent| {
            // Left eye (white globe)
            parent.spawn(PbrBundle {
                mesh: eye_mesh.clone(),
                material: eye_material.clone(),
                transform: Transform::from_xyz(-eye_spacing, eye_y, eye_z),
                ..default()
            }).with_children(|eye| {
                // Left pupil (positioned in front of the eye globe)
                eye.spawn((
                    PbrBundle {
                        mesh: pupil_mesh.clone(),
                        material: pupil_material.clone(),
                        transform: Transform::from_xyz(0.0, 0.0, 0.12),
                        ..default()
                    },
                    GooglyPupil { base_offset: Vec3::new(0.0, 0.0, 0.12) },
                ));
            });

            // Right eye (white globe)
            parent.spawn(PbrBundle {
                mesh: eye_mesh.clone(),
                material: eye_material.clone(),
                transform: Transform::from_xyz(eye_spacing, eye_y, eye_z),
                ..default()
            }).with_children(|eye| {
                // Right pupil (positioned in front of the eye globe)
                eye.spawn((
                    PbrBundle {
                        mesh: pupil_mesh.clone(),
                        material: pupil_material.clone(),
                        transform: Transform::from_xyz(0.0, 0.0, 0.12),
                        ..default()
                    },
                    GooglyPupil { base_offset: Vec3::new(0.0, 0.0, 0.12) },
                ));
            });
        });
}

/// Get the initial spawn position for a player
pub fn get_spawn_position(team: Team) -> Vec3 {
    match team {
        Team::Orange => Vec3::new(-FIELD_WIDTH / 4.0, FIELD_HEIGHT + CUBE_SIZE, 0.0),
        Team::Blue => Vec3::new(FIELD_WIDTH / 4.0, FIELD_HEIGHT + CUBE_SIZE, 0.0),
    }
}
