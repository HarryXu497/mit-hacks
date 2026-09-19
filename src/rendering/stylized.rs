//! Forward material extension: stepped diffuse lighting and animated river optics.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::*,
};
use std::collections::HashMap;
pub const JUNGLE_SHADER: Handle<Shader> =
    Handle::weak_from_u128(0x65d8a972e05f4ab8a7f7113ea5c00512);

pub fn register_shader(app: &mut App) {
    bevy::asset::load_internal_asset!(
        app,
        JUNGLE_SHADER,
        "../../assets/shaders/jungle.wgsl",
        Shader::from_wgsl
    );
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct JungleSurface {
    #[uniform(100)]
    pub settings: Vec4,
}
impl MaterialExtension for JungleSurface {
    fn fragment_shader() -> ShaderRef {
        JUNGLE_SHADER.into()
    }
}
pub type JungleMaterial = ExtendedMaterial<StandardMaterial, JungleSurface>;

/// Marks a surface that belongs to a figure rather than to the ground: players,
/// the ball, anything the eye must find first. The shader gives these a dark
/// contour and a specular hotspot; nothing else in the scene gets either.
#[derive(Component)]
pub struct ActorSurface;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_score_segments_keep_their_updatable_material() {
        let mut app = App::new();
        app.init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<JungleMaterial>>()
            .add_systems(Update, stylize);
        let handle = app
            .world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let digit = app
            .world
            .spawn((crate::systems::display::DigitSegment, handle.clone()))
            .id();
        let prop = app.world.spawn(handle.clone()).id();
        app.update();
        assert_eq!(
            app.world.get::<Handle<StandardMaterial>>(digit),
            Some(&handle)
        );
        assert!(app.world.get::<Handle<JungleMaterial>>(digit).is_none());
        assert!(app.world.get::<Handle<JungleMaterial>>(prop).is_some());
    }
}

/// Swaps draw materials only. Physics, transforms and scoreboard emissives stay untouched.
pub fn stylize(
    mut commands: Commands,
    source: Res<Assets<StandardMaterial>>,
    mut target: ResMut<Assets<JungleMaterial>>,
    surfaces: Query<
        (Entity, &Handle<StandardMaterial>, Option<&ActorSurface>),
        Without<crate::systems::display::DigitSegment>,
    >,
) {
    let mut cache = HashMap::new();
    for (entity, handle, actor) in &surfaces {
        let Some(base) = source.get(handle) else {
            continue;
        };
        if base.unlit || base.emissive != Color::BLACK {
            continue;
        }
        let is_actor = actor.is_some();
        // Actors and props share source materials (a monkey and a banner are both
        // team orange), so the cache key carries the actor flag too -- otherwise
        // whichever converted first would decide the other's shading.
        let converted = cache
            .entry((handle.id(), is_actor))
            .or_insert_with(|| {
                target.add(JungleMaterial {
                    base: base.clone(),
                    extension: JungleSurface {
                        settings: Vec4::new(
                            if base.perceptual_roughness < 0.3 {
                                1.
                            } else {
                                0.
                            },
                            if is_actor { 1. } else { 0. },
                            0.,
                            0.,
                        ),
                    },
                })
            })
            .clone();
        commands
            .entity(entity)
            .remove::<Handle<StandardMaterial>>()
            .insert(converted);
    }
}
