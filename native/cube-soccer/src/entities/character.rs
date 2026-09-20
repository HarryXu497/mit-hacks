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
//!
//! # What a player looks like
//!
//! Three tiers, each replacing the one below it, all hanging off the same animated
//! [`PlayerVisual`] node so they move identically:
//!
//! 1. **A generated model** — MonkeyForge turns a player's drawing into a GLB, and the whole team
//!    wears it. Chosen per *team*, not per player: one person draws one character and their side
//!    wears it. Shown only once the asset has genuinely loaded (see [`CharacterSkin`]).
//! 2. **The jungle's blocky character** — `jungle::monkey`, eleven blocks with ink outlines.
//!    Everything it spawns is tagged [`BlockyCharacter`] so tier 1 can take it off in one pass.
//! 3. **The cube and its googly eyes** — what `cube_player.rs` spawns, and all you get with no
//!    jungle and no model. Also tagged, for the same reason.
//!
//! Tier 1 starts as the undressed [`BASE_CHARACTER`], which is committed, so the fallback exists
//! even on a machine that can reach no GPU. Models stand on y=0 and are
//! [`CHARACTER_MODEL_HEIGHT`] tall in their own space, so the scene child is scaled to the
//! body's height and dropped half a cube to put its feet on the body's bottom face. The node
//! itself stays neutral, because tiers 2 and 3 are already authored at body scale.
//!
//! A generated character also keeps the *standard* material: the jungle's cel step is authored
//! for flat-coloured props and turns textured character art into bands of dark. See
//! `rendering::stylized::Unstylised`.

use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;

use crate::entities::cube_player::{CubePlayer, PlayerInput};
use crate::game::config::{Team, CUBE_SIZE};
use crate::systems::status_effects::ImpulseEvent;

/// The undressed base monkey, which MonkeyForge dresses to make a character.
///
/// Everyone wears this until something better has been forged for their team, including when the
/// GPU box that does the forging is unreachable. It is committed to the repo precisely so that
/// fallback always exists.
/// The base monkey, with the skeleton `tools/rig_dartmonkey.py` gave it.
///
/// `base.glb` beside it is the original export: the same artwork, but a single rigid mesh with no
/// skin and no joints, so nothing could animate it beyond bouncing the whole body. This is that
/// mesh bound to a 21-joint humanoid armature, same vertices, same textures, same 1.4-unit
/// height and origin -- which is what lets [`rig`](super::rig) walk it.
pub const BASE_CHARACTER: &str = "characters/base_rigged.glb#Scene0";

/// The original, unrigged export. Kept as the source the rig is built from.
pub const BASE_CHARACTER_UNRIGGED: &str = "characters/base.glb#Scene0";


/// The monkey a side takes the field in until its coach forges one of their own.
///
/// This is [`BASE_CHARACTER`] for both sides. There are `characters/orange.glb` and
/// `characters/blue.glb` beside it that look like they ought to be used here -- the base monkey
/// with a team garment already on -- but they are broken exports and must not be: measured, they
/// stand 17.7 units tall against this model's 1.4, sit from y=-11.6 to +6.0 instead of 0 to 1.4,
/// and carry a sub-mesh reaching out to z=-24.6. Worn, they render as oversized fragments in the
/// wrong place. Re-exporting them at the base model's scale and origin is what would make them
/// usable; until then the clean base monkey is what plays.
pub fn dressed_character(_team: Team) -> &'static str {
    BASE_CHARACTER
}

/// How tall a character model is in its own space, before the game scales it.
///
/// Measured from `base.glb`: 1.4 units, standing on y=0. This was previously assumed to be 1.0 --
/// the comment even said "exported one unit tall" -- so every character was rendered 40% taller
/// than the body it hangs on, which is why they did not sit right against the pitch or each other.
/// MonkeyForge exports on a fixed base, so every forged character shares this height; the test
/// below pins it, and a changed asset breaks that rather than silently mis-scaling.
pub const CHARACTER_MODEL_HEIGHT: f32 = 1.4;

/// Scale that makes a character exactly as tall as the body it replaces.
pub fn character_scale() -> f32 {
    CUBE_SIZE * CHARACTER_PRESENCE / CHARACTER_MODEL_HEIGHT
}

/// How much bigger than its collider a character is drawn.
///
/// Sized exactly to `CUBE_SIZE` the monkeys read as tiny: the pitch is 48 units across and the
/// ball alone is 1.2, so a 1.5-unit player is a speck at broadcast distance. Drawing them half
/// again over their collider gives them the chunky presence the art is built for, at the cost of
/// feet and ears overhanging the box they collide with -- which is the usual trade in a game with
/// stylised proportions, and far less noticeable than players you cannot see.
///
/// Visual only. The collider, mass and observation vector are untouched, so physics and any
/// trained policy are unaffected.
const CHARACTER_PRESENCE: f32 = 1.5;

/// Which model each team is wearing, as a path under `assets/`.
///
/// This is a resource rather than a constant because it changes at runtime: a character forged
/// from a player's drawing is written to disk and then pointed at here, and the whole team's
/// appearance changes on the next frame. Setting a team to `None` puts it back in the blocky
/// character the jungle dresses it in.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct WornCharacters {
    pub orange: Option<String>,
    pub blue: Option<String>,
}

impl Default for WornCharacters {
    fn default() -> Self {
        Self {
            orange: Some(dressed_character(Team::Orange).to_owned()),
            blue: Some(dressed_character(Team::Blue).to_owned()),
        }
    }
}

impl WornCharacters {
    pub fn get(&self, team: Team) -> Option<&str> {
        match team {
            Team::Orange => self.orange.as_deref(),
            Team::Blue => self.blue.as_deref(),
        }
    }

    pub fn set(&mut self, team: Team, path: Option<String>) {
        match team {
            Team::Orange => self.orange = path,
            Team::Blue => self.blue = path,
        }
    }
}

/// A generated model hanging off a player's visual node, and how far along it is.
///
/// The model is *not* shown the moment it is asked for. Bevy loads a glTF asynchronously and
/// fails silently when the file is missing — a wrong working directory would otherwise leave ten
/// invisible players, since the jungle blanks the body's own mesh either way. So the blocky
/// character stays up until the scene has genuinely finished loading, and only then is it taken
/// down. A model that never arrives simply never replaces anything.
#[derive(Component)]
pub struct CharacterSkin {
    /// What was asked for, so a change of model is noticed.
    pub path: String,
    pub(crate) scene: Handle<Scene>,
    /// Whether the blocky character underneath has been taken down yet.
    pub(crate) revealed: bool,
}

/// One block of the jungle's blocky character, so it can be removed as a unit when a generated
/// model is ready to take its place.
#[derive(Component)]
pub struct BlockyCharacter;

// --- Animation tuning --------------------------------------------------------------------------
// Offsets are in cube units and are scaled by CUBE_SIZE when applied, so the cube and a generated
// character move by the same visible amount.

/// Radians of gait phase per metre travelled. Phase advances with distance, not wall time, so the
/// step rate follows the speed instead of sliding against it.
// Amplitudes are deliberately small, and they used to be five times this.
//
// They were raised when the character was a single rigid mesh: with no limbs to swing, the only
// way to show life was to throw the whole body around. `base.glb` is rigged now
// (`tools/rig_dartmonkey.py`), so `rig::animate_rig` steps the legs and swings the arms, and the
// body motion's job changes completely -- it is there to support the gait, not to be it. Left
// large it did the opposite: a 0.26-unit hop on a 1.5-unit character moved the whole monkey
// further than its legs did, so the stride was lost inside the bounce and it read as hopping.
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
const LEAN_PITCH: f32 = 0.20;
const LEAN_BANK: f32 = 0.06;
/// How fast lean eases toward its target, per second.
const LEAN_EASE: f32 = 8.0;
/// Downward speed past which an arrested fall counts as a landing.
const LANDING_SPEED: f32 = 3.0;
/// How far a landing flattens the visual, and how fast that recovers.
const SQUASH_DEPTH: f32 = 0.12;
const SQUASH_DECAY: f32 = 6.0;
/// The kick: forward pitch, forward reach, and decay.
const KICK_PITCH: f32 = 0.26;
const KICK_REACH: f32 = 0.10;
const KICK_DECAY: f32 = 4.5;
/// Taking a hit: recoil rotation, shake amplitude and rate, and decay.
const HIT_RECOIL: f32 = 0.34;
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
    /// The gait phase, in radians.
    ///
    /// Exposed so [`rig::animate_rig`](super::rig::animate_rig) can drive the limbs from the very
    /// same phase this drives the body bob from. Two clocks would put the feet out of step with
    /// the bounce.
    pub fn gait(&self) -> f32 {
        self.gait
    }

    /// How far through a kick this player is: 1.0 the instant they strike, decaying to 0.
    ///
    /// Exposed for the same reason as [`Self::gait`] -- the rig swings the leg that does the
    /// kicking, and it has to be the same kick the body is leaning into.
    pub fn kick(&self) -> f32 {
        self.kick
    }

    /// How far through a knockback reaction this player is, 1.0 down to 0.
    pub fn hit(&self) -> f32 {
        self.hit
    }

    /// Local-space direction the last hit came from.
    pub fn hit_from(&self) -> Vec3 {
        self.hit_from
    }
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

/// Ask each player's visual node to wear the model its team is currently assigned.
///
/// Runs every frame but does work only when the answer changes, so pointing [`WornCharacters`] at
/// a freshly forged model is all it takes to re-dress a whole side mid-match.
pub fn wear_characters(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    worn: Res<WornCharacters>,
    bodies: Query<&CubePlayer>,
    visuals: Query<(Entity, &Parent, Option<&CharacterSkin>), With<PlayerVisual>>,
) {
    for (visual, parent, current) in &visuals {
        let Ok(player) = bodies.get(parent.get()) else {
            continue;
        };

        // Owned, because the borrow of the resource cannot outlive this arm.
        let wanted = worn.get(player.team).map(str::to_owned);

        match (wanted, current) {
            // Already wearing exactly this.
            (Some(wanted), Some(skin)) if skin.path == wanted => {}

            // A model is wanted, and either none or a different one is on.
            (Some(wanted), _) => {
                let scene = asset_server.load(&wanted);
                commands.entity(visual).insert(CharacterSkin {
                    path: wanted,
                    scene,
                    revealed: false,
                });
            }

            // Back to the blocky character: drop the model and let the jungle's own blocks show.
            (None, Some(_)) => {
                commands.entity(visual).remove::<CharacterSkin>();
            }
            (None, None) => {}
        }
    }
}

/// Show a model once it has actually loaded, and take the blocky character down.
///
/// The scene is spawned only here, not when it was requested, so a model that fails to load never
/// replaces the thing that was already working.
pub fn reveal_loaded_characters(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut visuals: Query<(Entity, &mut CharacterSkin, Option<&Children>), With<PlayerVisual>>,
    blocks: Query<(), With<BlockyCharacter>>,
) {
    for (visual, mut skin, children) in &mut visuals {
        if skin.revealed || !asset_server.is_loaded_with_dependencies(&skin.scene) {
            continue;
        }

        commands.entity(visual).with_children(|node| {
            node.spawn(SceneBundle {
                scene: skin.scene.clone(),
                // The model is exported standing on y=0 one unit tall, while the node it hangs
                // from is centred on the body's origin — so drop it half a cube to put its feet
                // at the cube's bottom face, and scale it up to the body's size. The node itself
                // stays unscaled, because the blocky character and the cube fallback are already
                // authored at body scale and share it.
                // Feet on the bottom face of the body, and exactly the body's height -- scaled
                // by its own measured height rather than by `CUBE_SIZE` directly, which had been
                // making every character 40% too tall.
                transform: Transform::from_xyz(0.0, -CUBE_SIZE / 2.0, 0.0)
                    .with_scale(Vec3::splat(character_scale())),
                ..default()
            });
        });

        // The generated monkey has its own face painted on, so the blocky character's blocks --
        // and the googly eyes the cube fallback brought with it -- come off together.
        for child in children.into_iter().flatten() {
            if blocks.contains(*child) {
                commands.entity(*child).despawn_recursive();
            }
        }

        skin.revealed = true;
    }
}

/// The animated node every player visual hangs from.
///
/// Deliberately a neutral pivot — no offset, no scale — because the three things that can hang
/// from it (the jungle's blocky character, the cube fallback, and a generated model) are authored
/// at different scales. The first two are already in body units; the model carries its own fit
/// transform where it is spawned.
pub fn visual_node() -> (PlayerVisual, SpatialBundle) {
    (PlayerVisual::new(0.0, 1.0), SpatialBundle::default())
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
    fn everyone_starts_in_the_base_monkey() {
        let worn = WornCharacters::default();
        assert_eq!(worn.get(Team::Orange), Some(BASE_CHARACTER));
        assert_eq!(worn.get(Team::Blue), Some(BASE_CHARACTER));
    }

    #[test]
    fn the_base_model_names_a_scene_bevy_can_find() {
        // Both halves matter and neither is checked at compile time: without `#Scene0` the glTF
        // loads and nothing is spawned, and the path is resolved against the working directory's
        // `assets/`, which is the repo root the launcher runs the binary from.
        assert!(
            BASE_CHARACTER.ends_with("#Scene0"),
            "bevy needs the scene label: {BASE_CHARACTER}"
        );
        assert!(
            BASE_CHARACTER.starts_with("characters/"),
            "models live in assets/characters"
        );
    }

    #[test]
    fn forging_for_one_team_leaves_the_other_alone() {
        let mut worn = WornCharacters::default();
        worn.set(Team::Orange, Some("characters/forged-orange.glb#Scene0".to_owned()));
        assert_eq!(worn.get(Team::Orange), Some("characters/forged-orange.glb#Scene0"));
        assert_eq!(
            worn.get(Team::Blue),
            Some(BASE_CHARACTER),
            "one player drawing a character must not re-dress their opponent"
        );
    }

    #[test]
    fn a_team_can_be_put_back_in_the_blocky_character() {
        let mut worn = WornCharacters::default();
        worn.set(Team::Orange, None);
        assert_eq!(worn.get(Team::Orange), None);
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


/// The eyelid mesh inside the monkey model, once found.
///
/// The model has no skeleton -- no skins and no clips -- so nothing about it can be animated by
/// playing an exported clip. What it does have is its eyelids as a separate node, which is enough
/// for a blink, and a blink is most of what makes a face look alive.
#[derive(Component)]
pub struct Eyelids {
    /// Seconds until the next blink.
    next: f32,
    /// How far through the current blink we are, or `None` between blinks.
    closing: Option<f32>,
}

/// Find the eyelid node in a freshly loaded character and take charge of it.
pub fn adopt_eyelids(
    mut commands: Commands,
    named: Query<(Entity, &Name), Without<Eyelids>>,
) {
    for (entity, name) in &named {
        if name.as_str().contains("Eyelid") {
            commands.entity(entity).insert(Eyelids { next: 1.5, closing: None });
        }
    }
}

/// Blink. Irregularly, because a metronome blink is worse than none.
pub fn blink(time: Res<Time>, mut lids: Query<(&mut Eyelids, &mut Transform)>) {
    /// How long one blink takes, closed and open again.
    const BLINK_SECS: f32 = 0.14;

    let dt = time.delta_seconds();
    for (index, (mut lid, mut transform)) in lids.iter_mut().enumerate() {
        match lid.closing {
            None => {
                lid.next -= dt;
                if lid.next <= 0.0 {
                    lid.closing = Some(0.0);
                }
                transform.scale.y = 0.0;
            }
            Some(progress) => {
                let progress = progress + dt;
                if progress >= BLINK_SECS {
                    lid.closing = None;
                    // Staggered per player so a team does not blink in unison.
                    lid.next = 2.0 + (index as f32 * 0.7) % 2.5;
                    transform.scale.y = 0.0;
                } else {
                    lid.closing = Some(progress);
                    // Down and back up over the blink.
                    let t = progress / BLINK_SECS;
                    transform.scale.y = (t * std::f32::consts::PI).sin();
                }
            }
        }
    }
}

/// Marks a character surface already set up, so the pass is idempotent.
#[derive(Component)]
pub struct Lit;

/// Light the characters the way their art expects: not at all.
///
/// The monkeys were coming out nearly black, and measurement (`CANOPY_SKIN_PROBE`) showed why:
/// their texture loads fine and their base colour is white, but they were the only things in the
/// scene still being lit by the real lights. Every jungle prop is cel-shaded and so carries its
/// own brightness, which left the players as the one PBR-lit object under a single sun -- dark
/// against props that ignore it.
///
/// The fix is not more light. This art is flat-shaded with its form already painted into the
/// diffuse, exactly like the cel-shaded props, so it wants no light model at all: unlit renders
/// the texture as drawn, which is the bright orange the model was authored as.
///
/// The alpha mode is corrected at the same time. The exporter marks the body `Blend`, which makes
/// an opaque monkey depth-sort against itself and flicker where ears and hair overlap the face --
/// the glitching on the pitch. `Mask` keeps the cutouts the atlas does use without sorting.
pub fn light_the_characters(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    surfaces: Query<(Entity, &Handle<StandardMaterial>), (Without<Lit>, Without<BlockyCharacter>)>,
    parents: Query<&Parent>,
    skins: Query<(), With<CharacterSkin>>,
) {
    for (entity, handle) in &surfaces {
        // glTF meshes arrive as descendants of the node that asked for the scene.
        let mut current = entity;
        let mut theirs = false;
        for _ in 0..12 {
            if skins.contains(current) {
                theirs = true;
                break;
            }
            let Ok(parent) = parents.get(current) else { break };
            current = parent.get();
        }
        if !theirs {
            continue;
        }
        let Some(material) = materials.get_mut(handle) else {
            continue;
        };
        material.unlit = true;
        if matches!(material.alpha_mode, AlphaMode::Blend) {
            material.alpha_mode = AlphaMode::Mask(0.5);
        }
        commands.entity(entity).insert(Lit);
    }
}

/// Report what the loaded character meshes actually ended up rendering with.
///
/// Env-gated diagnostic: `CANOPY_SKIN_PROBE=1`. Answers the only question worth asking when the
/// monkeys come out the wrong colour -- did the cel pass claim them, is the texture missing, or is
/// the base colour itself dark.
pub fn probe_character_materials(
    mut done: Local<bool>,
    standard: Query<(Entity, &Handle<StandardMaterial>)>,
    jungle: Query<(Entity, &Handle<crate::rendering::stylized::JungleMaterial>)>,
    parents: Query<&Parent>,
    skins: Query<(), With<CharacterSkin>>,
    names: Query<&Name>,
    materials: Res<Assets<StandardMaterial>>,
    jungle_materials: Option<Res<Assets<crate::rendering::stylized::JungleMaterial>>>,
    time: Res<Time>,
) {
    if *done || std::env::var("CANOPY_SKIN_PROBE").is_err() || time.elapsed_seconds() < 6.0 {
        return;
    }
    *done = true;

    let of_a_character = |mut e: Entity| {
        for _ in 0..12 {
            if skins.contains(e) {
                return true;
            }
            let Ok(p) = parents.get(e) else { return false };
            e = p.get();
        }
        false
    };

    for (entity, handle) in &standard {
        if !of_a_character(entity) {
            continue;
        }
        let name = names.get(entity).map(|n| n.as_str().to_owned()).unwrap_or_default();
        if let Some(m) = materials.get(handle) {
            eprintln!(
                "SKIN standard {name:?}: base={:?} tex={} emissive={:?} unlit={} alpha={:?}",
                m.base_color,
                m.base_color_texture.is_some(),
                m.emissive,
                m.unlit,
                m.alpha_mode,
            );
        }
    }
    let count = jungle.iter().filter(|(e, _)| of_a_character(*e)).count();
    eprintln!("SKIN cel-shaded character surfaces = {count}  (must be 0)");
    if let Some(jm) = jungle_materials {
        eprintln!("SKIN jungle material count = {}", jm.len());
    }
}
