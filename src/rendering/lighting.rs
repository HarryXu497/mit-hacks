use bevy::prelude::*;

pub fn setup_lighting(mut commands: Commands) {
    // Strong ambient light (white environment)
    commands.insert_resource(AmbientLight {
        color: Color::rgb(0.72, 0.84, 1.0),
        brightness: 260.0,
    });

    // Soft directional light (optional shadows)
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 12000.0,
            color: Color::rgb(1.0, 0.89, 0.70),
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(-12.0, 24.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Additional fill light from the opposite side
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 1800.0,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_xyz(-10.0, 15.0, -10.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}
