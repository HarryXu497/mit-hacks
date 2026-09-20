//! What a superpower looks like when it goes off.
//!
//! The powers used to fire silently: an opponent was flung across the pitch, or froze where they
//! stood, with nothing on screen to say why. Everything here is presentation only -- it reads
//! events and spawns short-lived emissive props, and never touches a collider, a velocity or a
//! status effect. Deleting this module would change how the match *looks* and nothing about how
//! it plays.
//!
//! The look is deliberately arcade rather than realistic, in the spirit of Mario Strikers: hard
//! silhouettes over soft smoke, fully saturated colour, an anticipation pop before the hit, and a
//! decay fast enough that the pitch is legible again within half a second. Each power owns a
//! shape the eye can name -- a cone, a lance, a chevron, a dome -- so which power fired is
//! readable from the broadcast camera without reading the HUD.

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

use crate::game::config::*;
use crate::rendering::stylized::Unstylised;
use crate::systems::superpowers::SuperpowerKind;

/// A power went off. Presentation layer only: nothing that reads this may affect the simulation.
///
/// Carries everything the effect needs by value rather than by entity, because the caster may be
/// gone (reset, despawned) by the time the burst finishes animating.
#[derive(Event, Clone, Copy, Debug)]
pub struct PowerFired {
    pub kind: SuperpowerKind,
    /// Where the power was cast from.
    pub origin: Vec3,
    /// The caster's facing, flattened to the pitch.
    pub facing: Vec3,
    /// Who it landed on, if it needed a target.
    pub target: Option<Vec3>,
    /// The casting team's colour, so a blast reads as *whose* blast it was.
    pub tint: Color,
}

/// How a piece of a burst moves over its life.
#[derive(Clone, Copy, Debug)]
enum Motion {
    /// Grows from `from` to `to` scale and fades: shockwave rings, muzzle flashes.
    Swell { from: Vec3, to: Vec3 },
    /// Snaps to full length almost at once, holds, then fades: beams and lances.
    Lance,
    /// Flies outward along its own local -Z and tumbles: debris and shards.
    Shard { velocity: Vec3, spin: Vec3 },
    /// Circles a point at a fixed radius while rising: frost crystals, boost motes.
    Orbit { centre: Vec3, radius: f32, turns: f32, rise: f32 },
    /// Falls onto a point and flattens: the slow field slamming down.
    Slam { centre: Vec3, drop: f32 },
}

/// One piece of a burst. Despawned when its life runs out.
#[derive(Component)]
pub struct PowerFx {
    elapsed: f32,
    life: f32,
    motion: Motion,
    /// Emissive strength at birth; decays with life so the burst dims as it dies.
    glow: Color,
}

/// Meshes and materials shared by every burst.
///
/// Built once. A burst spawns dozens of props and can fire several times a second across ten
/// players; allocating a fresh mesh and material per prop is what made the old speed trails
/// expensive enough to be cut from the jungle presentation, and this would have been worse.
#[derive(Resource)]
pub struct PowerFxAssets {
    ring: Handle<Mesh>,
    wedge: Handle<Mesh>,
    shard: Handle<Mesh>,
    lance: Handle<Mesh>,
    dome: Handle<Mesh>,
    mote: Handle<Mesh>,
    /// One material per power, plus a white-hot one for muzzle flashes.
    tinted: Vec<(Color, Handle<StandardMaterial>)>,
}

impl PowerFxAssets {
    /// The material for a colour, reusing one already built for it.
    ///
    /// Bursts draw from a fixed palette -- four powers and two team tints -- so this settles to a
    /// handful of entries rather than growing per cast.
    fn material(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        colour: Color,
    ) -> Handle<StandardMaterial> {
        const SAME: f32 = 0.02;
        if let Some((_, handle)) = self
            .tinted
            .iter()
            .find(|(c, _)| c.r().abs_sub(colour.r()) < SAME
                && c.g().abs_sub(colour.g()) < SAME
                && c.b().abs_sub(colour.b()) < SAME)
        {
            return handle.clone();
        }
        // Unlit and additive-looking: an arcade burst should not take shading from the jungle's
        // sun, and `stylize` skips unlit materials so the cel pass leaves these alone too.
        let handle = materials.add(StandardMaterial {
            base_color: colour,
            emissive: colour * 6.0,
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        });
        self.tinted.push((colour, handle.clone()));
        handle
    }
}

/// Small helper so the colour comparison above reads as a distance rather than a branch.
trait AbsSub {
    fn abs_sub(self, other: f32) -> f32;
}
impl AbsSub for f32 {
    fn abs_sub(self, other: f32) -> f32 {
        (self - other).abs()
    }
}

/// Build the shared burst geometry.
pub fn load_power_fx(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    commands.insert_resource(PowerFxAssets {
        // A thin ring lying in the XZ plane, for ground shockwaves. A torus squashed flat by its
        // transform rather than a disc, so the wave reads as an expanding edge and not a puddle.
        ring: meshes.add(Torus::new(0.88, 1.0)),
        // A flat triangle: the unit of a cone blast, fanned out in a ring of them. Built by hand
        // because this Bevy has no triangle primitive.
        wedge: meshes.add(wedge_mesh()),
        shard: meshes.add(Cuboid::new(0.16, 0.16, 0.5)),
        // A unit-length bar along -Z, scaled to reach its target.
        lance: meshes.add(Cuboid::new(0.22, 0.22, 1.0)),
        dome: meshes.add(Sphere::new(1.0).mesh().uv(24, 12)),
        mote: meshes.add(Cuboid::new(0.14, 0.14, 0.14)),
        tinted: Vec::new(),
    });
}

/// A single flat triangle in the XZ plane, pointing down -Z from the origin.
///
/// One unit long and 0.7 wide, so scaling it by a range gives a blade of exactly that reach.
fn wedge_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0.0, 0.0, 0.0], [-0.35, 0.0, -1.0], [0.35, 0.0, -1.0]],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.5, 0.0], [0.0, 1.0], [1.0, 1.0]]);
    // Both windings, so the blade is visible from under the pitch as well as above it.
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 1]));
    mesh
}

/// The colour each power reads as.
///
/// Chosen to be unmistakable at a glance and against jungle green: a red-hot blast, a cold cyan
/// ray, gold for the boost, violet for the slow. These are the same hues the drawn badges use, so
/// the icon on the HUD and the burst on the pitch agree.
///
/// The blast is deliberately red rather than orange. Orange put it within a shade of the boost's
/// gold -- `every_power_has_a_colour_of_its_own` caught it -- and two powers that look alike are
/// two powers a player cannot tell apart mid-match. Red also matches its badge more closely.
fn palette(kind: SuperpowerKind) -> (Color, Color) {
    match kind {
        // (core, edge)
        SuperpowerKind::BeamBlast => (Color::rgb(1.0, 0.90, 0.74), Color::rgb(0.98, 0.22, 0.08)),
        SuperpowerKind::FreezeRay => (Color::rgb(0.86, 0.99, 1.0), Color::rgb(0.24, 0.72, 1.0)),
        SuperpowerKind::Boost => (Color::rgb(1.0, 0.96, 0.80), Color::rgb(1.0, 0.70, 0.16)),
        SuperpowerKind::Slow => (Color::rgb(0.90, 0.80, 1.0), Color::rgb(0.52, 0.26, 0.92)),
    }
}

/// Spawn the burst for every power that fired this frame.
#[allow(clippy::too_many_arguments)]
pub fn spawn_power_fx(
    mut commands: Commands,
    mut fired: EventReader<PowerFired>,
    assets: Option<ResMut<PowerFxAssets>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    // Both absent headless, where there is nothing to draw into.
    let (Some(mut assets), Some(mut materials)) = (assets, materials) else {
        fired.clear();
        return;
    };

    for shot in fired.read() {
        let (core, edge) = palette(shot.kind);
        let ground = flat_at(shot.origin, FIELD_HEIGHT + 0.06);

        // Every power opens on the same anticipation pop at the caster's feet. It is what makes
        // a cast feel struck rather than faded in, and it reads as the team that cast it.
        let pop = assets.ring.clone();
        spawn(
            &mut commands,
            &mut assets,
            &mut materials,
            pop,
            shot.tint,
            Transform::from_translation(ground),
            0.22,
            Motion::Swell {
                from: Vec3::splat(0.2),
                to: Vec3::new(2.6, 1.0, 2.6),
            },
        );

        match shot.kind {
            SuperpowerKind::BeamBlast => blast(&mut commands, &mut assets, &mut materials, shot, core, edge),
            SuperpowerKind::FreezeRay => freeze(&mut commands, &mut assets, &mut materials, shot, core, edge),
            SuperpowerKind::Boost => boost(&mut commands, &mut assets, &mut materials, shot, core, edge),
            SuperpowerKind::Slow => slow(&mut commands, &mut assets, &mut materials, shot, core, edge),
        }
    }
}

/// A cone of light punched out along the caster's facing, with the ground kicked up under it.
fn blast(
    commands: &mut Commands,
    assets: &mut PowerFxAssets,
    materials: &mut Assets<StandardMaterial>,
    shot: &PowerFired,
    core: Color,
    edge: Color,
) {
    let yaw = yaw_of(shot.facing);
    let half = BLAST_HALF_ANGLE_DEG.to_radians();
    let muzzle = shot.origin + shot.facing * 0.9 + Vec3::Y * 0.2;

    // The muzzle flash: white-hot, gone in a tenth of a second.
    let mesh = assets.dome.clone();
    spawn(commands, assets, materials, mesh, core, Transform::from_translation(muzzle), 0.12,
        Motion::Swell { from: Vec3::splat(0.25), to: Vec3::splat(1.5) });

    // The cone itself, built from wedges fanned across the real half-angle so what is drawn is
    // the volume that actually gets hit -- a blast that looks wider than it is teaches the wrong
    // thing about where to stand.
    let blades = 9;
    for i in 0..blades {
        let t = i as f32 / (blades - 1) as f32;
        let angle = yaw + (t * 2.0 - 1.0) * half;
        let mesh = assets.wedge.clone();
        let colour = if i % 2 == 0 { core } else { edge };
        spawn(
            commands, assets, materials, mesh, colour,
            Transform::from_translation(flat_at(shot.origin, FIELD_HEIGHT + 0.5))
                .with_rotation(Quat::from_rotation_y(angle)),
            0.26,
            Motion::Swell {
                from: Vec3::new(0.4, 1.0, 0.2),
                to: Vec3::new(1.5, 1.0, BLAST_RANGE),
            },
        );
    }

    // A shockwave running out along the ground, and debris thrown with it.
    let mesh = assets.ring.clone();
    spawn(commands, assets, materials, mesh, edge,
        Transform::from_translation(flat_at(shot.origin, FIELD_HEIGHT + 0.05)), 0.42,
        Motion::Swell { from: Vec3::splat(0.4), to: Vec3::new(BLAST_RANGE, 1.0, BLAST_RANGE) });

    for i in 0..10 {
        let t = i as f32 / 9.0;
        let angle = yaw + (t * 2.0 - 1.0) * half;
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        let mesh = assets.shard.clone();
        spawn(
            commands, assets, materials, mesh,
            if i % 3 == 0 { core } else { edge },
            Transform::from_translation(shot.origin + Vec3::Y * 0.4)
                .with_rotation(Quat::from_rotation_y(angle)),
            0.5,
            Motion::Shard {
                velocity: out * (9.0 + 5.0 * t) + Vec3::Y * 4.0,
                spin: Vec3::new(7.0 * t, 5.0, 3.0),
            },
        );
    }
}

/// A lance of ice snapped out to the target, which is then caged in crystals.
fn freeze(
    commands: &mut Commands,
    assets: &mut PowerFxAssets,
    materials: &mut Assets<StandardMaterial>,
    shot: &PowerFired,
    core: Color,
    edge: Color,
) {
    let hit = shot.target.unwrap_or(shot.origin + shot.facing * FREEZE_RANGE);
    let from = shot.origin + Vec3::Y * 0.5;
    let to = flat_at(hit, from.y);
    let span = to - from;
    let length = span.length().max(0.001);

    // The beam: a bar from caster to target, pointing down its own -Z so one unit mesh serves
    // any range.
    let mesh = assets.lance.clone();
    spawn(
        commands, assets, materials, mesh, core,
        Transform::from_translation(from + span * 0.5)
            .with_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, span / length))
            .with_scale(Vec3::new(1.0, 1.0, length)),
        0.30,
        Motion::Lance,
    );
    // A second, wider, dimmer pass around it so the beam has an edge rather than being a stripe.
    let mesh = assets.lance.clone();
    spawn(
        commands, assets, materials, mesh, edge,
        Transform::from_translation(from + span * 0.5)
            .with_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, span / length))
            .with_scale(Vec3::new(2.4, 2.4, length)),
        0.22,
        Motion::Lance,
    );

    // The cage: crystals circling the frozen player for as long as the freeze lasts, so the
    // reason they are not moving stays on screen the whole time.
    for i in 0..7 {
        let mesh = assets.shard.clone();
        spawn(
            commands, assets, materials, mesh,
            if i % 2 == 0 { core } else { edge },
            Transform::from_translation(hit),
            FREEZE_SECS,
            Motion::Orbit {
                centre: hit,
                radius: 1.05,
                turns: 1.5 + i as f32 * 0.06,
                rise: 1.2,
            },
        );
    }
    let mesh = assets.ring.clone();
    spawn(commands, assets, materials, mesh, edge,
        Transform::from_translation(flat_at(hit, FIELD_HEIGHT + 0.05)), FREEZE_SECS,
        Motion::Swell { from: Vec3::new(1.6, 1.0, 1.6), to: Vec3::new(1.9, 1.0, 1.9) });
}

/// An afterburner: chevrons shed off the back of the caster while the boost runs.
fn boost(
    commands: &mut Commands,
    assets: &mut PowerFxAssets,
    materials: &mut Assets<StandardMaterial>,
    shot: &PowerFired,
    core: Color,
    edge: Color,
) {
    let yaw = yaw_of(shot.facing);
    let behind = shot.origin - shot.facing * 0.7 + Vec3::Y * 0.45;

    // Three chevrons, staggered down the trail, pointing the way the player is about to go.
    for i in 0..3 {
        let mesh = assets.wedge.clone();
        let back = behind - shot.facing * (i as f32 * 0.5);
        spawn(
            commands, assets, materials, mesh,
            if i == 0 { core } else { edge },
            Transform::from_translation(back)
                // Turned to face backwards, so the wedge trails the player like exhaust.
                .with_rotation(Quat::from_rotation_y(yaw + std::f32::consts::PI)),
            0.34 + i as f32 * 0.06,
            Motion::Swell {
                from: Vec3::new(1.1, 1.0, 0.4),
                to: Vec3::new(0.2, 1.0, 2.6),
            },
        );
    }

    // Motes spiralling up off the player for the length of the boost.
    for i in 0..9 {
        let mesh = assets.mote.clone();
        spawn(
            commands, assets, materials, mesh,
            if i % 3 == 0 { core } else { edge },
            Transform::from_translation(shot.origin),
            BOOST_SECS,
            Motion::Orbit {
                centre: shot.origin,
                radius: 0.75,
                turns: 2.4 + i as f32 * 0.12,
                rise: 2.4,
            },
        );
    }
    let mesh = assets.ring.clone();
    spawn(commands, assets, materials, mesh, core,
        Transform::from_translation(flat_at(shot.origin, FIELD_HEIGHT + 0.05)), 0.3,
        Motion::Swell { from: Vec3::splat(0.3), to: Vec3::new(2.2, 1.0, 2.2) });
}

/// A heavy dome dropped over the target, sinking as it holds them.
fn slow(
    commands: &mut Commands,
    assets: &mut PowerFxAssets,
    materials: &mut Assets<StandardMaterial>,
    shot: &PowerFired,
    core: Color,
    edge: Color,
) {
    let hit = shot.target.unwrap_or(shot.origin);

    let mesh = assets.dome.clone();
    spawn(commands, assets, materials, mesh, edge,
        Transform::from_translation(hit), SLOW_SECS,
        Motion::Slam { centre: hit, drop: 3.5 });

    // Rings settling inward, so the field reads as pressing down rather than expanding.
    for i in 0..3 {
        let mesh = assets.ring.clone();
        spawn(
            commands, assets, materials, mesh,
            if i == 1 { core } else { edge },
            Transform::from_translation(flat_at(hit, FIELD_HEIGHT + 0.05 + i as f32 * 0.02)),
            SLOW_SECS * (0.5 + i as f32 * 0.25),
            Motion::Swell {
                from: Vec3::new(2.8 - i as f32 * 0.4, 1.0, 2.8 - i as f32 * 0.4),
                to: Vec3::new(1.3, 1.0, 1.3),
            },
        );
    }
    for i in 0..6 {
        let mesh = assets.mote.clone();
        spawn(
            commands, assets, materials, mesh, core,
            Transform::from_translation(hit),
            SLOW_SECS,
            Motion::Orbit {
                centre: hit,
                radius: 1.3,
                turns: -0.8 - i as f32 * 0.05,
                rise: -0.4,
            },
        );
    }
}

/// Put one piece of a burst into the world.
#[allow(clippy::too_many_arguments)]
fn spawn(
    commands: &mut Commands,
    assets: &mut PowerFxAssets,
    materials: &mut Assets<StandardMaterial>,
    mesh: Handle<Mesh>,
    colour: Color,
    transform: Transform,
    life: f32,
    motion: Motion,
) {
    let material = assets.material(materials, colour);
    commands.spawn((
        PbrBundle { mesh, material, transform, ..default() },
        PowerFx { elapsed: 0.0, life, motion, glow: colour },
        // No collider, no rigid body: a burst is scenery. And `Unstylised` because the cel pass
        // bands flat colour into stripes, which on a burst reads as a rendering fault.
        Unstylised,
        NotShadowCaster,
    ));
}

/// Marker re-export so a burst never casts a shadow: an unlit prop with a shadow looks solid.
use bevy::pbr::NotShadowCaster;

/// Advance every burst and despawn the spent ones.
pub fn animate_power_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fx: Query<(Entity, &mut PowerFx, &mut Transform, &Handle<StandardMaterial>)>,
) {
    let dt = time.delta_seconds();
    for (entity, mut piece, mut transform, material) in &mut fx {
        piece.elapsed += dt;
        let t = (piece.elapsed / piece.life).clamp(0.0, 1.0);
        if t >= 1.0 {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        match piece.motion {
            Motion::Swell { from, to } => {
                // Eased out, so the burst is largest early and settles: the arcade "pop".
                let e = 1.0 - (1.0 - t) * (1.0 - t);
                transform.scale = from.lerp(to, e);
            }
            Motion::Lance => {
                // Snaps to full width in the first fifth of its life, then thins away.
                let open = (t / 0.2).min(1.0);
                let fade = 1.0 - ((t - 0.2) / 0.8).clamp(0.0, 1.0);
                let width = open * fade;
                transform.scale.x *= 0.0;
                transform.scale.x = width;
                transform.scale.y = width;
            }
            Motion::Shard { velocity, spin } => {
                transform.translation += velocity * dt;
                // Gravity, so debris arcs rather than flying flat.
                transform.translation.y += 0.5 * GRAVITY * dt * dt;
                transform.rotate_local(Quat::from_euler(
                    EulerRot::XYZ,
                    spin.x * dt,
                    spin.y * dt,
                    spin.z * dt,
                ));
            }
            Motion::Orbit { centre, radius, turns, rise } => {
                let angle = t * turns * std::f32::consts::TAU;
                transform.translation = centre
                    + Vec3::new(angle.cos() * radius, rise * t, angle.sin() * radius);
                transform.rotation = Quat::from_rotation_y(-angle);
            }
            Motion::Slam { centre, drop } => {
                // Falls fast, then flattens into a dome hugging the ground.
                let fall = 1.0 - (1.0 - (t / 0.25).min(1.0)).powi(3);
                transform.translation = centre + Vec3::Y * (drop * (1.0 - fall));
                let squash = 0.35 + 0.65 * (1.0 - fall);
                transform.scale = Vec3::new(1.5, 1.5 * squash, 1.5);
            }
        }

        // Dim towards the end of life. The material is shared between pieces of the same colour,
        // so this writes the *brightest* remaining demand rather than each piece stamping its
        // own -- otherwise the last piece to run would decide the colour for all of them.
        if let Some(material) = materials.get_mut(material) {
            let fade = 1.0 - t * t;
            let wanted = piece.glow * (6.0 * fade);
            if wanted.r() + wanted.g() + wanted.b()
                > material.emissive.r() + material.emissive.g() + material.emissive.b()
            {
                material.emissive = wanted;
            }
        }
    }
}

/// Restore each burst material to full brightness before the frame's pieces bid on it.
///
/// `animate_power_fx` keeps the brightest demand across pieces sharing a material, which needs a
/// floor to bid up from; without this reset the brightest cast of the match would pin the colour
/// for every later one.
pub fn reset_power_fx_glow(
    assets: Option<Res<PowerFxAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = assets else { return };
    for (colour, handle) in &assets.tinted {
        if let Some(material) = materials.get_mut(handle) {
            material.emissive = *colour * 0.0;
        }
    }
}

/// The same point, moved to a given height.
fn flat_at(point: Vec3, y: f32) -> Vec3 {
    Vec3::new(point.x, y, point.z)
}

/// The pitch-plane yaw of a facing vector.
fn yaw_of(facing: Vec3) -> f32 {
    let flat = Vec3::new(facing.x, 0.0, facing.z);
    if flat.length() < 1e-3 {
        0.0
    } else {
        flat.x.atan2(flat.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_power_has_a_colour_of_its_own() {
        // Two powers that look alike are two powers a player cannot tell apart mid-match.
        let cores: Vec<[f32; 3]> = SuperpowerKind::ALL
            .iter()
            .map(|k| {
                let (core, _) = palette(*k);
                [core.r(), core.g(), core.b()]
            })
            .collect();
        let edges: Vec<[f32; 3]> = SuperpowerKind::ALL
            .iter()
            .map(|k| {
                let (_, edge) = palette(*k);
                [edge.r(), edge.g(), edge.b()]
            })
            .collect();
        for i in 0..edges.len() {
            for j in (i + 1)..edges.len() {
                let apart: f32 = (0..3).map(|c| (edges[i][c] - edges[j][c]).abs()).sum();
                assert!(
                    apart > 0.4,
                    "{:?} and {:?} read the same",
                    SuperpowerKind::ALL[i],
                    SuperpowerKind::ALL[j]
                );
            }
        }
        assert_eq!(cores.len(), 4);
    }

    #[test]
    fn the_blast_cone_is_drawn_no_wider_than_it_hits() {
        // The wedges are fanned across `BLAST_HALF_ANGLE_DEG` and scaled to `BLAST_RANGE`, so
        // what is drawn is the volume `in_cone` tests. A burst that oversells its reach would
        // teach players to stand in a place that is not actually safe.
        let half = BLAST_HALF_ANGLE_DEG.to_radians();
        let facing = Vec3::Z;
        let edge = Vec3::new(half.sin(), 0.0, half.cos()) * (BLAST_RANGE - 0.1);
        assert!(crate::systems::superpowers::in_cone(
            Vec3::ZERO, facing, edge, BLAST_RANGE, half
        ));
        let beyond = Vec3::new(half.sin(), 0.0, half.cos()) * (BLAST_RANGE + 1.0);
        assert!(!crate::systems::superpowers::in_cone(
            Vec3::ZERO, facing, beyond, BLAST_RANGE, half
        ));
    }

    #[test]
    fn yaw_of_a_degenerate_facing_does_not_produce_nan() {
        assert_eq!(yaw_of(Vec3::Y), 0.0);
        assert_eq!(yaw_of(Vec3::ZERO), 0.0);
        assert!((yaw_of(Vec3::Z) - 0.0).abs() < 1e-5);
        assert!((yaw_of(Vec3::X) - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }
}
