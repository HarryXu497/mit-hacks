use bevy::prelude::*;
use bevy::core_pipeline::tonemapping::Tonemapping;

#[derive(Component)]
pub struct MainCamera;

/// Set CANOPY_CLOSEUP to inspect the characters at model scale. Presentation
/// only: it moves the camera and nothing else, so physics and controls are
/// unaffected and the normal broadcast view is what ships.
fn closeup_transform(target: Vec3) -> Option<Transform> {
    std::env::var("CANOPY_CLOSEUP").ok().map(|_| {
        Transform::from_translation(target + Vec3::new(-2.6, 3.4, 7.0))
            .looking_at(target + Vec3::new(0., 1.4, 0.), Vec3::Y)
    })
}


fn broadcast_transform(target: Vec3) -> Transform {
    // Summit broadcast: the pitch fills the lower frame while the aim point sits
    // above the turf, so the horizon, the ranges behind it and a band of sky stay
    // in shot. Pitched much steeper than this and the mountains crop off-screen.
    Transform::from_translation(target + Vec3::new(0., 44., 78.))
        .looking_at(target + Vec3::new(0., 10., 0.), Vec3::Y)
}


pub fn setup_camera(mut commands: Commands) {
    // Isometric view, slightly elevated
    commands.spawn((
                Camera3dBundle {
            transform: closeup_transform(Vec3::ZERO).unwrap_or(broadcast_transform(Vec3::ZERO)),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: 42.0_f32.to_radians(),
                ..default()
            }),
            // The default filmic curve (TonyMcMapface) desaturates and rolls off
            // highlights for photographic realism. A flat cartoon palette wants
            // its authored chroma delivered intact, so the transform is skipped.
            tonemapping: Tonemapping::None,
            ..default()
        },
        MainCamera,
    ));
}
