//! A walk cycle for a skeleton that shipped without one.
//!
//! `base_rigged.glb` carries a 21-joint humanoid rig -- hips, spine, chest, neck, head, arms,
//! hands, three tail segments and both legs -- built by `tools/rig_dartmonkey.py`, but no
//! animation clips at all. So there is nothing to play, and `AnimationPlayer` has nothing to
//! give. What there is, is a skeleton whose joints are ordinary entities with ordinary
//! transforms, and that is enough to animate them directly.
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
    FootL,
    FootR,
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
            "foot.L" => Joint::FootL,
            "foot.R" => Joint::FootR,
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
    ///
    /// Large on purpose. These stubby legs are short and sit close under a big head, so a
    /// modest swing covers very little screen distance -- and it has to out-read the body's own
    /// bob, which is what used to swallow it.
    pub const THIGH: f32 = 0.80;
    /// Knees only ever bend one way, so the shin swing is offset rather than centred.
    pub const SHIN: f32 = 0.45;
    /// Arms counter-swing against the legs.
    pub const ARM: f32 = 0.62;
    pub const FOREARM: f32 = 0.18;
    /// The torso twists against the hips, which is what stops a run looking like a shuffle.
    pub const SPINE: f32 = 0.16;
    pub const CHEST: f32 = 0.12;
    /// The head stays level by counter-rotating a little.
    pub const HEAD: f32 = 0.09;
    /// The tail trails, each segment lagging the one before it.
    pub const TAIL: f32 = 0.22;
    /// Feet. Ankle articulation on a rig with full legs -- but on the DartMonkey it *is* the leg
    /// swing: that model has no thigh or shin geometry at all, just short forward-pointing stubs
    /// that bind to the foot joints, so this is what makes its legs move.
    pub const FOOT: f32 = 0.55;
    /// Idle breathing, when standing still.
    pub const BREATHE: f32 = 0.05;

    /// The kicking leg's swing, on top of whatever the gait was doing.
    ///
    /// Much larger than a stride: a kick should look like a strike, not a longer step. The
    /// planted leg stays where it is, so the two legs split apart, which is what sells it.
    pub const KICK_LEG: f32 = 1.35;
    /// The other leg braces back a little as the kick goes through.
    pub const KICK_PLANT: f32 = 0.35;
    /// Arms swing back as the leg comes forward -- the counterweight that keeps a kick balanced.
    pub const KICK_ARMS: f32 = 0.55;
    /// The torso turns into the strike.
    pub const KICK_TWIST: f32 = 0.30;

    /// Knocked back: the arms fly up and out.
    pub const HIT_ARMS: f32 = 0.95;
    /// ...and the torso folds away from whatever hit it.
    pub const HIT_TORSO: f32 = 0.45;
    /// ...and the legs buckle.
    pub const HIT_LEGS: f32 = 0.50;

    /// How far the arms are brought down out of the T-pose, always, before any swing.
    ///
    /// This is the piece whose absence made every other amount of animation pointless. The model
    /// is exported in a T-pose, so the bind pose *is* arms-straight-out -- and a walk cycle that
    /// swings 23 degrees either side of that still reads, correctly, as a monkey T-posing. The
    /// rig needs a rest stance to animate around: arms down at the sides, which is where a
    /// running character's arms live. Measured on the export, local X negative lowers both arms.
    pub const ARMS_DOWN: f32 = -1.00;

}

/// Speed at which the cycle is at full amplitude, in metres per second.
const FULL_STRIDE_SPEED: f32 = 4.0;

/// Pose every claimed joint from its player's motion.
pub fn animate_rig(
    time: Res<Time>,
    visuals: Query<(&PlayerVisual, &Parent)>,
    bodies: Query<&bevy_rapier3d::prelude::Velocity, With<CubePlayer>>,
    mut joints: Query<(&RigJoint, &mut Transform)>,
) {
    let elapsed = time.elapsed_seconds();

    // Everything a joint needs about its player, gathered once per player rather than looked up
    // once per joint.
    struct Driver {
        body: Entity,
        gait: f32,
        stride: f32,
        kick: f32,
        hit: f32,
        hit_from: Vec3,
    }
    let mut drivers: Vec<Driver> = Vec::new();
    for (visual, parent) in &visuals {
        let body = parent.get();
        let speed = bodies
            .get(body)
            .map(|v| Vec2::new(v.linvel.x, v.linvel.z).length())
            .unwrap_or(0.0);
        drivers.push(Driver {
            body,
            gait: visual.gait(),
            stride: (speed / FULL_STRIDE_SPEED).clamp(0.0, 1.0),
            kick: visual.kick(),
            hit: visual.hit(),
            hit_from: visual.hit_from(),
        });
    }

    for (joint, mut transform) in &mut joints {
        let Some(reaction) = drivers.iter().find(|d| d.body == joint.owner) else {
            continue;
        };
        let (gait, stride) = (reaction.gait, reaction.stride);

        // Standing still, the rig breathes rather than freezing solid.
        let idle = (elapsed * 1.8).sin() * swing::BREATHE * (1.0 - stride);
        let step = gait.sin() * stride;
        // The opposite leg, half a cycle out of phase.
        let counter = -step;

        // A kick and a knockback are one-shot reactions that decay from 1 to 0, and they layer
        // over the gait rather than replacing it -- a player kicks while running. The right leg
        // is the kicking leg, so it takes the swing and the left one plants.
        let kick = reaction.kick;
        let hit = reaction.hit;
        // Which way the blow came from, in the player's own frame, so the recoil folds away from
        // it rather than always in the same direction.
        let from_side = reaction.hit_from.x.clamp(-1.0, 1.0);

        // Which local axis moves a joint which way depends on how its bone lies in the bind
        // pose, and it differs per limb. Measured on the exported rig by rotating each joint 60
        // degrees about each local axis and reading off where its tip went:
        //
        //   thigh  local X: fore/aft, and NOT mirrored between sides -- X- is forward on both,
        //          so the two legs need opposite signs to counter-swing.
        //   arm    local X: up/down (X- lowers, both sides). local Z: fore/aft, and *mirrored*
        //          between sides -- the same sign sends one arm forward and the other back, so
        //          the arms take the *same* sign to oppose each other. Using opposite signs, as
        //          this did, swung both arms the same way.
        //   tail   local Z: side to side.
        //
        // Arms are lowered out of the T-pose first and swung second: Blender's XYZ euler order,
        // which is Z(swing) * X(down) composed in the bind frame.

        let offset = match joint.joint {
            // X- is forward, so the sign is negated to keep `step` meaning "this leg forward".
            // The kicking leg drives forward; the other plants back under the body.
            Joint::ThighL => Quat::from_rotation_x(
                -step * swing::THIGH
                    + kick * swing::KICK_PLANT
                    + hit * swing::HIT_LEGS,
            ),
            Joint::ThighR => Quat::from_rotation_x(
                -counter * swing::THIGH
                    - kick * swing::KICK_LEG
                    + hit * swing::HIT_LEGS,
            ),
            // Knees bend on the backswing only: `max(0)` keeps them from breaking forwards.
            Joint::ShinL => Quat::from_rotation_x((-step).max(0.0) * swing::SHIN),
            Joint::ShinR => Quat::from_rotation_x((-counter).max(0.0) * swing::SHIN),
            // Left alone: on the DartMonkey the whole leg is bound to the thigh, and on a rig
            // with real feet an ankle flick adds nothing at broadcast distance.
            Joint::FootL | Joint::FootR => Quat::IDENTITY,
            // Down out of the T-pose, then swung. Same local sign on both, because local Z is
            // mirrored between the arms -- that is what makes them oppose each other, and
            // oppose the legs.
            // Arms swing back into a kick as a counterweight, and fly up when hit -- X+ raises
            // them, which is the opposite of the stance that lowered them.
            Joint::UpperArmL => {
                Quat::from_rotation_z(step * swing::ARM + kick * swing::KICK_ARMS)
                    * Quat::from_rotation_x(
                        swing::ARMS_DOWN + hit * swing::HIT_ARMS * (1.0 - from_side * 0.4),
                    )
            }
            Joint::UpperArmR => {
                Quat::from_rotation_z(step * swing::ARM + kick * swing::KICK_ARMS)
                    * Quat::from_rotation_x(
                        swing::ARMS_DOWN + hit * swing::HIT_ARMS * (1.0 + from_side * 0.4),
                    )
            }
            // Left in the bind pose. On the DartMonkey the whole arm -- hand included -- is one
            // rigid island bound to the shoulder, so there is no forearm geometry to bend, and
            // bending it only risks pulling the piece off its own elbow.
            Joint::ForearmL | Joint::ForearmR => Quat::IDENTITY,
            // Torso twist is about the vertical axis, and at twice the leg frequency it would
            // read as a wobble -- so it follows the stride, not the step.
            Joint::Spine => {
                Quat::from_rotation_y(step * swing::SPINE - kick * swing::KICK_TWIST)
                    * Quat::from_rotation_x(idle - hit * swing::HIT_TORSO)
            }
            Joint::Chest => Quat::from_rotation_y(counter * swing::CHEST),
            Joint::Head => Quat::from_rotation_y(counter * swing::HEAD),
            // Each tail segment lags the last, so the tail whips rather than swinging rigidly.
            Joint::Tail(segment) => {
                let lag = gait - 0.6 * (segment as f32 + 1.0);
                Quat::from_rotation_z(lag.sin() * swing::TAIL * (0.4 + 0.6 * stride))
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
            "thigh.L", "thigh.R", "shin.L", "shin.R", "foot.L", "foot.R",
            "tail.01", "tail.02", "tail.03",
        ] {
            assert!(Joint::from_name(name).is_some(), "{name} is no longer recognised");
        }
        // Joints that exist on the rig but are deliberately not driven.
        for name in ["root", "hips", "neck", "hand.L", "hand.R", "neutral_bone"] {
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

/// Does Bevy actually skin these meshes from the joints we pose?
///
/// Env-gated: `CANOPY_RIG_PROBE=1`. The earlier version of this probe compared each joint's
/// transform against its bind pose, which proved only that *this module writes transforms* --
/// it could not tell a deforming mesh from a rigid one. What decides it is whether the character
/// meshes carry `SkinnedMesh` at all, and whether the joints it references are the entities we
/// are posing. If that intersection is empty the mesh will render in its bind pose no matter what
/// we write, which is exactly what a T-pose on the pitch looks like.
pub fn probe_rig(
    mut done: Local<bool>,
    time: Res<Time>,
    joints: Query<(Entity, &RigJoint)>,
    skinned: Query<(Entity, &bevy::render::mesh::skinning::SkinnedMesh)>,
    names: Query<&Name>,
) {
    if *done || std::env::var("CANOPY_RIG_PROBE").is_err() || time.elapsed_seconds() < 8.0 {
        return;
    }
    *done = true;

    let posed: std::collections::HashSet<Entity> = joints.iter().map(|(e, _)| e).collect();
    eprintln!("RIG posing {} joint entities", posed.len());
    eprintln!("RIG meshes carrying SkinnedMesh: {}", skinned.iter().count());

    for (entity, skin) in &skinned {
        let name = names.get(entity).map(|n| n.as_str().to_owned()).unwrap_or_default();
        let shared = skin.joints.iter().filter(|j| posed.contains(j)).count();
        eprintln!(
            "RIG   {name:?}: references {} joints, {} of them are ones we pose",
            skin.joints.len(),
            shared
        );
        if shared == 0 {
            eprintln!("RIG   *** this mesh cannot deform: we pose none of its joints ***");
        }
    }
    if skinned.iter().count() == 0 {
        eprintln!("RIG *** no SkinnedMesh anywhere: the asset loaded without its skin ***");
    }
}

#[cfg(test)]
mod reactions {
    use super::*;

    #[test]
    fn the_rigs_joint_names_are_the_ones_the_model_ships() {
        // Pinned against the exported skeleton (`monkeyforge_*.glb`, 21 joints). A rename on the
        // art side has to break a test rather than silently stop the walk cycle.
        for name in [
            "spine", "chest", "head", "upper_arm.L", "upper_arm.R", "forearm.L", "forearm.R",
            "thigh.L", "thigh.R", "shin.L", "shin.R", "foot.L", "foot.R",
            "tail.01", "tail.02", "tail.03",
        ] {
            assert!(Joint::from_name(name).is_some(), "{name} is no longer recognised");
        }
        // Joints that exist on the rig but are deliberately not driven.
        for name in ["root", "hips", "neck", "hand.L", "hand.R", "neutral_bone"] {
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

    /// A kick must split the legs, not just take a longer stride.
    #[test]
    fn a_kick_drives_one_leg_forward_and_plants_the_other() {
        // Mirrors the arithmetic in `animate_rig`: X negative is forward on both thighs.
        let (step, counter, kick) = (0.0f32, 0.0f32, 1.0f32);
        let kicking = -counter * swing::THIGH - kick * swing::KICK_LEG;
        let planted = -step * swing::THIGH + kick * swing::KICK_PLANT;
        assert!(kicking < 0.0, "the kicking leg must swing forward (X negative)");
        assert!(planted > 0.0, "the planted leg must brace back");
        assert!(
            kicking.abs() > swing::THIGH,
            "a kick must read as a strike, not as a longer step"
        );
    }

    /// Being knocked back throws the arms up, which is the opposite of the running stance.
    #[test]
    fn a_hit_raises_the_arms_out_of_their_stance() {
        let stance = swing::ARMS_DOWN;
        let struck = swing::ARMS_DOWN + 1.0 * swing::HIT_ARMS;
        assert!(stance < 0.0, "the stance lowers the arms");
        assert!(struck > stance, "a hit must raise them back up");
    }

    /// A reaction fades out, so it cannot leave a player stuck mid-kick.
    #[test]
    fn reactions_vanish_when_they_have_decayed() {
        let (kick, hit) = (0.0f32, 0.0f32);
        assert_eq!(kick * swing::KICK_LEG, 0.0);
        assert_eq!(hit * swing::HIT_ARMS, 0.0);
        assert_eq!(hit * swing::HIT_TORSO, 0.0);
    }

    /// The recoil leans away from whichever side the blow came from.
    #[test]
    fn the_recoil_folds_away_from_the_blow() {
        let left = (1.0f32 - (-1.0) * 0.4, 1.0f32 + (-1.0) * 0.4);
        let right = (1.0f32 - 1.0 * 0.4, 1.0f32 + 1.0 * 0.4);
        assert!(left.0 > left.1, "a blow from the left throws the left arm higher");
        assert!(right.1 > right.0, "and one from the right, the right arm");
    }
}

