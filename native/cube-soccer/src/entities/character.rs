//! What a player looks like, and how that look moves.
//!
//! Players are simulated as cubes and always will be: the collider, mass and locked axes in
//! `cube_player.rs` are what the physics and the trained policies depend on. This module only
//! changes what a player *looks* like, by hanging a visual off that same body.
//!
//! Keeping the two apart is deliberate and is what makes this safe to land:
//!
//! * The collider stays `Collider::cuboid(CUBE_SIZE/2, ..)`. No contact behaviour changes.
//! * `OBSERVATION_SIZE` and the observation vector are untouched, so every existing PPO
//!   checkpoint stays valid.
//! * With no model file present the game falls back to the original cube, so a checkout without
//!   assets still runs -- and the cube animates too, because the animation lives on the visual
//!   node rather than on the model.
//!
//! Models are produced by MonkeyForge from a player's drawing and exported one unit tall standing
//! on the origin, so the only scaling needed here is by `CUBE_SIZE`. The model is chosen per
//! *team*, not per player: one person draws one character and their whole side wears it.

use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;

use crate::entities::cube_player::PlayerInput;
use crate::game::config::{Team, CUBE_SIZE};
use crate::systems::status_effects::ImpulseEvent;

/// Path to each team's character model, relative to `assets/`.
///
/// `None` means "no model for this team": the player keeps the procedural cube. That is the
/// default state of a fresh checkout, since the models are generated rather than committed
/// wholesale.
pub fn skin_path(team: Team) -> Option<&'static str> {
    match team {
        Team::Orange => Some("characters/orange.glb#Scene0"),
        Team::Blue => Some("characters/blue.glb#Scene0"),
    }
}

/// Marker for a generated model hanging off a player body.
#[derive(Component)]
pub struct CharacterSkin;

// --- Animation tuning --------------------------------------------------------------------------
// Offsets are in cube units and are scaled by CUBE_SIZE when applied, so the cube and a generated
// character move by the same visible amount.

/// Radians of gait phase per metre travelled. Phase advances with distance, not wall time, so the
/// step rate follows the speed instead of sliding against it.
const GAIT_PER_METRE: f32 = 1.8;
/// Horizontal speed at which the gait reaches full amplitude.
const FULL_STRIDE_SPEED: f32 = 6.0;
/// Peak bob height at full stride.
const BOB_HEIGHT: f32 = 0.055;
/// Shoulder roll that rides along with the stride.
const SWAY: f32 = 0.05;
/// Amplitude and rate of the standing-still breath.
const IDLE_RISE: f32 = 0.018;
const IDLE_RATE: f32 = 1.6;
/// Forward pitch at full speed, and bank per rad/s of turn.
const LEAN_PITCH: f32 = 0.22;
const LEAN_BANK: f32 = 0.06;
/// How fast lean eases toward its target, per second.
const LEAN_EASE: f32 = 8.0;
/// Downward speed past which an arrested fall counts as a landing.
const LANDING_SPEED: f32 = 3.0;
/// How far a landing flattens the visual, and how fast that recovers.
const SQUASH_DEPTH: f32 = 0.22;
const SQUASH_DECAY: f32 = 6.0;
/// The kick: forward pitch, forward reach, and decay.
const KICK_PITCH: f32 = 0.45;
const KICK_REACH: f32 = 0.18;
const KICK_DECAY: f32 = 4.5;
/// Taking a hit: recoil rotation, shake amplitude and rate, and decay.
const HIT_RECOIL: f32 = 0.5;
const HIT_SHAKE: f32 = 0.09;
const HIT_SHAKE_RATE: f32 = 34.0;
const HIT_DECAY: f32 = 3.0;

/// The node every player visual hangs from, and the state its animation needs.
///
/// Physics owns the player body's `Transform`: Rapier writes its position each step and
/// `movement.rs` yaws it to face the direction of travel. Nothing here may touch that. So all
/// animation happens on this child node, in the body's local frame -- where `+Z` is forward,
/// because that is the axis `movement.rs` turns into the velocity and the axis the googly eyes
/// are mounted on.
///
/// Because the body already yaws, this node deliberately does *not* turn. It leans, banks, bobs,
/// squashes, lunges and recoils; adding yaw here would double the turn.
#[derive(Component)]
pub struct PlayerVisual {
    /// Where this visual sits when nothing is happening, in the body's frame.
    rest_y: f32,
    /// Uniform scale the visual needs at rest: `CUBE_SIZE` for a one-unit glTF, `1.0` for the
    /// cube mesh, which is already full size.
    rest_scale: f32,
    /// Gait phase, in radians.
    gait: f32,
    /// Smoothed `(bank, pitch)`, so a direction change eases instead of snapping.
    lean: Vec2,
    /// One-shot reactions, each decaying from 1.0 to 0.0.
    kick: f32,
    hit: f32,
    /// Local-space direction the last hit came from.
    hit_from: Vec3,
    /// Landing squash, set when a fall is arrested.
    squash: f32,
    /// Previous vertical velocity, for spotting that landing.
    prev_vy: f32,
    /// Edge detection for `PlayerInput::fire`, so holding the key is one kick, not many.
    was_firing: bool,
}

impl PlayerVisual {
    pub fn new(rest_y: f32, rest_scale: f32) -> Self {
        Self {
            rest_y,
            rest_scale,
            gait: 0.0,
            lean: Vec2::ZERO,
            kick: 0.0,
            hit: 0.0,
            hit_from: Vec3::ZERO,
            squash: 0.0,
            prev_vy: 0.0,
            was_firing: false,
        }
    }

    /// The transform this visual should have, given where it is in its animation.
    ///
    /// Split out from the system so the arithmetic is testable without a `World`.
    fn compose(&self, stride: f32, elapsed: f32) -> Transform {
        let bob = if stride > 0.01 {
            // Two footfalls per stride, so the rise happens twice per cycle.
            (self.gait * 2.0).sin().abs() * BOB_HEIGHT * stride
        } else {
            (elapsed * IDLE_RATE).sin() * IDLE_RISE
        };
        let sway = self.gait.sin() * SWAY * stride;
        let squash = self.squash * SQUASH_DEPTH;
        let shake = (elapsed * HIT_SHAKE_RATE).sin() * HIT_SHAKE * self.hit;

        let pitch = -self.lean.y - self.kick * KICK_PITCH + self.hit * HIT_RECOIL * self.hit_from.z;
        let bank = self.lean.x + sway - self.hit * HIT_RECOIL * self.hit_from.x;

        Transform {
            translation: Vec3::new(
                shake * CUBE_SIZE,
                self.rest_y + (bob - squash * 0.5) * CUBE_SIZE,
                self.kick * KICK_REACH * CUBE_SIZE,
            ),
            rotation: Quat::from_rotation_x(pitch) * Quat::from_rotation_z(bank),
            scale: Vec3::new(
                self.rest_scale * (1.0 + squash * 0.5),
                self.rest_scale * (1.0 - squash),
                self.rest_scale * (1.0 + squash * 0.5),
            ),
        }
    }
}

/// Attach the generated model to a player, returning whether one was attached.
///
/// The caller uses the answer to decide whether to spawn the cube and its googly eyes instead: a
/// generated monkey has its own eyes painted into its texture, and adding spheres on top of them
/// looks like a bug.
pub fn spawn_skin(parent: &mut ChildBuilder, asset_server: &AssetServer, team: Team) -> bool {
    let Some(path) = skin_path(team) else {
        return false;
    };

    // The model is exported standing on y=0 one unit tall, while the body it hangs from is centred
    // on its own origin -- so drop it half a cube to put its feet at the cube's bottom face rather
    // than at its middle.
    let visual = PlayerVisual::new(-CUBE_SIZE / 2.0, CUBE_SIZE);
    let rest = visual.compose(0.0, 0.0);

    parent
        .spawn((visual, CharacterSkin, SpatialBundle::from_transform(rest)))
        .with_children(|skin| {
            skin.spawn(SceneBundle {
                scene: asset_server.load(path),
                ..default()
            });
        });
    true
}

/// Animate every player visual from the state of the body it hangs off.
///
/// Reads velocity, fire input and knockback events; writes only the visual node's `Transform`.
/// It never touches a body, so it cannot perturb the simulation -- which is also why it is safe
/// to leave out of the headless RL schedule entirely.
pub fn animate_player_visual(
    time: Res<Time>,
    mut impulses: EventReader<ImpulseEvent>,
    bodies: Query<(&Velocity, &PlayerInput, &GlobalTransform)>,
    mut visuals: Query<(&Parent, &mut PlayerVisual, &mut Transform)>,
) {
    let dt = time.delta_seconds();
    if dt <= 0.0 {
        return;
    }
    let elapsed = time.elapsed_seconds();

    // Knockbacks arrive as events addressed to a body, so collect them once rather than trying to
    // re-read per visual -- an `EventReader` is drained by the first pass.
    let knocks: Vec<(Entity, Vec3)> = impulses.read().map(|e| (e.target, e.impulse)).collect();

    for (parent, mut visual, mut transform) in visuals.iter_mut() {
        let body = parent.get();
        let Ok((velocity, input, global)) = bodies.get(body) else {
            continue;
        };

        let speed = Vec2::new(velocity.linvel.x, velocity.linvel.z).length();
        let stride = (speed / FULL_STRIDE_SPEED).clamp(0.0, 1.0);

        visual.gait = (visual.gait + speed * dt * GAIT_PER_METRE) % std::f32::consts::TAU;

        // Rising edge only: holding fire is one lunge, not one per frame.
        if input.fire && !visual.was_firing {
            visual.kick = 1.0;
        }
        visual.was_firing = input.fire;

        for (target, impulse) in &knocks {
            if *target == body {
                visual.hit = 1.0;
                let rotation = global.compute_transform().rotation;
                visual.hit_from = rotation.inverse() * impulse.normalize_or_zero();
            }
        }

        // A landing is a fall that stopped being a fall this frame.
        if visual.prev_vy < -LANDING_SPEED && velocity.linvel.y > -LANDING_SPEED * 0.3 {
            visual.squash = (-visual.prev_vy / (LANDING_SPEED * 3.0)).clamp(0.0, 1.0);
        }
        visual.prev_vy = velocity.linvel.y;

        visual.kick = (visual.kick - dt * KICK_DECAY).max(0.0);
        visual.hit = (visual.hit - dt * HIT_DECAY).max(0.0);
        visual.squash = (visual.squash - dt * SQUASH_DECAY).max(0.0);

        // The body already yaws to face travel, so the lean left to do here is a forward pitch
        // that grows with speed, plus a bank into whichever way the body is turning.
        let target = Vec2::new(velocity.angvel.y * LEAN_BANK, stride * LEAN_PITCH);
        visual.lean = visual.lean.lerp(target, (dt * LEAN_EASE).clamp(0.0, 1.0));

        *transform = visual.compose(stride, elapsed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_team_resolves_to_a_distinct_model() {
        assert_ne!(
            skin_path(Team::Orange),
            skin_path(Team::Blue),
            "teams must be tellable apart on the pitch"
        );
    }

    #[test]
    fn skin_paths_name_a_scene_inside_the_gltf() {
        for team in [Team::Orange, Team::Blue] {
            if let Some(path) = skin_path(team) {
                assert!(path.ends_with("#Scene0"), "bevy needs the scene label: {path}");
                assert!(path.starts_with("characters/"), "models live in assets/characters");
            }
        }
    }

    #[test]
    fn a_resting_visual_sits_exactly_at_its_rest_pose() {
        let visual = PlayerVisual::new(-CUBE_SIZE / 2.0, CUBE_SIZE);
        // elapsed 0 puts the idle breath at sin(0) = 0, so this is the true rest pose.
        let t = visual.compose(0.0, 0.0);
        assert_eq!(t.translation, Vec3::new(0.0, -CUBE_SIZE / 2.0, 0.0));
        assert_eq!(t.scale, Vec3::splat(CUBE_SIZE));
        assert_eq!(t.rotation, Quat::IDENTITY);
    }

    #[test]
    fn the_cube_fallback_rests_unscaled_and_centred() {
        let t = PlayerVisual::new(0.0, 1.0).compose(0.0, 0.0);
        assert_eq!(t.translation, Vec3::ZERO);
        assert_eq!(t.scale, Vec3::ONE);
    }

    #[test]
    fn a_standing_player_still_breathes() {
        let visual = PlayerVisual::new(0.0, 1.0);
        // A quarter of the way through the idle cycle is the peak of the breath.
        let peak = visual.compose(0.0, std::f32::consts::FRAC_PI_2 / IDLE_RATE);
        assert!(peak.translation.y > 0.0, "a still player should not be frozen solid");
        assert!(peak.translation.y < 0.05 * CUBE_SIZE, "the breath should be subtle");
    }

    #[test]
    fn running_leans_forward_and_never_yaws() {
        let mut visual = PlayerVisual::new(0.0, 1.0);
        visual.lean = Vec2::new(0.0, LEAN_PITCH);
        let t = visual.compose(1.0, 0.0);
        let (yaw, pitch, _) = t.rotation.to_euler(EulerRot::YXZ);
        assert!(pitch < 0.0, "a running player leans into the run");
        assert!(
            yaw.abs() < 1e-5,
            "the body already yaws in movement.rs -- the visual must not double it"
        );
    }

    #[test]
    fn a_kick_lunges_forward() {
        let mut visual = PlayerVisual::new(0.0, 1.0);
        visual.kick = 1.0;
        let t = visual.compose(0.0, 0.0);
        assert!(t.translation.z > 0.0, "+Z is forward, where the eyes face");
        let (_, pitch, _) = t.rotation.to_euler(EulerRot::YXZ);
        assert!(pitch < -KICK_PITCH * 0.5, "the kick pitches the body over the ball");
    }

    #[test]
    fn a_landing_flattens_and_spreads() {
        let mut visual = PlayerVisual::new(0.0, 1.0);
        visual.squash = 1.0;
        let t = visual.compose(0.0, 0.0);
        assert!(t.scale.y < 1.0, "a landing squashes");
        assert!(t.scale.x > 1.0 && t.scale.z > 1.0, "and spreads");
        assert!(t.translation.y < 0.0, "and drops toward the ground");
    }

    #[test]
    fn a_hit_recoils_away_from_where_it_came_from() {
        let mut visual = PlayerVisual::new(0.0, 1.0);
        visual.hit = 1.0;
        visual.hit_from = Vec3::Z; // struck from in front
        let front = visual.compose(0.0, 0.0);
        visual.hit_from = -Vec3::Z; // struck from behind
        let back = visual.compose(0.0, 0.0);
        let (_, front_pitch, _) = front.rotation.to_euler(EulerRot::YXZ);
        let (_, back_pitch, _) = back.rotation.to_euler(EulerRot::YXZ);
        assert!(
            front_pitch * back_pitch < 0.0,
            "being hit from the front and from behind must not look the same"
        );
    }

    #[test]
    fn reactions_decay_back_to_rest() {
        let mut visual = PlayerVisual::new(0.0, 1.0);
        visual.kick = 1.0;
        visual.hit = 1.0;
        visual.squash = 1.0;
        // One second is longer than every decay rate above needs to reach zero.
        let dt = 1.0;
        visual.kick = (visual.kick - dt * KICK_DECAY).max(0.0);
        visual.hit = (visual.hit - dt * HIT_DECAY).max(0.0);
        visual.squash = (visual.squash - dt * SQUASH_DECAY).max(0.0);
        let t = visual.compose(0.0, 0.0);
        assert_eq!(t.translation, Vec3::ZERO);
        assert_eq!(t.scale, Vec3::ONE);
    }
}
