use bevy::prelude::*;

pub fn setup_lighting(mut commands: Commands) {
    // Ambient fill is what flattens chroma fastest, so it stays low: the authored
    // material colours are already the saturated cartoon palette we want on
    // screen, and over-lighting was washing them out (measured sat 0.77 -> 0.49).
    // With the filmic curve gone there is no highlight rolloff either, so the
    // whole rig is exposed to land mid-tones near 0.6 rather than clipping.
    commands.insert_resource(AmbientLight {
        color: Color::rgb(0.72, 0.84, 1.0),
        brightness: 22.0,
    });

    // Soft directional light (optional shadows)
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 4800.0,
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
            illuminance: 700.0,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_xyz(-10.0, 15.0, -10.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}
