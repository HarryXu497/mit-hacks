use bevy::prelude::*;
use crate::game::config::*;
use crate::entities::{CubePlayer, PlayerInput};
use crate::systems::status_effects::{StatusEffects, EffectKind, ImpulseEvent};

/// Which ability a cube holds. Assignment (who gets which) is a later spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuperpowerKind { BeamBlast, FreezeRay, Boost, Slow }

impl SuperpowerKind {
    /// Seconds between uses.
    pub fn cooldown(self) -> f32 {
        match self {
            SuperpowerKind::BeamBlast => BLAST_COOLDOWN,
            SuperpowerKind::FreezeRay => FREEZE_COOLDOWN,
            SuperpowerKind::Boost => BOOST_COOLDOWN,
            SuperpowerKind::Slow => SLOW_COOLDOWN,
        }
    }

    /// One-hot index for the observation: blast=0, freeze=1, boost=2, slow=3.
    pub fn onehot_index(self) -> usize {
        match self {
            SuperpowerKind::BeamBlast => 0,
            SuperpowerKind::FreezeRay => 1,
            SuperpowerKind::Boost => 2,
            SuperpowerKind::Slow => 3,
        }
    }

    /// The name MonkeyForge calls this power.
    ///
    /// These four powers are the whole list, and this is the string its classifier answers with
    /// (`monkeyforge.powers.registry`, which asserts the same order at import). Keeping the
    /// spelling here rather than in the host means one place to look when a drawn superpower has
    /// to become a real one.
    pub fn slug(self) -> &'static str {
        match self {
            SuperpowerKind::BeamBlast => "beam_blast",
            SuperpowerKind::FreezeRay => "freeze_ray",
            SuperpowerKind::Boost => "boost",
            SuperpowerKind::Slow => "slow",
        }
    }

    /// The power a slug names, or `None` if it names nothing.
    ///
    /// Deliberately not lenient. An unrecognised slug means the classifier and this enum have
    /// drifted apart, and quietly falling back to a default power would hide that — the drawn
    /// superpower would simply come out wrong, with nothing to say why.
    pub fn from_slug(slug: &str) -> Option<Self> {
        [
            SuperpowerKind::BeamBlast,
            SuperpowerKind::FreezeRay,
            SuperpowerKind::Boost,
            SuperpowerKind::Slow,
        ]
        .into_iter()
        .find(|kind| kind.slug() == slug)
    }

    /// The badge for this power, relative to `assets/`.
    ///
    /// Rendered by MonkeyForge and committed, so the HUD has an icon whether or not anything has
    /// been forged this session.
    pub fn badge_path(self) -> String {
        format!("icons/powers/{}.png", self.slug())
    }

    /// Every power, in observation order.
    pub const ALL: [SuperpowerKind; 4] = [
        SuperpowerKind::BeamBlast,
        SuperpowerKind::FreezeRay,
        SuperpowerKind::Boost,
        SuperpowerKind::Slow,
    ];
}

/// Optional component: a cube that has a power. Attach to give a power.
#[derive(Component, Debug)]
pub struct Superpower {
    pub kind: SuperpowerKind,
    pub cooldown_remaining: f32, // seconds until ready; 0.0 = ready
}
impl Superpower {
    pub fn new(kind: SuperpowerKind) -> Self { Self { kind, cooldown_remaining: 0.0 } }
    /// Ready fraction in [0,1] for the observation (1.0 = ready).
    pub fn ready_fraction(&self) -> f32 {
        let cd = self.kind.cooldown();
        if cd <= 0.0 { 1.0 } else { (1.0 - self.cooldown_remaining / cd).clamp(0.0, 1.0) }
    }
}

/// Cube's yaw heading in XZ (from its rotation). Defaults to +Z if degenerate.
pub fn facing_dir(rot: Quat) -> Vec3 {
    let f = rot * Vec3::Z;
    let flat = Vec3::new(f.x, 0.0, f.z);
    if flat.length() < 1e-3 { Vec3::Z } else { flat.normalize() }
}

/// True if `target` is within `range` and within `half_angle_rad` of `facing` (XZ).
pub fn in_cone(caster: Vec3, facing: Vec3, target: Vec3, range: f32, half_angle_rad: f32) -> bool {
    let to = Vec3::new(target.x - caster.x, 0.0, target.z - caster.z);
    let d = to.length();
    if d > range { return false; }
    if d < 1e-3 { return true; }
    facing.normalize_or_zero().dot(to / d) >= half_angle_rad.cos()
}

/// Index of the nearest in-cone point, if any.
pub fn nearest_in_cone(caster: Vec3, facing: Vec3, points: &[Vec3], range: f32, half_angle_rad: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, &p) in points.iter().enumerate() {
        if in_cone(caster, facing, p, range, half_angle_rad) {
            let d = Vec3::new(p.x - caster.x, 0.0, p.z - caster.z).length();
            if best.map_or(true, |(_, bd)| d < bd) { best = Some((i, d)); }
        }
    }
    best.map(|(i, _)| i)
}

/// Horizontal impulse pushing `target` away from `caster`, magnitude `strength`.
pub fn blast_impulse(caster: Vec3, target: Vec3, strength: f32) -> Vec3 {
    Vec3::new(target.x - caster.x, 0.0, target.z - caster.z).normalize_or_zero() * strength
}

/// Count down each cube's power cooldown.
pub fn tick_superpower_cooldowns(time: Res<Time>, mut q: Query<&mut Superpower>) {
    let dt = time.delta_seconds();
    for mut sp in q.iter_mut() {
        if sp.cooldown_remaining > 0.0 {
            sp.cooldown_remaining = (sp.cooldown_remaining - dt).max(0.0);
        }
    }
}

/// Fire superpowers for cubes that request it and are off cooldown. Auto-targets.
/// The three queries are disjoint by component (&mut Superpower vs &mut StatusEffects
/// vs read-only &CubePlayer/&Transform), so they coexist in one system.
pub fn activate_superpowers(
    mut casters: Query<(Entity, &CubePlayer, &Transform, &PlayerInput, &mut Superpower)>,
    others: Query<(Entity, &CubePlayer, &Transform)>,
    mut effects: Query<&mut StatusEffects>,
    mut impulses: EventWriter<ImpulseEvent>,
) {
    for (caster_e, caster, caster_tf, input, mut sp) in casters.iter_mut() {
        if !input.fire || sp.cooldown_remaining > 0.0 {
            continue;
        }
        let cpos = caster_tf.translation;
        let facing = facing_dir(caster_tf.rotation);

        let fired = match sp.kind {
            SuperpowerKind::BeamBlast => {
                let ha = BLAST_HALF_ANGLE_DEG.to_radians();
                let mut hit = false;
                for (e, cp, tf) in others.iter() {
                    if cp.team != caster.team && in_cone(cpos, facing, tf.translation, BLAST_RANGE, ha) {
                        impulses.send(ImpulseEvent { target: e, impulse: blast_impulse(cpos, tf.translation, BLAST_IMPULSE) });
                        hit = true;
                    }
                }
                hit
            }
            SuperpowerKind::FreezeRay => {
                let ha = FREEZE_HALF_ANGLE_DEG.to_radians();
                let opp: Vec<(Entity, Vec3)> = others
                    .iter()
                    .filter(|(_, cp, _)| cp.team != caster.team)
                    .map(|(e, _, tf)| (e, tf.translation))
                    .collect();
                let pts: Vec<Vec3> = opp.iter().map(|(_, p)| *p).collect();
                match nearest_in_cone(cpos, facing, &pts, FREEZE_RANGE, ha) {
                    Some(idx) => {
                        if let Ok(mut se) = effects.get_mut(opp[idx].0) {
                            se.add(EffectKind::SpeedFactor(0.0), FREEZE_SECS);
                        }
                        true
                    }
                    None => false,
                }
            }
            SuperpowerKind::Boost => {
                if let Ok(mut se) = effects.get_mut(caster_e) {
                    se.add(EffectKind::SpeedFactor(BOOST_FACTOR), BOOST_SECS);
                    se.add(EffectKind::AccelFactor(BOOST_FACTOR), BOOST_SECS);
                }
                true
            }
            SuperpowerKind::Slow => {
                let mut best: Option<(Entity, f32)> = None;
                for (e, cp, tf) in others.iter() {
                    if cp.team != caster.team {
                        let d = tf.translation.distance(cpos);
                        if d <= SLOW_RANGE && best.map_or(true, |(_, bd)| d < bd) {
                            best = Some((e, d));
                        }
                    }
                }
                match best {
                    Some((e, _)) => {
                        if let Ok(mut se) = effects.get_mut(e) {
                            se.add(EffectKind::SpeedFactor(SLOW_FACTOR), SLOW_SECS);
                        }
                        true
                    }
                    None => false,
                }
            }
        };

        if fired {
            sp.cooldown_remaining = sp.kind.cooldown();
        }
    }
}

#[cfg(test)]
mod slug_tests {
    use super::*;

    #[test]
    fn every_power_round_trips_through_its_slug() {
        for kind in SuperpowerKind::ALL {
            assert_eq!(
                SuperpowerKind::from_slug(kind.slug()),
                Some(kind),
                "{kind:?} does not survive its own slug"
            );
        }
    }

    #[test]
    fn the_slugs_are_the_four_the_classifier_answers_with() {
        // Pinned literally, because these strings cross a process boundary: MonkeyForge's
        // classifier prints them and this is what reads them back. A rename on either side has to
        // break a test rather than silently produce the wrong power.
        let slugs: Vec<&str> = SuperpowerKind::ALL.iter().map(|k| k.slug()).collect();
        assert_eq!(slugs, vec!["beam_blast", "freeze_ray", "boost", "slow"]);
    }

    #[test]
    fn an_unknown_slug_is_refused_rather_than_defaulted() {
        assert_eq!(SuperpowerKind::from_slug("teleport"), None);
        assert_eq!(SuperpowerKind::from_slug(""), None);
        assert_eq!(SuperpowerKind::from_slug("BeamBlast"), None, "slugs are snake_case");
    }

    #[test]
    fn the_slug_order_is_the_observation_order() {
        // The one-hot index is written into the observation vector, so reordering `ALL` would
        // invalidate every trained checkpoint. This pins the two together.
        for (index, kind) in SuperpowerKind::ALL.iter().enumerate() {
            assert_eq!(kind.onehot_index(), index);
        }
    }

    #[test]
    fn every_badge_points_at_a_committed_asset() {
        for kind in SuperpowerKind::ALL {
            let path = kind.badge_path();
            assert!(path.starts_with("icons/powers/"), "{path}");
            assert!(path.ends_with(".png"), "{path}");
            // Relative to the repo root, which is the working directory the app is launched from
            // and therefore the asset root.
            let on_disk = std::path::Path::new("../../assets").join(&path);
            assert!(on_disk.exists(), "{} is missing", on_disk.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn facing_dir_maps_yaw() {
        assert!((facing_dir(Quat::from_rotation_y(0.0)) - Vec3::Z).length() < 1e-5);
        assert!((facing_dir(Quat::from_rotation_y(FRAC_PI_2)) - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn in_cone_accepts_ahead_rejects_behind_and_far() {
        let c = Vec3::ZERO;
        let f = Vec3::Z;
        let ha = 30f32.to_radians();
        assert!(in_cone(c, f, Vec3::new(0.0, 1.0, 5.0), 8.0, ha), "directly ahead in range");
        assert!(!in_cone(c, f, Vec3::new(0.0, 1.0, -5.0), 8.0, ha), "behind");
        assert!(!in_cone(c, f, Vec3::new(0.0, 1.0, 20.0), 8.0, ha), "out of range");
        assert!(!in_cone(c, f, Vec3::new(10.0, 1.0, 1.0), 8.0, ha), "outside the angle");
    }

    #[test]
    fn nearest_in_cone_picks_closest() {
        let c = Vec3::ZERO;
        let pts = [Vec3::new(0.0, 0.0, 6.0), Vec3::new(0.0, 0.0, 3.0), Vec3::new(0.0, 0.0, -3.0)];
        assert_eq!(nearest_in_cone(c, Vec3::Z, &pts, 8.0, 30f32.to_radians()), Some(1));
    }

    #[test]
    fn blast_impulse_points_away() {
        let imp = blast_impulse(Vec3::ZERO, Vec3::new(0.0, 0.0, 2.0), 25.0);
        assert!(imp.z > 0.0 && imp.x.abs() < 1e-5);
        assert!((imp.length() - 25.0).abs() < 1e-4);
    }

    #[test]
    fn ready_fraction_full_when_off_cooldown() {
        let sp = Superpower::new(SuperpowerKind::BeamBlast);
        assert!((sp.ready_fraction() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn onehot_index_maps_kinds() {
        assert_eq!(SuperpowerKind::BeamBlast.onehot_index(), 0);
        assert_eq!(SuperpowerKind::FreezeRay.onehot_index(), 1);
        assert_eq!(SuperpowerKind::Boost.onehot_index(), 2);
        assert_eq!(SuperpowerKind::Slow.onehot_index(), 3);
    }

    use bevy::time::TimePlugin;
    use bevy_rapier3d::prelude::Velocity;
    use crate::game::Team;
    use crate::entities::{CubePlayer, PlayerInput};
    use crate::systems::status_effects::{StatusEffects, ImpulseEvent};

    fn power_app() -> App {
        let mut app = App::new();
        app.add_plugins(TimePlugin);
        app.add_event::<ImpulseEvent>();
        app.add_systems(Update, (tick_superpower_cooldowns, activate_superpowers).chain());
        app
    }

    fn spawn_cube(app: &mut App, team: Team, index: usize, pos: Vec3, fire: bool) -> Entity {
        app.world.spawn((
            CubePlayer { team, index, can_jump: true },
            Transform::from_translation(pos),
            Velocity::zero(),
            StatusEffects::default(),
            PlayerInput { movement: Vec2::ZERO, jump: false, fire, kick: None },
        )).id()
    }

    #[test]
    fn boost_applies_speed_and_accel_to_self_and_sets_cooldown() {
        let mut app = power_app();
        let e = spawn_cube(&mut app, Team::Orange, 0, Vec3::ZERO, true);
        app.world.entity_mut(e).insert(Superpower::new(SuperpowerKind::Boost));
        app.update();
        let se = app.world.get::<StatusEffects>(e).unwrap();
        assert_eq!(se.active_len(), 2, "boost adds speed + accel effects");
        assert!(app.world.get::<Superpower>(e).unwrap().cooldown_remaining > 0.0, "cooldown started");
    }

    #[test]
    fn freeze_ray_freezes_opponent_in_front() {
        let mut app = power_app();
        let caster = spawn_cube(&mut app, Team::Orange, 0, Vec3::ZERO, true);
        app.world.entity_mut(caster).insert(Superpower::new(SuperpowerKind::FreezeRay));
        let opp = spawn_cube(&mut app, Team::Blue, 0, Vec3::new(0.0, 0.0, 4.0), false);
        app.update();
        assert_eq!(app.world.get::<StatusEffects>(opp).unwrap().active_len(), 1, "opponent frozen");
    }

    #[test]
    fn blast_sends_impulse_to_opponent_in_cone() {
        let mut app = power_app();
        let caster = spawn_cube(&mut app, Team::Orange, 0, Vec3::ZERO, true);
        app.world.entity_mut(caster).insert(Superpower::new(SuperpowerKind::BeamBlast));
        let _opp = spawn_cube(&mut app, Team::Blue, 0, Vec3::new(0.0, 0.0, 3.0), false);
        app.update();
        let events = app.world.resource::<Events<ImpulseEvent>>();
        let mut reader = events.get_reader();
        assert!(reader.read(events).count() >= 1, "blast should emit at least one impulse");
    }

    #[test]
    fn no_target_does_not_consume_cooldown() {
        let mut app = power_app();
        let caster = spawn_cube(&mut app, Team::Orange, 0, Vec3::ZERO, true);
        app.world.entity_mut(caster).insert(Superpower::new(SuperpowerKind::FreezeRay));
        app.update();
        assert_eq!(app.world.get::<Superpower>(caster).unwrap().cooldown_remaining, 0.0, "no target => still ready");
    }

    #[test]
    fn cooldown_blocks_refire() {
        let mut app = power_app();
        let caster = spawn_cube(&mut app, Team::Orange, 0, Vec3::ZERO, true);
        app.world.entity_mut(caster).insert(Superpower::new(SuperpowerKind::Boost));
        app.update();
        let cd_after = app.world.get::<Superpower>(caster).unwrap().cooldown_remaining;
        app.update();
        let se_len = app.world.get::<StatusEffects>(caster).unwrap().active_len();
        assert!(cd_after > 0.0);
        assert!(se_len <= 2, "should not fire a second boost while on cooldown");
    }

    #[test]
    fn no_power_component_means_inert() {
        let mut app = power_app();
        let e = spawn_cube(&mut app, Team::Orange, 0, Vec3::ZERO, true);
        app.update();
        assert_eq!(app.world.get::<StatusEffects>(e).unwrap().active_len(), 0);
    }
}
