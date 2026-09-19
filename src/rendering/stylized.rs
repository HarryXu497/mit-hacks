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
        (Entity, &Handle<StandardMaterial>),
        Without<crate::systems::display::DigitSegment>,
    >,
) {
    let mut cache = HashMap::new();
    for (entity, handle) in &surfaces {
        let Some(base) = source.get(handle) else {
            continue;
        };
        if base.unlit || base.emissive != Color::BLACK {
            continue;
        }
        let converted = cache
            .entry(handle.id())
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
                            0.,
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
