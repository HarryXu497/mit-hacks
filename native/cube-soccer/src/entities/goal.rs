use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::game::config::*;

#[derive(Component)]
pub struct Goal {
    pub team: Team,
}

#[derive(Component)]
pub struct GoalSensor {
    pub team: Team,
}

pub fn spawn_goals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Orange goal (left, negative X)
    spawn_single_goal(&mut commands, &mut meshes, &mut materials, Team::Orange, -FIELD_WIDTH / 2.0);

    // Blue goal (right, positive X)
    spawn_single_goal(&mut commands, &mut meshes, &mut materials, Team::Blue, FIELD_WIDTH / 2.0);
}

fn spawn_single_goal(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    team: Team,
    x_position: f32,
) {
    let color = team.goal_color();
    let post_radius = 0.15;
    let y_base = FIELD_HEIGHT;

    // Goal material
    let goal_material = materials.add(StandardMaterial {
        base_color: color,
        metallic: 0.8,
        perceptual_roughness: 0.3,
        ..default()
    });

    // Create a cylinder mesh for posts
    let post_mesh = meshes.add(Cylinder::new(post_radius, GOAL_HEIGHT));

    // Bottom post (front)
    commands.spawn((
        PbrBundle {
            mesh: post_mesh.clone(),
            material: goal_material.clone(),
            transform: Transform::from_xyz(x_position, y_base + GOAL_HEIGHT / 2.0, GOAL_DEPTH / 2.0),
            ..default()
        },
        Goal { team },
        Collider::cylinder(GOAL_HEIGHT / 2.0, post_radius),
        RigidBody::Fixed,
    ));

    // Bottom post (back)
    commands.spawn((
        PbrBundle {
            mesh: post_mesh.clone(),
            material: goal_material.clone(),
            transform: Transform::from_xyz(x_position, y_base + GOAL_HEIGHT / 2.0, -GOAL_DEPTH / 2.0),
            ..default()
        },
        Goal { team },
        Collider::cylinder(GOAL_HEIGHT / 2.0, post_radius),
        RigidBody::Fixed,
    ));

    // Crossbar mesh
    let crossbar_mesh = meshes.add(Cylinder::new(post_radius, GOAL_DEPTH));

    // Crossbar (horizontal)
    commands.spawn((
        PbrBundle {
            mesh: crossbar_mesh,
            material: goal_material.clone(),
            transform: Transform::from_xyz(x_position, y_base + GOAL_HEIGHT, 0.0)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ..default()
        },
        Goal { team },
        Collider::cylinder(GOAL_DEPTH / 2.0, post_radius),
        RigidBody::Fixed,
    ));

    // Goal sensor (trigger for detecting ball entering goal)
    // Position it slightly inside the goal area
    let sensor_offset = if team == Team::Orange { -0.5 } else { 0.5 };
    commands.spawn((
        TransformBundle::from_transform(Transform::from_xyz(
            x_position + sensor_offset,
            y_base + GOAL_HEIGHT / 2.0,
            0.0
        )),
        Collider::cuboid(0.3, GOAL_HEIGHT / 2.0, GOAL_DEPTH / 2.0 - 0.2),
        Sensor,
        GoalSensor { team },
    ));

    // Spawn the goal nets
    spawn_goal_nets(commands, meshes, materials, team, x_position, y_base);
}

/// Spawn realistic goal nets (back, sides, top)
fn spawn_goal_nets(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    team: Team,
    x_position: f32,
    y_base: f32,
) {
    let net_thickness = 0.05;  // Thin net panels
    let direction = if team == Team::Orange { -1.0 } else { 1.0 };
    let back_x = x_position + direction * GOAL_NET_DEPTH;
    let center_x = x_position + direction * GOAL_NET_DEPTH / 2.0;

    // Net material - semi-transparent white
    let net_material = materials.add(StandardMaterial {
        base_color: NET_COLOR,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // 1. Back net (vertical panel at the back of the goal)
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(net_thickness, GOAL_HEIGHT, GOAL_DEPTH)),
            material: net_material.clone(),
            transform: Transform::from_xyz(back_x, y_base + GOAL_HEIGHT / 2.0, 0.0),
            ..default()
        },
        Goal { team },
        Collider::cuboid(net_thickness / 2.0, GOAL_HEIGHT / 2.0, GOAL_DEPTH / 2.0),
        RigidBody::Fixed,
    ));

    // 2. Left side net (Z = +GOAL_DEPTH/2.0)
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(GOAL_NET_DEPTH, GOAL_HEIGHT, net_thickness)),
            material: net_material.clone(),
            transform: Transform::from_xyz(center_x, y_base + GOAL_HEIGHT / 2.0, GOAL_DEPTH / 2.0),
            ..default()
        },
        Goal { team },
        Collider::cuboid(GOAL_NET_DEPTH / 2.0, GOAL_HEIGHT / 2.0, net_thickness / 2.0),
        RigidBody::Fixed,
    ));

    // 3. Right side net (Z = -GOAL_DEPTH/2.0)
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(GOAL_NET_DEPTH, GOAL_HEIGHT, net_thickness)),
            material: net_material.clone(),
            transform: Transform::from_xyz(center_x, y_base + GOAL_HEIGHT / 2.0, -GOAL_DEPTH / 2.0),
            ..default()
        },
        Goal { team },
        Collider::cuboid(GOAL_NET_DEPTH / 2.0, GOAL_HEIGHT / 2.0, net_thickness / 2.0),
        RigidBody::Fixed,
    ));

    // 4. Top net (horizontal panel connecting crossbar to back)
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(GOAL_NET_DEPTH, net_thickness, GOAL_DEPTH)),
            material: net_material,
            transform: Transform::from_xyz(center_x, y_base + GOAL_HEIGHT, 0.0),
            ..default()
        },
        Goal { team },
        Collider::cuboid(GOAL_NET_DEPTH / 2.0, net_thickness / 2.0, GOAL_DEPTH / 2.0),
        RigidBody::Fixed,
    ));
}
