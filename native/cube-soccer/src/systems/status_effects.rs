use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;
use crate::entities::CubePlayer;

/// A single durative effect kind applied to a cube.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectKind {
    /// Multiplies the cube's max speed (0.0 = frozen, 1.0 = normal, >1 = boost).
    SpeedFactor(f32),
    /// Multiplies the movement acceleration (the control lerp rate).
    AccelFactor(f32),
    /// A sustained acceleration (m/s^2, mass-independent) applied each tick.
    Force(Vec3),
}

/// A durative effect with a remaining lifetime in seconds.
#[derive(Clone, Copy, Debug)]
pub struct TimedEffect {
    pub kind: EffectKind,
    pub remaining: f32,
}

/// Per-cube status effects + cached net modifiers (recomputed each tick).
#[derive(Component, Debug)]
pub struct StatusEffects {
    effects: Vec<TimedEffect>,
    pub speed_factor: f32,
    pub accel_factor: f32,
    pub net_force: Vec3,
}

impl Default for StatusEffects {
    fn default() -> Self {
        Self { effects: Vec::new(), speed_factor: 1.0, accel_factor: 1.0, net_force: Vec3::ZERO }
    }
}

impl StatusEffects {
    /// Stack a durative effect for `duration` seconds. (Superpower-facing API.)
    pub fn add(&mut self, kind: EffectKind, duration: f32) {
        self.effects.push(TimedEffect { kind, remaining: duration });
    }
    /// Number of active effects (for tests/introspection).
    pub fn active_len(&self) -> usize { self.effects.len() }
}

/// Instantaneous velocity kick (knockback), applied once when fired.
#[derive(Event)]
pub struct ImpulseEvent {
    pub target: Entity,
    pub impulse: Vec3,
}

/// Net (speed_factor, accel_factor, net_force) from active effects.
/// speed/accel multiply (so a 0.0 freeze dominates); forces sum.
pub fn net_modifiers(effects: &[TimedEffect]) -> (f32, f32, Vec3) {
    let mut sf = 1.0;
    let mut af = 1.0;
    let mut nf = Vec3::ZERO;
    for e in effects {
        match e.kind {
            EffectKind::SpeedFactor(f) => sf *= f,
            EffectKind::AccelFactor(f) => af *= f,
            EffectKind::Force(v) => nf += v,
        }
    }
    (sf, af, nf)
}

/// Decrement effect lifetimes, drop expired, and recompute cached net modifiers.
pub fn tick_status_effects(time: Res<Time>, mut query: Query<&mut StatusEffects>) {
    let dt = time.delta_seconds();
    for mut se in query.iter_mut() {
        for eff in se.effects.iter_mut() {
            eff.remaining -= dt;
        }
        se.effects.retain(|eff| eff.remaining > 0.0);
        let (sf, af, nf) = net_modifiers(&se.effects);
        se.speed_factor = sf;
        se.accel_factor = af;
        se.net_force = nf;
    }
}

/// Apply durative forces (as acceleration) and drain one-shot impulses onto velocity.
/// Runs AFTER `apply_player_movement` so external pushes layer on top of control.
pub fn apply_status_forces(
    time: Res<Time>,
    mut impulses: EventReader<ImpulseEvent>,
    mut query: Query<(&StatusEffects, &mut Velocity), With<CubePlayer>>,
) {
    let dt = time.delta_seconds();
    for (se, mut vel) in query.iter_mut() {
        vel.linvel += se.net_force * dt;
    }
    for ev in impulses.read() {
        if let Ok((_, mut vel)) = query.get_mut(ev.target) {
            vel.linvel += ev.impulse;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(kind: EffectKind) -> TimedEffect { TimedEffect { kind, remaining: 1.0 } }

    #[test]
    fn net_modifiers_empty_is_neutral() {
        assert_eq!(net_modifiers(&[]), (1.0, 1.0, Vec3::ZERO));
    }

    #[test]
    fn net_modifiers_multiplies_speed_and_accel() {
        let fx = [e(EffectKind::SpeedFactor(0.5)), e(EffectKind::SpeedFactor(0.5)), e(EffectKind::AccelFactor(2.0))];
        let (sf, af, _) = net_modifiers(&fx);
        assert!((sf - 0.25).abs() < 1e-6);
        assert!((af - 2.0).abs() < 1e-6);
    }

    #[test]
    fn net_modifiers_freeze_dominates() {
        let fx = [e(EffectKind::SpeedFactor(0.0)), e(EffectKind::SpeedFactor(2.0))];
        assert!((net_modifiers(&fx).0 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn net_modifiers_sums_forces() {
        let fx = [e(EffectKind::Force(Vec3::new(1.0, 0.0, 0.0))), e(EffectKind::Force(Vec3::new(0.0, 0.0, 2.0)))];
        assert_eq!(net_modifiers(&fx).2, Vec3::new(1.0, 0.0, 2.0));
    }

    #[test]
    fn default_is_neutral() {
        let s = StatusEffects::default();
        assert_eq!((s.speed_factor, s.accel_factor, s.net_force), (1.0, 1.0, Vec3::ZERO));
        assert_eq!(s.active_len(), 0);
    }

    use bevy::time::TimePlugin;
    use crate::game::Team;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(TimePlugin);
        app.add_event::<ImpulseEvent>();
        app
    }

    #[test]
    fn impulse_event_changes_velocity() {
        let mut app = test_app();
        app.add_systems(Update, apply_status_forces);
        let id = app.world.spawn((
            StatusEffects::default(),
            Velocity::zero(),
            CubePlayer { team: Team::Orange, index: 0, can_jump: true },
        )).id();
        app.world.send_event(ImpulseEvent { target: id, impulse: Vec3::new(5.0, 0.0, 0.0) });
        app.update();
        let v = app.world.get::<Velocity>(id).unwrap();
        assert!(v.linvel.x > 4.0, "impulse should add to velocity, got {:?}", v.linvel);
    }

    #[test]
    fn force_accelerates_cube() {
        let mut app = test_app();
        app.add_systems(Update, (tick_status_effects, apply_status_forces).chain());
        let mut se = StatusEffects::default();
        se.add(EffectKind::Force(Vec3::new(20.0, 0.0, 0.0)), 10.0);
        let id = app.world.spawn((
            se,
            Velocity::zero(),
            CubePlayer { team: Team::Orange, index: 0, can_jump: true },
        )).id();
        for _ in 0..10 { app.update(); }
        let v = app.world.get::<Velocity>(id).unwrap();
        assert!(v.linvel.x > 0.0, "sustained +x force should accelerate the cube, got {:?}", v.linvel);
    }

    #[test]
    fn expired_effect_is_removed_and_neutralized() {
        let mut app = test_app();
        app.add_systems(Update, tick_status_effects);
        let mut se = StatusEffects::default();
        se.add(EffectKind::SpeedFactor(0.0), 0.0001);
        let id = app.world.spawn(se).id();
        for _ in 0..3 { app.update(); }
        let se = app.world.get::<StatusEffects>(id).unwrap();
        assert_eq!(se.active_len(), 0, "effect should have expired");
        assert!((se.speed_factor - 1.0).abs() < 1e-6, "speed_factor back to neutral");
    }
}
