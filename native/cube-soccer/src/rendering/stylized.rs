//! Forward material extension: stepped diffuse lighting and animated river optics.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::*,
};
use std::collections::HashMap;
pub const JUNGLE_SHADER: Handle<Shader> =
    Handle::weak_from_u128(0x65d8a972e05f4ab8a7f7113ea5c00512);

/// Whether this app can render at all.
///
/// `Assets<Shader>` is registered by Bevy's `RenderPlugin`, not by `AssetPlugin` — so a headless
/// app (the RL environment, and the coaching crate's integration tests, which add `AssetPlugin`
/// and `ScenePlugin` but no renderer) has an asset server and still no shader store. Everything
/// in this module has to be a no-op there rather than a panic.
pub fn is_rendering(app: &App) -> bool {
    app.world.get_resource::<Assets<Shader>>().is_some()
}

/// Load the cel-lighting shader, if there is anything to load it into.
///
/// `load_internal_asset!` reaches straight into `Assets<Shader>` and panics when it is absent,
/// which took down the test that proves ten coached AI players spawn and keep their tactics
/// across resets. A headless app draws nothing, so skipping the shader costs it nothing.
pub fn register_shader(app: &mut App) {
    if !is_rendering(app) {
        return;
    }
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

/// A surface the jungle's cel pass must not touch.
///
/// The step function here is authored for flat-coloured props: a banner, a rock, a painted board.
/// Run over a *textured* character it reads as bands of dark, which is not a look anyone chose --
/// the generated monkeys came out as dark lumps. Their art already has its own shading baked into
/// the texture, so they keep the standard material.
#[derive(Component)]
pub struct Unstylised;

/// Mark a generated character's meshes so the cel pass leaves them alone.
///
/// A glTF scene's meshes are spawned by the asset server some frames after the scene is asked
/// for, and they arrive as descendants rather than on the entity that asked. So this walks up
/// from each unmarked surface looking for a [`CharacterSkin`](crate::entities::CharacterSkin),
/// and tags what it finds. It runs before `stylize` and does nothing once everything is tagged.
pub fn keep_characters_unstylised(
    mut commands: Commands,
    surfaces: Query<Entity, (With<Handle<StandardMaterial>>, Without<Unstylised>)>,
    parents: Query<&Parent>,
    skins: Query<(), With<crate::entities::CharacterSkin>>,
) {
    /// Deep enough for a glTF scene under a visual node under a body; the bound only stops a
    /// malformed hierarchy from spinning here.
    const MAX_DEPTH: usize = 12;

    for surface in &surfaces {
        let mut current = surface;
        for _ in 0..MAX_DEPTH {
            if skins.contains(current) {
                commands.entity(surface).insert(Unstylised);
                break;
            }
            let Ok(parent) = parents.get(current) else { break };
            current = parent.get();
        }
    }
}

/// Whether a surface hangs off a generated character.
///
/// Shares [`MAX_DEPTH`](keep_characters_unstylised)'s reasoning: a glTF scene's meshes arrive as
/// descendants of the entity that asked for the scene, not on it.
fn belongs_to_a_character(
    surface: Entity,
    parents: &Query<&Parent>,
    skins: &Query<(), With<crate::entities::CharacterSkin>>,
) -> bool {
    let mut current = surface;
    for _ in 0..12 {
        if skins.contains(current) {
            return true;
        }
        let Ok(parent) = parents.get(current) else { return false };
        current = parent.get();
    }
    false
}

/// Swaps draw materials only. Physics, transforms and scoreboard emissives stay untouched.
pub fn stylize(
    mut commands: Commands,
    source: Res<Assets<StandardMaterial>>,
    // Walked here as well as in `keep_characters_unstylised`, because the marker that system
    // inserts is a deferred command: on the frame a character's meshes first appear the marker
    // has not landed yet, and without this the cel pass would claim them before it did. Once is
    // enough to lose them -- the swap is permanent.
    parents: Query<&Parent>,
    skins: Query<(), With<crate::entities::CharacterSkin>>,
    // Optional for the same reason `register_shader` checks first: without a renderer there is no
    // `MaterialPlugin<JungleMaterial>` and so no store to put the swapped materials in. Scheduling
    // this system in a headless app is then simply a no-op instead of a panic.
    target: Option<ResMut<Assets<JungleMaterial>>>,
    surfaces: Query<
        (Entity, &Handle<StandardMaterial>, Option<&ActorSurface>),
        (
            Without<crate::systems::display::DigitSegment>,
            Without<Unstylised>,
        ),
    >,
) {
    let Some(mut target) = target else {
        return;
    };
    let mut cache = HashMap::new();
    for (entity, handle, actor) in &surfaces {
        if belongs_to_a_character(entity, &parents, &skins) {
            commands.entity(entity).insert(Unstylised);
            continue;
        }
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
