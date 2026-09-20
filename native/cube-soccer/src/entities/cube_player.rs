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
    pub index: usize,
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
    pub fire: bool,      // request to fire the cube's superpower this frame
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
    pub status: crate::systems::status_effects::StatusEffects,
}

impl CubePlayerBundle {
    pub fn new(
        team: Team,
        index: usize,
        position: Vec3,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> Self {
        let color = team.color();

        Self {
            player: CubePlayer { team, index, can_jump: true },
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
            ccd: Ccd::disabled(), // CCD off for sim throughput; re-enable if cubes tunnel
            status: crate::systems::status_effects::StatusEffects::default(),
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

    for team in [Team::Orange, Team::Blue] {
        for index in 0..PLAYERS_PER_TEAM {
            spawn_player_with_eyes(
                &mut commands,
                &mut meshes,
                &mut materials,
                team,
                index,
                get_spawn_position(team, index),
                eye_white.clone(),
                pupil_black.clone(),
                eye_mesh.clone(),
                pupil_mesh.clone(),
            );
        }
    }
}

fn spawn_player_with_eyes(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    team: Team,
    index: usize,
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

    // Everything visible hangs off a `PlayerVisual` child rather than off the body itself,
    // because the body's transform belongs to Rapier and to `movement.rs`. Animating a child
    // leaves both alone, so the cube below animates exactly as a generated character does.
    //
    // The body draws nothing of its own. `Visibility::Hidden` would take the children down with
    // it, since visibility propagates -- so its mesh handle is swapped for the default one, which
    // draws nothing while leaving the hierarchy intact. Only the *appearance* moves; the bundle's
    // collider, mass, damping and locked axes are untouched, so physics and the observation
    // vector are unaffected and trained policies stay valid.
    //
    // The cube and its googly eyes are the bottom of three tiers. In the jungle they are replaced
    // by its blocky character, and that in turn by a generated model once one has loaded -- both
    // of which work by removing everything tagged `BlockyCharacter`, which is why these carry the
    // tag. Bare, with no jungle and no model, the cube is what a player looks like.
    let cube_mesh = meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE));
    let cube_material = materials.add(StandardMaterial {
        base_color: team.color(),
        metallic: 0.5,
        perceptual_roughness: 0.4,
        ..default()
    });

    let mut entity = commands.spawn(CubePlayerBundle::new(team, index, position, meshes, materials));
    entity.insert(Handle::<Mesh>::default());

    entity.with_children(|parent| {
        parent
            .spawn(crate::entities::character::visual_node())
            .with_children(|visual| {
                visual.spawn((
                    PbrBundle {
                        mesh: cube_mesh,
                        material: cube_material,
                        ..default()
                    },
                    crate::entities::character::BlockyCharacter,
                ));

                // A googly eye, twice: a white globe with a pupil parked in front of it.
                for side in [-1.0f32, 1.0] {
                    visual
                        .spawn((
                            PbrBundle {
                                mesh: eye_mesh.clone(),
                                material: eye_material.clone(),
                                transform: Transform::from_xyz(side * eye_spacing, eye_y, eye_z),
                                ..default()
                            },
                            crate::entities::character::BlockyCharacter,
                        ))
                        .with_children(|eye| {
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
                }
            });
    });
}

/// Where a player starts, and where every reset puts them back.
///
/// Delegates to [`crate::entities::roster::formation_position`] so the kickoff
/// shape has exactly one definition. This used to be an even spread along the
/// width of the box; it is now Jeremy Liu's authored 5-a-side formation, which
/// changes the initial state distribution the RL environment samples from. The
/// observation and action layouts are untouched, so existing checkpoints still
/// load — they were simply trained from a different kickoff.
pub fn get_spawn_position(team: Team, index: usize) -> Vec3 {
    crate::entities::roster::formation_position(team, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::config::{PLAYERS_PER_TEAM, FIELD_WIDTH};

    #[test]
    fn spawn_positions_are_on_correct_side() {
        for index in 0..PLAYERS_PER_TEAM {
            let orange = get_spawn_position(Team::Orange, index);
            let blue = get_spawn_position(Team::Blue, index);
            assert!(orange.x < 0.0, "orange should be on -x side");
            assert!(blue.x > 0.0, "blue should be on +x side");
            assert!(orange.x.abs() <= FIELD_WIDTH / 2.0);
        }
    }

    #[test]
    fn spawn_positions_are_distinct_within_team() {
        if PLAYERS_PER_TEAM >= 2 {
            let a = get_spawn_position(Team::Orange, 0);
            let b = get_spawn_position(Team::Orange, 1);
            assert!((a.z - b.z).abs() > 0.01, "teammates must not overlap");
        }
    }
}
