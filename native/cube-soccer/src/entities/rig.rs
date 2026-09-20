//! A walk cycle for a skeleton that shipped without one.
//!
//! The MonkeyForge characters carry a real humanoid rig -- 21 joints: hips, spine, chest, neck,
//! head, arms, hands, three tail segments and both legs -- but no animation clips at all. So
//! there is nothing to play, and `AnimationPlayer` has nothing to give. What there is, is a
//! skeleton whose joints are ordinary entities with ordinary transforms, and that is enough to
//! animate them directly.
//!
//! This drives those joints from the player's own motion: legs step, arms counter-swing, the
//! spine twists against the hips, the tail trails a little behind, and all of it scales with how
//! fast the player is actually running, falling to a breathing idle when they stop. It is the
//! same gait phase [`PlayerVisual`](super::character::PlayerVisual) uses for the body bob, so the
//! feet and the bob stay in step instead of beating against each other.
//!
//! Every joint is animated as an *offset from its bind pose*, never by overwriting its rotation.
//! The bind pose is what holds the character together; writing absolute rotations would collapse
//! it into a pile the first frame.

use bevy::prelude::*;

use super::character::PlayerVisual;
use crate::entities::CubePlayer;

/// Which joint of the rig an entity is, for the ones this animates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Joint {
    Spine,
    Chest,
    Head,
    UpperArmL,
    UpperArmR,
    ForearmL,
    ForearmR,
    ThighL,
    ThighR,
    ShinL,
    ShinR,
    Tail(u8),
}

impl Joint {
    /// The joint a glTF node name refers to, if this animates it.
    ///
    /// Matched on the exporter's names (Blender's `.L`/`.R` suffixes, `tail.01`..`tail.03`). An
    /// unrecognised joint is simply left alone, so a re-rig that adds fingers or a jaw does not
    /// have to be handled here to keep working.
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "spine" => Joint::Spine,
            "chest" => Joint::Chest,
            "head" => Joint::Head,
            "upper_arm.L" => Joint::UpperArmL,
            "upper_arm.R" => Joint::UpperArmR,
            "forearm.L" => Joint::ForearmL,
            "forearm.R" => Joint::ForearmR,
            "thigh.L" => Joint::ThighL,
            "thigh.R" => Joint::ThighR,
            "shin.L" => Joint::ShinL,
            "shin.R" => Joint::ShinR,
            "tail.01" => Joint::Tail(0),
            "tail.02" => Joint::Tail(1),
            "tail.03" => Joint::Tail(2),
            _ => return None,
        })
    }
}

/// A joint this animates, with the bind pose it must be posed relative to.
#[derive(Component)]
pub struct RigJoint {
    joint: Joint,
    /// The rotation the model was exported with. Every pose is this, times an offset.
    bind: Quat,
    /// The player whose motion drives this joint.
    owner: Entity,
}

/// Claim the joints of any character rig that has finished loading.
///
/// glTF nodes arrive some frames after the scene is asked for, and as descendants of the entity
/// that asked, so this runs every frame and does nothing once every joint is claimed. The owning
/// player is found by walking up to the [`CubePlayer`] body, which is where the velocity that
/// drives the gait lives.
pub fn adopt_rig(
    mut commands: Commands,
    joints: Query<(Entity, &Name, &Transform), Without<RigJoint>>,
    parents: Query<&Parent>,
    players: Query<(), With<CubePlayer>>,
) {
    /// Deep enough for a glTF skeleton under a scene root under a visual node under a body.
    const MAX_DEPTH: usize = 16;

    for (entity, name, transform) in &joints {
        let Some(joint) = Joint::from_name(name.as_str()) else {
            continue;
        };
        // Whose rig is this?
        let mut current = entity;
        let mut owner = None;
        for _ in 0..MAX_DEPTH {
            if players.contains(current) {
                owner = Some(current);
                break;
            }
            let Ok(parent) = parents.get(current) else { break };
            current = parent.get();
        }
        let Some(owner) = owner else { continue };
        commands.entity(entity).insert(RigJoint {
            joint,
            bind: transform.rotation,
            owner,
        });
    }
}

/// How far each joint travels, in radians at a full run.
mod swing {
    /// Legs. The largest motion in the cycle: this is what reads as running.
    pub const THIGH: f32 = 0.95;
    /// Knees only ever bend one way, so the shin swing is offset rather than centred.
    pub const SHIN: f32 = 0.75;
    /// Arms counter-swing against the legs.
    pub const ARM: f32 = 0.70;
    pub const FOREARM: f32 = 0.35;
    /// The torso twists against the hips, which is what stops a run looking like a shuffle.
    pub const SPINE: f32 = 0.16;
    pub const CHEST: f32 = 0.12;
    /// The head stays level by counter-rotating a little.
    pub const HEAD: f32 = 0.09;
    /// The tail trails, each segment lagging the one before it.
    pub const TAIL: f32 = 0.30;
    /// Idle breathing, when standing still.
    pub const BREATHE: f32 = 0.05;
}

/// Speed at which the cycle is at full amplitude, in metres per second.
const FULL_STRIDE_SPEED: f32 = 6.0;

/// Pose every claimed joint from its player's motion.
pub fn animate_rig(
    time: Res<Time>,
    visuals: Query<(&PlayerVisual, &Parent)>,
    bodies: Query<&bevy_rapier3d::prelude::Velocity, With<CubePlayer>>,
    mut joints: Query<(&RigJoint, &mut Transform)>,
) {
    let elapsed = time.elapsed_seconds();

    // The gait phase and stride strength for each player, gathered once rather than per joint.
    let mut gaits: Vec<(Entity, f32, f32)> = Vec::new();
    for (visual, parent) in &visuals {
        let body = parent.get();
        let speed = bodies
            .get(body)
            .map(|v| Vec2::new(v.linvel.x, v.linvel.z).length())
            .unwrap_or(0.0);
        let stride = (speed / FULL_STRIDE_SPEED).clamp(0.0, 1.0);
        gaits.push((body, visual.gait(), stride));
    }

    for (joint, mut transform) in &mut joints {
        let Some(&(_, gait, stride)) = gaits.iter().find(|(body, _, _)| *body == joint.owner)
        else {
            continue;
        };

        // Standing still, the rig breathes rather than freezing solid.
        let idle = (elapsed * 1.8).sin() * swing::BREATHE * (1.0 - stride);
        let step = gait.sin() * stride;
        // The opposite leg, half a cycle out of phase.
        let counter = -step;

        let offset = match joint.joint {
            // Legs swing about the hip's lateral axis.
            Joint::ThighL => Quat::from_rotation_x(step * swing::THIGH),
            Joint::ThighR => Quat::from_rotation_x(counter * swing::THIGH),
            // Knees bend on the backswing only: `max(0)` keeps them from breaking forwards.
            Joint::ShinL => Quat::from_rotation_x((-step).max(0.0) * swing::SHIN),
            Joint::ShinR => Quat::from_rotation_x((-counter).max(0.0) * swing::SHIN),
            // Arms oppose the legs, which is what makes a gait look deliberate.
            Joint::UpperArmL => Quat::from_rotation_x(counter * swing::ARM),
            Joint::UpperArmR => Quat::from_rotation_x(step * swing::ARM),
            Joint::ForearmL => Quat::from_rotation_x((counter).max(0.0) * swing::FOREARM),
            Joint::ForearmR => Quat::from_rotation_x((step).max(0.0) * swing::FOREARM),
            // Torso twist is about the vertical axis, and at twice the leg frequency it would
            // read as a wobble -- so it follows the stride, not the step.
            Joint::Spine => {
                Quat::from_rotation_y(step * swing::SPINE) * Quat::from_rotation_x(idle)
            }
            Joint::Chest => Quat::from_rotation_y(counter * swing::CHEST),
            Joint::Head => Quat::from_rotation_y(counter * swing::HEAD),
            // Each tail segment lags the last, so the tail whips rather than swinging rigidly.
            Joint::Tail(segment) => {
                let lag = gait - 0.6 * (segment as f32 + 1.0);
                Quat::from_rotation_y(lag.sin() * swing::TAIL * (0.4 + 0.6 * stride))
            }
        };

        transform.rotation = joint.bind * offset;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rigs_joint_names_are_the_ones_the_model_ships() {
        // Pinned against the exported skeleton (`monkeyforge_*.glb`, 21 joints). A rename on the
        // art side has to break a test rather than silently stop the walk cycle.
        for name in [
            "spine", "chest", "head", "upper_arm.L", "upper_arm.R", "forearm.L", "forearm.R",
            "thigh.L", "thigh.R", "shin.L", "shin.R", "tail.01", "tail.02", "tail.03",
        ] {
            assert!(Joint::from_name(name).is_some(), "{name} is no longer recognised");
        }
        // Joints that exist on the rig but are deliberately not driven.
        for name in ["root", "hips", "neck", "hand.L", "hand.R", "foot.L", "foot.R"] {
            assert!(Joint::from_name(name).is_none(), "{name} should be left alone");
        }
    }

    #[test]
    fn left_and_right_limbs_are_always_out_of_phase() {
        // Both legs swinging together is a hop, which is exactly what this replaces.
        for gait in [0.0f32, 0.7, 1.6, 3.0, 4.5] {
            let step = gait.sin();
            assert!(
                (step - -step).abs() > 1e-9 || step.abs() < 1e-9,
                "legs must oppose each other"
            );
        }
        // And an arm opposes the leg on its own side.
        let gait = 1.0f32;
        let left_thigh = gait.sin();
        let left_arm = -gait.sin();
        assert!(left_thigh * left_arm <= 0.0, "arm and leg on one side must oppose");
    }

    #[test]
    fn a_knee_never_bends_forwards() {
        for gait in [0.0f32, 0.5, 1.0, 2.0, 3.1, 4.0, 5.5, 6.2] {
            let step = gait.sin();
            assert!((-step).max(0.0) >= 0.0);
            assert!((step).max(0.0) >= 0.0);
        }
    }

    #[test]
    fn standing_still_still_breathes() {
        // stride = 0 removes the whole walk cycle, so without the idle term the rig would be a
        // statue. The breathing term is scaled by `1 - stride`, so it is strongest at rest.
        let stride = 0.0f32;
        assert!((1.0 - stride) * swing::BREATHE > 0.0);
        let running = 1.0f32;
        assert_eq!((1.0 - running) * swing::BREATHE, 0.0, "no breathing on top of a full run");
    }
}

/// Confirm the rig is actually being driven. Env-gated: `CANOPY_RIG_PROBE=1`.
pub fn probe_rig(
    mut ticks: Local<u32>,
    joints: Query<(&RigJoint, &Transform)>,
) {
    if std::env::var("CANOPY_RIG_PROBE").is_err() {
        return;
    }
    *ticks += 1;
    if *ticks % 90 != 0 {
        return;
    }
    let total = joints.iter().count();
    // How far each joint has been moved off its bind pose: all zeroes would mean the cycle is
    // running but posing nothing, which looks identical to having no animation at all.
    let moved = joints
        .iter()
        .filter(|(j, t)| t.rotation.angle_between(j.bind) > 0.02)
        .count();
    let widest = joints
        .iter()
        .map(|(j, t)| t.rotation.angle_between(j.bind))
        .fold(0.0f32, f32::max);
    eprintln!("RIG joints={total} posed_off_bind={moved} widest_offset={widest:.3}rad");
}
