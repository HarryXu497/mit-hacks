use bevy::prelude::*;

/// The one shadow-casting light in the scene. Tagged so the front end can shift
/// it to a low afternoon sun for the menu and back for kickoff.
#[derive(Component)]
pub struct KeyLight;

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

    // Key light. Casts the only shadows in the scene.
    commands.spawn((KeyLight, DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 4800.0,
            color: Color::rgb(1.0, 0.89, 0.70),
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(-12.0, 24.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        // One cascade, bounded to the stadium. Bevy's default is four cascades
        // reaching 1000 units, which renders the whole archipelago and both
        // mountain ranges into four 2048px shadow maps every frame -- for
        // shadows that are never on screen. Confining the map to the bowl the
        // camera actually frames costs nothing visible and makes the shadows on
        // the pitch sharper, because the same texels now cover 160 units of
        // depth instead of 1000. Measured 28 -> 37 fps on integrated graphics.
        cascade_shadow_config: bevy::pbr::CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 30.0,
            maximum_distance: 160.0,
            first_cascade_far_bound: 160.0,
            overlap_proportion: 0.2,
        }
        .into(),
        ..default()
    }));

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
