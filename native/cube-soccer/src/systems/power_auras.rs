//! The shell a cube wears while a status effect is on it.
//!
//! `power_vfx` draws the *cast* - the moment a power goes off. This draws the consequence, for as
//! long as it lasts. Without it a freeze is invisible from the broadcast camera: the beam is gone
//! in a fifth of a second and the victim simply stands still, which looks like a player who is not
//! being played rather than one who is frozen.
//!
//! The shell is read off the cube's live speed/accel factors rather than the duration assumed when
//! the power fired, so it stays honest when effects stack or end early: a cube that is frozen *and*
//! boosted shows frozen, because 0.0 is what the physics is using.
//!
//! Cost is three shared materials and one child entity per affected cube - no per-frame allocation.

use bevy::pbr::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::entities::CubePlayer;
use crate::game::config::*;
use crate::rendering::stylized::Unstylised;
use crate::systems::status_effects::StatusEffects;

/// How far past the cube's own size the shell sits.
const AURA_SWELL: f32 = 1.24;
/// Alpha swings this far either side of its base, once per `AURA_PULSE_HZ`.
const AURA_PULSE_DEPTH: f32 = 0.12;
const AURA_PULSE_HZ: f32 = 2.2;

/// What a cube's effects look like from outside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuraClass {
    Frozen,
    Boosted,
    Slowed,
}

impl AuraClass {
    fn index(self) -> usize {
        match self {
            AuraClass::Frozen => 0,
            AuraClass::Boosted => 1,
            AuraClass::Slowed => 2,
        }
    }

    fn colour(self) -> Color {
        match self {
            AuraClass::Frozen => Color::rgb(0.55, 0.90, 1.0),
            AuraClass::Boosted => Color::rgb(1.0, 0.85, 0.25),
            AuraClass::Slowed => Color::rgb(0.65, 0.35, 1.0),
        }
    }

    fn base_alpha(self) -> f32 {
        match self {
            AuraClass::Frozen => 0.42,
            AuraClass::Boosted => 0.26,
            AuraClass::Slowed => 0.32,
        }
    }

    const ALL: [AuraClass; 3] = [AuraClass::Frozen, AuraClass::Boosted, AuraClass::Slowed];
}

/// Shared shell mesh and one material per class.
#[derive(Resource)]
pub struct AuraAssets {
    shell: Handle<Mesh>,
    materials: [Handle<StandardMaterial>; 3],
}

pub fn load_auras(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let make = |materials: &mut Assets<StandardMaterial>, class: AuraClass| {
        materials.add(StandardMaterial {
            base_color: class.colour().with_a(class.base_alpha()),
            emissive: class.colour() * 2.0,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        })
    };
    let materials = [
        make(&mut materials, AuraClass::Frozen),
        make(&mut materials, AuraClass::Boosted),
        make(&mut materials, AuraClass::Slowed),
    ];
    commands.insert_resource(AuraAssets {
        shell: meshes.add(Cuboid::new(
            CUBE_SIZE * AURA_SWELL,
            CUBE_SIZE * AURA_SWELL,
            CUBE_SIZE * AURA_SWELL,
        )),
        materials,
    });
}

/// The shell entity, parented to the cube.
#[derive(Component)]
pub struct StatusAura {
    pub class: AuraClass,
}

/// Classify a cube's cached modifiers.
pub fn aura_class(speed_factor: f32, accel_factor: f32) -> Option<AuraClass> {
    if speed_factor <= 0.01 {
        Some(AuraClass::Frozen)
    } else if speed_factor > 1.05 || accel_factor > 1.05 {
        Some(AuraClass::Boosted)
    } else if speed_factor < 0.95 {
        Some(AuraClass::Slowed)
    } else {
        None
    }
}

/// Add, swap or remove each cube's shell to match its live effects.
pub fn sync_status_auras(
    mut commands: Commands,
    assets: Option<Res<AuraAssets>>,
    players: Query<(Entity, &StatusEffects, Option<&Children>), With<CubePlayer>>,
    auras: Query<&StatusAura>,
) {
    let Some(assets) = assets else { return };

    for (entity, effects, children) in players.iter() {
        let wanted = aura_class(effects.speed_factor, effects.accel_factor);
        let existing = children.and_then(|kids| {
            kids.iter()
                .find_map(|&kid| auras.get(kid).ok().map(|aura| (kid, aura.class)))
        });

        match (existing, wanted) {
            (Some((_, have)), Some(want)) if have == want => {}
            (None, None) => {}
            (existing, wanted) => {
                if let Some((kid, _)) = existing {
                    commands.entity(kid).despawn_recursive();
                }
                if let Some(class) = wanted {
                    commands.entity(entity).with_children(|parent| {
                        parent.spawn((
                            PbrBundle {
                                mesh: assets.shell.clone(),
                                material: assets.materials[class.index()].clone(),
                                ..default()
                            },
                            StatusAura { class },
                            Unstylised,
                            NotShadowCaster,
                            NotShadowReceiver,
                        ));
                    });
                }
            }
        }
    }
}

/// Breathe the shells. Three writes a frame however many cubes wear one, and none when nobody does.
pub fn pulse_status_auras(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Option<Res<AuraAssets>>,
    auras: Query<&StatusAura>,
) {
    let Some(assets) = assets else { return };
    if auras.is_empty() {
        return;
    }
    let wave = (time.elapsed_seconds() * AURA_PULSE_HZ * std::f32::consts::TAU).sin();
    for class in AuraClass::ALL {
        if let Some(material) = materials.get_mut(&assets.materials[class.index()]) {
            let alpha = (class.base_alpha() + wave * AURA_PULSE_DEPTH).clamp(0.05, 1.0);
            material.base_color = material.base_color.with_a(alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aura_class_reads_the_factors() {
        assert_eq!(aura_class(0.0, 1.0), Some(AuraClass::Frozen));
        assert_eq!(aura_class(BOOST_FACTOR, BOOST_FACTOR), Some(AuraClass::Boosted));
        assert_eq!(aura_class(SLOW_FACTOR, 1.0), Some(AuraClass::Slowed));
        assert_eq!(aura_class(1.0, 1.0), None);
    }

    #[test]
    fn freeze_beats_a_stacked_boost() {
        assert_eq!(aura_class(0.0, BOOST_FACTOR), Some(AuraClass::Frozen));
    }

    #[test]
    fn each_class_addresses_its_own_material() {
        let mut seen: Vec<usize> = AuraClass::ALL.iter().map(|c| c.index()).collect();
        seen.sort();
        assert_eq!(seen, vec![0, 1, 2]);
    }
}
