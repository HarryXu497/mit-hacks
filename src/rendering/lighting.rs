use bevy::prelude::*;

pub fn setup_lighting(mut commands: Commands) {
    // Strong ambient light (white environment)
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 1000.0,
    });

    // Soft directional light (optional shadows)
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 20000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(10.0, 20.0, 10.0)
            .looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Additional fill light from the opposite side
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_xyz(-10.0, 15.0, -10.0)
            .looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}
