use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::game::config::*;
use crate::systems::display::spawn_digit;

#[derive(Component)]
pub struct Arena;

/// Resource storing digit display entities for updating
#[derive(Resource, Default)]
pub struct ScoreboardDigits {
    pub orange_digits: Vec<Vec<Entity>>, // 2 digits (tens, units)
    pub blue_digits: Vec<Vec<Entity>>,   // 2 digits
    pub timer_digits: Vec<Vec<Entity>>,  // 2 digits
}

pub fn spawn_arena(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wall_material = materials.add(StandardMaterial {
        base_color: WALL_COLOR,
        ..default()
    });

    // Grey floor - same color as the playing field
    let floor_material = materials.add(StandardMaterial {
        base_color: FIELD_COLOR,
        ..default()
    });

    let plane_size = ARENA_WIDTH.max(ARENA_DEPTH);
    // Use a thin cuboid as floor instead of Plane3d for simplicity
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(plane_size, 0.02, plane_size)),
            material: floor_material,
            transform: Transform::from_xyz(0.0, FIELD_HEIGHT, 0.0),
            ..default()
        },
        Arena,
        Collider::cuboid(ARENA_WIDTH / 2.0, 0.1, ARENA_DEPTH / 2.0),
        RigidBody::Fixed,
    ));

    // Back wall
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(ARENA_WIDTH, ARENA_HEIGHT, WALL_THICKNESS)),
            material: wall_material.clone(),
            transform: Transform::from_xyz(0.0, ARENA_HEIGHT / 2.0, -ARENA_DEPTH / 2.0),
            ..default()
        },
        Arena,
        Collider::cuboid(ARENA_WIDTH / 2.0, ARENA_HEIGHT / 2.0, WALL_THICKNESS / 2.0),
        RigidBody::Fixed,
    ));

    // Left wall
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(WALL_THICKNESS, ARENA_HEIGHT, ARENA_DEPTH)),
            material: wall_material.clone(),
            transform: Transform::from_xyz(-ARENA_WIDTH / 2.0, ARENA_HEIGHT / 2.0, 0.0),
            ..default()
        },
        Arena,
        Collider::cuboid(WALL_THICKNESS / 2.0, ARENA_HEIGHT / 2.0, ARENA_DEPTH / 2.0),
        RigidBody::Fixed,
    ));

    // Right wall
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(WALL_THICKNESS, ARENA_HEIGHT, ARENA_DEPTH)),
            material: wall_material.clone(),
            transform: Transform::from_xyz(ARENA_WIDTH / 2.0, ARENA_HEIGHT / 2.0, 0.0),
            ..default()
        },
        Arena,
        Collider::cuboid(WALL_THICKNESS / 2.0, ARENA_HEIGHT / 2.0, ARENA_DEPTH / 2.0),
        RigidBody::Fixed,
    ));

    // Grid material for wall lines
    let grid_material = materials.add(StandardMaterial {
        base_color: GRID_COLOR,
        ..default()
    });

    // === BACK WALL GRID ===
    let back_wall_z = -ARENA_DEPTH / 2.0 + WALL_THICKNESS / 2.0 + 0.01;

    // Vertical lines on back wall
    let num_back_v_lines = (ARENA_WIDTH / FIELD_GRID_SPACING) as i32;
    for i in -num_back_v_lines/2..=num_back_v_lines/2 {
        let x = i as f32 * FIELD_GRID_SPACING;
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(0.05, ARENA_HEIGHT, 0.02)),
                material: grid_material.clone(),
                transform: Transform::from_xyz(x, ARENA_HEIGHT / 2.0, back_wall_z),
                ..default()
            },
            Arena,
        ));
    }

    // Horizontal lines on back wall
    let num_back_h_lines = (ARENA_HEIGHT / FIELD_GRID_SPACING) as i32;
    for i in 0..=num_back_h_lines {
        let y = i as f32 * FIELD_GRID_SPACING;
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(ARENA_WIDTH, 0.05, 0.02)),
                material: grid_material.clone(),
                transform: Transform::from_xyz(0.0, y, back_wall_z),
                ..default()
            },
            Arena,
        ));
    }

    // === LEFT WALL GRID ===
    let left_wall_x = -ARENA_WIDTH / 2.0 + WALL_THICKNESS / 2.0 + 0.01;

    // Vertical lines on left wall
    let num_left_v_lines = (ARENA_DEPTH / FIELD_GRID_SPACING) as i32;
    for i in -num_left_v_lines/2..=num_left_v_lines/2 {
        let z = i as f32 * FIELD_GRID_SPACING;
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(0.02, ARENA_HEIGHT, 0.05)),
                material: grid_material.clone(),
                transform: Transform::from_xyz(left_wall_x, ARENA_HEIGHT / 2.0, z),
                ..default()
            },
            Arena,
        ));
    }

    // Horizontal lines on left wall
    for i in 0..=num_back_h_lines {
        let y = i as f32 * FIELD_GRID_SPACING;
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(0.02, 0.05, ARENA_DEPTH)),
                material: grid_material.clone(),
                transform: Transform::from_xyz(left_wall_x, y, 0.0),
                ..default()
            },
            Arena,
        ));
    }

    // === RIGHT WALL GRID ===
    let right_wall_x = ARENA_WIDTH / 2.0 - WALL_THICKNESS / 2.0 - 0.01;

    // Vertical lines on right wall
    for i in -num_left_v_lines/2..=num_left_v_lines/2 {
        let z = i as f32 * FIELD_GRID_SPACING;
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(0.02, ARENA_HEIGHT, 0.05)),
                material: grid_material.clone(),
                transform: Transform::from_xyz(right_wall_x, ARENA_HEIGHT / 2.0, z),
                ..default()
            },
            Arena,
        ));
    }

    // Horizontal lines on right wall
    for i in 0..=num_back_h_lines {
        let y = i as f32 * FIELD_GRID_SPACING;
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Cuboid::new(0.02, 0.05, ARENA_DEPTH)),
                material: grid_material.clone(),
                transform: Transform::from_xyz(right_wall_x, y, 0.0),
                ..default()
            },
            Arena,
        ));
    }
}

/// Spawn wall-mounted scoreboard on the back wall with 7-segment digits
pub fn spawn_wall_scoreboard(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wall_z = -ARENA_DEPTH / 2.0 + WALL_THICKNESS + 0.15;
    let panel_y = FIELD_HEIGHT + 3.5;  // Lower position, just above field level

    // Create a dark panel for the scoreboard
    let panel_material = materials.add(StandardMaterial {
        base_color: Color::rgb(0.05, 0.05, 0.05),
        ..default()
    });

    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(11.0, 4.5, 0.1)),
            material: panel_material,
            transform: Transform::from_xyz(0.0, panel_y + 0.75, wall_z - 0.1),
            ..default()
        },
        Arena,
    ));

    let digit_scale = 1.5;
    let digit_z = wall_z + 0.1;

    // Orange team score digits (left side) - initial value 0
    let orange_tens = spawn_digit(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(-4.5, panel_y, digit_z),
        CUBE_ORANGE_COLOR,
        digit_scale,
        0, // initial digit
    );
    let orange_units = spawn_digit(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(-3.0, panel_y, digit_z),
        CUBE_ORANGE_COLOR,
        digit_scale,
        0, // initial digit
    );

    // Separator between scores
    let separator_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::WHITE * 2.0,
        ..default()
    });

    // Colon separator (two dots)
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, 0.1, 0.025)),
            material: separator_material.clone(),
            transform: Transform::from_xyz(0.0, panel_y + 0.4, digit_z),
            ..default()
        },
        Arena,
    ));
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Cuboid::new(0.1, 0.1, 0.025)),
            material: separator_material.clone(),
            transform: Transform::from_xyz(0.0, panel_y - 0.4, digit_z),
            ..default()
        },
        Arena,
    ));

    // Blue team score digits (right side) - initial value 0
    let blue_tens = spawn_digit(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(3.0, panel_y, digit_z),
        CUBE_BLUE_COLOR,
        digit_scale,
        0, // initial digit
    );
    let blue_units = spawn_digit(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(4.5, panel_y, digit_z),
        CUBE_BLUE_COLOR,
        digit_scale,
        0, // initial digit
    );

    // Timer digits (top center) - smaller, initial value 15
    let timer_scale = 1.0;
    let timer_y = panel_y + 2.0;

    let timer_tens = spawn_digit(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(-0.6, timer_y, digit_z),
        Color::WHITE,
        timer_scale,
        1, // initial digit (tens of 15)
    );
    let timer_units = spawn_digit(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(0.6, timer_y, digit_z),
        Color::WHITE,
        timer_scale,
        5, // initial digit (units of 15)
    );

    // Store digit entities in resource
    commands.insert_resource(ScoreboardDigits {
        orange_digits: vec![orange_tens, orange_units],
        blue_digits: vec![blue_tens, blue_units],
        timer_digits: vec![timer_tens, timer_units],
    });
}
