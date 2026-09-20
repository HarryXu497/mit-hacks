use crate::game::config::*;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

#[derive(Component)]
pub struct Field;

#[derive(Component)]
pub struct FieldBorder;

pub fn spawn_field(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let field_material = materials.add(StandardMaterial {
        base_color: FIELD_COLOR,
        ..default()
    });

    // Main elevated platform (dark grey)
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(FIELD_WIDTH, FIELD_HEIGHT, FIELD_DEPTH)),
            material: field_material.clone(),
            transform: Transform::from_xyz(0.0, FIELD_HEIGHT / 2.0, 0.0),
            ..default()
        },
        Field,
        Collider::cuboid(FIELD_WIDTH / 2.0, FIELD_HEIGHT / 2.0, FIELD_DEPTH / 2.0),
        RigidBody::Fixed,
    ));

    // Side extension platforms (same color as main field)
    // Positive Z side
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(FIELD_WIDTH, FIELD_HEIGHT, SIDE_EXTENSION)),
            material: field_material.clone(),
            transform: Transform::from_xyz(
                0.0,
                FIELD_HEIGHT / 2.0,
                FIELD_DEPTH / 2.0 + SIDE_EXTENSION / 2.0,
            ),
            ..default()
        },
        Field,
        Collider::cuboid(FIELD_WIDTH / 2.0, FIELD_HEIGHT / 2.0, SIDE_EXTENSION / 2.0),
        RigidBody::Fixed,
    ));

    // Negative Z side
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(FIELD_WIDTH, FIELD_HEIGHT, SIDE_EXTENSION)),
            material: field_material,
            transform: Transform::from_xyz(
                0.0,
                FIELD_HEIGHT / 2.0,
                -FIELD_DEPTH / 2.0 - SIDE_EXTENSION / 2.0,
            ),
            ..default()
        },
        Field,
        Collider::cuboid(FIELD_WIDTH / 2.0, FIELD_HEIGHT / 2.0, SIDE_EXTENSION / 2.0),
        RigidBody::Fixed,
    ));

    // Grid lines on the field (white lines)
    let line_material = materials.add(StandardMaterial {
        base_color: Color::rgba(1.0, 1.0, 1.0, 0.5),
        ..default()
    });

    // Vertical lines (along Z axis)
    let num_v_lines = (FIELD_WIDTH / FIELD_GRID_SPACING) as i32;
    for i in -num_v_lines / 2..=num_v_lines / 2 {
        let x = i as f32 * FIELD_GRID_SPACING;
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(0.05, 0.02, FIELD_DEPTH)),
                material: line_material.clone(),
                transform: Transform::from_xyz(x, FIELD_HEIGHT + 0.01, 0.0),
                ..default()
            },
            Field,
        ));
    }

    // Horizontal lines (along X axis)
    let num_h_lines = (FIELD_DEPTH / FIELD_GRID_SPACING) as i32;
    for i in -num_h_lines / 2..=num_h_lines / 2 {
        let z = i as f32 * FIELD_GRID_SPACING;
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(FIELD_WIDTH, 0.02, 0.05)),
                material: line_material.clone(),
                transform: Transform::from_xyz(0.0, FIELD_HEIGHT + 0.01, z),
                ..default()
            },
            Field,
        ));
    }

    // Extended zone limits
    let extended_z = FIELD_DEPTH / 2.0 + SIDE_EXTENSION;

    // Fluorescent material for borders
    let fluorescent_material = materials.add(StandardMaterial {
        base_color: FLUORESCENT_COLOR,
        emissive: FLUORESCENT_COLOR * 2.0, // Glowing effect
        ..default()
    });

    // Fluorescent border walls to keep players on extended field
    let border_height = 1.0;

    // Front border wall (positive Z) - fluorescent
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(FIELD_WIDTH, border_height, 0.1)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(0.0, FIELD_HEIGHT + border_height / 2.0, extended_z),
            ..default()
        },
        Collider::cuboid(FIELD_WIDTH / 2.0, border_height / 2.0, 0.05),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Back border wall (negative Z) - fluorescent
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(FIELD_WIDTH, border_height, 0.1)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(0.0, FIELD_HEIGHT + border_height / 2.0, -extended_z),
            ..default()
        },
        Collider::cuboid(FIELD_WIDTH / 2.0, border_height / 2.0, 0.05),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Side walls for extended zones (X = ±FIELD_WIDTH/2)
    let side_z = FIELD_DEPTH / 2.0 + SIDE_EXTENSION / 2.0;

    // Left side wall, positive Z zone
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, border_height, SIDE_EXTENSION)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(
                -FIELD_WIDTH / 2.0,
                FIELD_HEIGHT + border_height / 2.0,
                side_z,
            ),
            ..default()
        },
        Collider::cuboid(0.05, border_height / 2.0, SIDE_EXTENSION / 2.0),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Right side wall, positive Z zone
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, border_height, SIDE_EXTENSION)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(
                FIELD_WIDTH / 2.0,
                FIELD_HEIGHT + border_height / 2.0,
                side_z,
            ),
            ..default()
        },
        Collider::cuboid(0.05, border_height / 2.0, SIDE_EXTENSION / 2.0),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Left side wall, negative Z zone
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, border_height, SIDE_EXTENSION)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(
                -FIELD_WIDTH / 2.0,
                FIELD_HEIGHT + border_height / 2.0,
                -side_z,
            ),
            ..default()
        },
        Collider::cuboid(0.05, border_height / 2.0, SIDE_EXTENSION / 2.0),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Right side wall, negative Z zone
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, border_height, SIDE_EXTENSION)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(
                FIELD_WIDTH / 2.0,
                FIELD_HEIGHT + border_height / 2.0,
                -side_z,
            ),
            ..default()
        },
        Collider::cuboid(0.05, border_height / 2.0, SIDE_EXTENSION / 2.0),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Walls to close gaps beside goals (between goal posts and Z = ±FIELD_DEPTH/2)
    let side_gap = (FIELD_DEPTH - GOAL_DEPTH) / 2.0;
    let gap_center_z = (FIELD_DEPTH / 2.0 + GOAL_DEPTH / 2.0) / 2.0;

    // Left goal, positive Z side
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, border_height, side_gap)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(
                -FIELD_WIDTH / 2.0,
                FIELD_HEIGHT + border_height / 2.0,
                gap_center_z,
            ),
            ..default()
        },
        Collider::cuboid(0.05, border_height / 2.0, side_gap / 2.0),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Left goal, negative Z side
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, border_height, side_gap)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(
                -FIELD_WIDTH / 2.0,
                FIELD_HEIGHT + border_height / 2.0,
                -gap_center_z,
            ),
            ..default()
        },
        Collider::cuboid(0.05, border_height / 2.0, side_gap / 2.0),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Right goal, positive Z side
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, border_height, side_gap)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(
                FIELD_WIDTH / 2.0,
                FIELD_HEIGHT + border_height / 2.0,
                gap_center_z,
            ),
            ..default()
        },
        Collider::cuboid(0.05, border_height / 2.0, side_gap / 2.0),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Right goal, negative Z side
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, border_height, side_gap)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(
                FIELD_WIDTH / 2.0,
                FIELD_HEIGHT + border_height / 2.0,
                -gap_center_z,
            ),
            ..default()
        },
        Collider::cuboid(0.05, border_height / 2.0, side_gap / 2.0),
        RigidBody::Fixed,
        FieldBorder,
    ));

    // Fluorescent lines at original field limits (Z = ±FIELD_DEPTH/2)
    let line_thickness = 0.1;
    let line_height = 0.03;

    // Positive Z line (original field limit)
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(FIELD_WIDTH, line_height, line_thickness)),
            material: fluorescent_material.clone(),
            transform: Transform::from_xyz(0.0, FIELD_HEIGHT + 0.02, FIELD_DEPTH / 2.0),
            ..default()
        },
        Field,
    ));

    // Negative Z line (original field limit)
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(FIELD_WIDTH, line_height, line_thickness)),
            material: fluorescent_material,
            transform: Transform::from_xyz(0.0, FIELD_HEIGHT + 0.02, -FIELD_DEPTH / 2.0),
            ..default()
        },
        Field,
    ));
}
