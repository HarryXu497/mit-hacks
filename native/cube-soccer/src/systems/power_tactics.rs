//! Who carries the superpower, and when they fire it.
//!
//! The powers themselves ([`crate::systems::superpowers`]) have always known how to go off. What
//! was missing either side of them was judgement: `activate_superpowers` fires whenever an input
//! asks it to, and until now the only thing asking was "a player exists and is not on cooldown".
//! A whole side was armed and every one of them held the button down, which is a pitch nobody can
//! read through the effects and a power that never lands anywhere it matters.
//!
//! This was meant to be the reinforcement learner's job -- `fire` is the fourth element of the
//! action vector precisely so a policy could learn it. That did not work well enough to keep, so
//! the decision is made here instead, by two deterministic algorithms:
//!
//! 1. **Placement** ([`bearer_slot`]) -- which one player on a side carries the power. Decided
//!    once, from the shape of the formation and the mechanics of the power, so a side fields
//!    exactly one caster and it is the one best placed to use that particular power.
//! 2. **Use** ([`should_fire`]) -- whether this frame is the frame. One policy per power, because
//!    the four powers want four genuinely different things.
//!
//! # The rule that shapes all of this
//!
//! `activate_superpowers` **picks its own victim**. Beam Blast hits everyone in its cone, Freeze
//! Ray takes the nearest opponent *inside* the cone, Slow takes the nearest opponent within range
//! regardless of facing, and Boost lands on the caster. Nothing here chooses a target, so the only
//! question a policy can usefully answer is *"is the target the engine is about to pick the one
//! worth spending the cooldown on?"* Every policy below therefore runs the engine's own search --
//! [`in_cone`], [`nearest_in_cone`], [`nearest_within`] -- rather than a lookalike of it. A policy
//! that reasons about a different player than the cast hits is a policy that fires at the wrong
//! one, and the two drifting apart is silent.
//!
//! The second rule is that **a cast with no target costs nothing**. `activate_superpowers` only
//! starts the cooldown if the power actually landed, so a hopeful Blast, Freeze or Slow that finds
//! nobody is free. Boost is the exception: it always lands, so a Boost fired at the wrong moment
//! really does cost the full ten seconds. That asymmetry is why [`boost`] is the strictest policy
//! here and [`blast`] the most willing.

use bevy::prelude::*;

use crate::entities::roster::formation_position;
use crate::game::config::*;
use crate::systems::soccer_ai::{intercept_time, lane_clearance};
use crate::systems::superpowers::{in_cone, nearest_in_cone, nearest_within, SuperpowerKind};

// === Placement ===

/// How fast an opponent must be closing on the caster's goal to count as doing damage, when they
/// are still in their own half. Roughly a quarter of [`CUBE_MAX_SPEED`]: enough to tell a player
/// carrying the ball forward from one shielding it or turning back.
const THREAT_CLOSING_SPEED: f32 = 4.0;

/// How much room a *running* player needs either side of their line to the goal, in metres.
///
/// Wider than the ball needs for a pass, because this is a body at speed rather than a ball
/// threading a gap -- and much wider than one cube, which is a corridor so narrow that almost
/// every lane on the pitch reads as open.
const RUN_LANE_RADIUS: f32 = CUBE_SIZE * 2.0;

/// How quickly a teammate stops counting toward a slot's congestion, in metres.
///
/// Set to the width of the Beam Blast cone's reach, because congestion is standing in for "how
/// often is somebody else's body near this slot" and that is the scale the powers work at.
const CONGESTION_SCALE: f32 = 8.0;

/// What one formation slot is like to play, on the five traits the powers care about.
///
/// All six are `0..=1` and all six are derived from [`crate::entities::roster::FORMATION`] rather
/// than written down, so moving the formation moves the powers with it instead of leaving them
/// pinned to slot numbers that no longer mean what they meant.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Traits {
    /// How far up the pitch the slot starts, `1` for the most advanced player on the side.
    advancement: f32,
    /// How near the middle of the pitch, `1` dead centre and `0` on the touchline.
    centrality: f32,
    /// How many other bodies are usually close by, `1` for the busiest slot on the side.
    congestion: f32,
    /// How steadily the slot faces up the pitch: deep and central players spend the match looking
    /// one way, forwards and wingers spend it turning. `(1 - advancement) * centrality`.
    steadiness: f32,
    /// Room to run into. The complement of congestion.
    isolation: f32,
    /// How midfield the slot is -- peaks between the anchor and the forward.
    midfield: f32,
    /// How wide. The complement of centrality.
    wideness: f32,
}

/// What a power wants from whoever carries it, as weights over [`Traits`].
///
/// These are read off the power's own numbers in [`crate::game::config`], and the reasoning is
/// written out at each one because the weights are the argument.
fn demands(kind: SuperpowerKind) -> Traits {
    match kind {
        // 8 m, a 30-degree cone, and it shoves *everyone* standing in it -- the only power that
        // scales with how many opponents are present. Five seconds is also much the shortest
        // cooldown, so it wants volume rather than a perfect moment. Both point at the busiest
        // slot on the side; the mild advancement weight breaks toward the attacking end, where
        // shoving a defender out of the way is worth more than shoving one in midfield.
        SuperpowerKind::BeamBlast => Traits {
            congestion: 1.0,
            centrality: 0.5,
            advancement: 0.2,
            ..Default::default()
        },
        // 10 m down a 15-degree cone: by some way the most demanding aim in the game, and cubes
        // turn by angular velocity, so a player who is mid-turn has no cone worth speaking of.
        // It belongs to the one slot that spends the match pointed steadily up the pitch, which
        // is also the slot that most wants to stop somebody: two seconds frozen in front of your
        // own goal is the single most valuable two seconds this power can buy.
        SuperpowerKind::FreezeRay => Traits {
            steadiness: 1.0,
            centrality: 0.5,
            ..Default::default()
        },
        // A self buff and the longest cooldown on the board, so each use has to be worth ten
        // seconds. Speed only converts into anything where there is grass to cover: it wants the
        // advanced slot with the fewest bodies around it, which is the wide player, not the
        // central forward who is running into traffic whatever his top speed is.
        SuperpowerKind::Boost => Traits {
            isolation: 1.0,
            advancement: 0.8,
            wideness: 0.6,
            ..Default::default()
        },
        // 12 m, radial, and single-target. Aim is free, so nothing here is spent on facing; what
        // it wants instead is to be near the opponent carrying the ball, often, which is the
        // midfielder's job rather than the forward's or the anchor's. Single-target also means it
        // is worth most where there is one opponent who matters rather than four.
        SuperpowerKind::Slow => Traits {
            midfield: 1.0,
            congestion: 0.6,
            centrality: 0.2,
            ..Default::default()
        },
    }
}

/// The traits of every formation slot, for a side of `slots` players.
fn slot_traits(slots: usize) -> Vec<Traits> {
    if slots == 0 {
        return Vec::new();
    }
    // Either side gives the same answer -- the formation is mirrored -- so one is picked and the
    // distances below are measured in its frame.
    let team = Team::Orange;
    let spots: Vec<Vec3> = (0..slots).map(|slot| formation_position(team, slot)).collect();

    // Distance up the pitch from the side's own goal line.
    let from_goal: Vec<f32> = spots.iter().map(|p| p.x + FIELD_WIDTH / 2.0).collect();
    let deepest = from_goal.iter().cloned().fold(f32::INFINITY, f32::min);
    let highest = from_goal.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let spread = (highest - deepest).max(1e-3);

    // Raw crowding: how much of the rest of the side is standing near this slot.
    let crowding: Vec<f32> = spots
        .iter()
        .map(|me| {
            spots
                .iter()
                .filter(|other| (**other - *me).length() > 1e-3)
                .map(|other| (-(*other - *me).length() / CONGESTION_SCALE).exp())
                .sum::<f32>()
        })
        .collect();
    let busiest = crowding.iter().cloned().fold(f32::NEG_INFINITY, f32::max).max(1e-3);

    (0..slots)
        .map(|i| {
            let advancement = ((from_goal[i] - deepest) / spread).clamp(0.0, 1.0);
            let centrality = (1.0 - spots[i].z.abs() / (FIELD_DEPTH / 2.0)).clamp(0.0, 1.0);
            let congestion = (crowding[i] / busiest).clamp(0.0, 1.0);
            Traits {
                advancement,
                centrality,
                congestion,
                steadiness: (1.0 - advancement) * centrality,
                isolation: 1.0 - congestion,
                midfield: 1.0 - (advancement - 0.5).abs() * 2.0,
                wideness: 1.0 - centrality,
            }
        })
        .collect()
}

/// How well one slot suits one power.
fn fitness(want: &Traits, has: &Traits) -> f32 {
    want.advancement * has.advancement
        + want.centrality * has.centrality
        + want.congestion * has.congestion
        + want.steadiness * has.steadiness
        + want.isolation * has.isolation
        + want.midfield * has.midfield
        + want.wideness * has.wideness
}

/// Which player on a side of `slots` should carry `kind`.
///
/// Scores every slot on the traits above against what the power asks for and takes the best. On
/// the shipped five-a-side formation the four powers land on four different players, each for the
/// reason written against its weights in [`demands`]:
///
/// | Power | Slot | Why |
/// |---|---|---|
/// | Beam Blast | the central forward | most bodies in front of him, and displacement is worth most in the attacking third |
/// | Freeze Ray | the deep anchor | the only slot steady enough to land a 15-degree cone, and the one that most needs to stop a runner |
/// | Boost | the wide winger | the advanced slot with space to spend the speed in |
/// | Slow | the central midfielder | lives nearest the opposing carrier, and needs no aim to punish him |
///
/// Ties break toward the lower slot, so the choice is stable rather than merely deterministic.
pub fn bearer_slot(kind: SuperpowerKind, slots: usize) -> Option<usize> {
    let want = demands(kind);
    slot_traits(slots)
        .iter()
        .enumerate()
        .fold(None, |best: Option<(usize, f32)>, (slot, has)| {
            let score = fitness(&want, has);
            match best {
                Some((_, b)) if b >= score => best,
                _ => Some((slot, score)),
            }
        })
        .map(|(slot, _)| slot)
}

/// The power that answers `kind`, for arming a side that did not draw one.
///
/// A self-inverse pairing, so it reads the same from either side of the pitch: speed is answered
/// by the denial of speed, and knocking players out of position by stopping them where they are.
/// Deterministic on purpose -- the opponent's power is visible on the HUD from the first whistle,
/// and a coach who knows what they drew can work out what they are playing against.
pub fn counter_to(kind: SuperpowerKind) -> SuperpowerKind {
    match kind {
        SuperpowerKind::Boost => SuperpowerKind::Slow,
        SuperpowerKind::Slow => SuperpowerKind::Boost,
        SuperpowerKind::BeamBlast => SuperpowerKind::FreezeRay,
        SuperpowerKind::FreezeRay => SuperpowerKind::BeamBlast,
    }
}

// === Use ===

/// One player, as a firing policy sees them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Actor {
    pub pos: Vec3,
    pub vel: Vec3,
}

/// Everything a policy reads. Borrowed rather than owned so the callers can hand over the slices
/// they already built for the rest of their decision.
#[derive(Clone, Copy, Debug)]
pub struct Cast<'a> {
    pub kind: SuperpowerKind,
    pub team: Team,
    /// The caster.
    pub me: Actor,
    /// Which way the caster is actually pointing -- `facing_dir(transform.rotation)`, **not** the
    /// direction it would like to move. Cubes turn by angular velocity, so a player changing
    /// direction is aiming somewhere between the two for several frames, and the cone powers are
    /// resolved against this one.
    pub facing: Vec3,
    pub ball: Vec3,
    pub ball_vel: Vec3,
    /// The caster's side, excluding the caster.
    pub mates: &'a [Actor],
    pub opponents: &'a [Actor],
}

/// Who has the ball.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holder {
    /// The caster.
    Me,
    /// Somebody on the caster's side.
    Mate,
    /// An opponent, by index into [`Cast::opponents`].
    Opponent(usize),
    /// Nobody is close enough to it.
    Loose,
}

impl<'a> Cast<'a> {
    fn opponent_positions(&self) -> Vec<Vec3> {
        self.opponents.iter().map(|o| o.pos).collect()
    }

    /// Who is in control, by the same radius possession uses to decide somebody has kept the ball.
    fn holder(&self) -> Holder {
        let mut best = (Holder::Loose, CONTROL_RADIUS);
        let consider = |best: &mut (Holder, f32), who: Holder, pos: Vec3| {
            let d = pos.distance(self.ball);
            if d <= best.1 {
                *best = (who, d);
            }
        };
        consider(&mut best, Holder::Me, self.me.pos);
        for mate in self.mates {
            consider(&mut best, Holder::Mate, mate.pos);
        }
        for (i, opp) in self.opponents.iter().enumerate() {
            consider(&mut best, Holder::Opponent(i), opp.pos);
        }
        best.0
    }

    /// The opponent who would reach a loose ball first, and how long it would take them.
    fn opponent_favourite(&self) -> Option<(usize, f32)> {
        self.opponents
            .iter()
            .enumerate()
            .map(|(i, o)| (i, intercept_time(o.pos, self.ball, self.ball_vel)))
            .fold(None, |best: Option<(usize, f32)>, (i, t)| match best {
                Some((_, b)) if b <= t => best,
                _ => Some((i, t)),
            })
    }

    /// The soonest anyone on the caster's side, the caster included, gets to a loose ball.
    fn our_best_time(&self) -> f32 {
        std::iter::once(self.me)
            .chain(self.mates.iter().copied())
            .map(|p| intercept_time(p.pos, self.ball, self.ball_vel))
            .fold(f32::INFINITY, f32::min)
    }

    /// Is this opponent the one worth spending a cooldown on -- either holding the ball, or about
    /// to pick up a loose one before we can?
    ///
    /// The single question every targeted policy reduces to, since the engine has already chosen
    /// *which* opponent gets hit.
    fn worth_hitting(&self, index: usize) -> bool {
        match self.holder() {
            Holder::Opponent(held) => held == index,
            Holder::Loose => self
                .opponent_favourite()
                .is_some_and(|(fav, t)| fav == index && t < self.our_best_time()),
            // Our ball. Nothing to deny -- clearing a path is Blast's business, and it has its
            // own reason to fire below.
            Holder::Me | Holder::Mate => false,
        }
    }

    fn opp_goal_x(&self) -> f32 {
        match self.team {
            Team::Orange => FIELD_WIDTH / 2.0,
            Team::Blue => -FIELD_WIDTH / 2.0,
        }
    }

    fn own_goal_x(&self) -> f32 {
        -self.opp_goal_x()
    }

    /// Is this opponent actually doing damage, as opposed to merely being the one holding it?
    ///
    /// A cooldown is the same length whoever it is spent on, so the question is not "has he got
    /// the ball" but "is the ball going anywhere". He counts if he has carried it into our half,
    /// or if he is still in his own but running at us hard enough that he is about to.
    fn threatening(&self, index: usize) -> bool {
        let them = self.opponents[index];
        let toward_us = self.own_goal_x().signum();
        let in_our_half = them.pos.x * toward_us > 0.0;
        in_our_half || them.vel.x * toward_us > THREAT_CLOSING_SPEED
    }
}

/// Whether this player should fire their power this frame.
///
/// Pure, and pure of the ECS: the callers assemble a [`Cast`] out of whatever they already have.
pub fn should_fire(cast: &Cast) -> bool {
    match cast.kind {
        SuperpowerKind::BeamBlast => blast(cast),
        SuperpowerKind::FreezeRay => freeze(cast),
        SuperpowerKind::Boost => boost(cast),
        SuperpowerKind::Slow => slow(cast),
    }
}

/// Beam Blast: the willing one.
///
/// Five seconds is the shortest cooldown in the game and a cast that catches nobody costs nothing,
/// so this does not wait for a perfect moment. Two opponents in the cone is always worth it -- the
/// power hits both, and no other power on the board does that. A single opponent is worth it when
/// the ball is the thing being contested: shoving the man off our carrier, or shoving the carrier
/// off the ball.
fn blast(cast: &Cast) -> bool {
    let half_angle = BLAST_HALF_ANGLE_DEG.to_radians();
    let caught: Vec<usize> = cast
        .opponents
        .iter()
        .enumerate()
        .filter(|(_, o)| in_cone(cast.me.pos, cast.facing, o.pos, BLAST_RANGE, half_angle))
        .map(|(i, _)| i)
        .collect();

    match caught.len() {
        0 => false,
        1 => matches!(cast.holder(), Holder::Me) || cast.worth_hitting(caught[0]),
        _ => true,
    }
}

/// Freeze Ray: the surgical one.
///
/// Two seconds of a player not existing, for eight seconds of cooldown, down a cone so narrow that
/// the shot has to be set up rather than taken. Only ever spent on the opponent who is actually
/// doing something with the ball, and only at the player the engine would really hit.
fn freeze(cast: &Cast) -> bool {
    let half_angle = FREEZE_HALF_ANGLE_DEG.to_radians();
    let positions = cast.opponent_positions();
    nearest_in_cone(cast.me.pos, cast.facing, &positions, FREEZE_RANGE, half_angle)
        .is_some_and(|target| cast.worth_hitting(target))
}

/// Boost: the strict one.
///
/// The only power that cannot miss, which is exactly why it is rationed: every press spends the
/// full ten seconds whether or not it achieved anything, where a mistimed Blast or Freeze simply
/// finds nobody and stays ready. So it fires in three situations, all of which are decided by how
/// fast the caster can move and by nothing else.
fn boost(cast: &Cast) -> bool {
    let goal = Vec3::new(cast.opp_goal_x(), cast.ball.y, 0.0);
    let opponents = cast.opponent_positions();

    match cast.holder() {
        // Running at goal with the lane open: this is the breakaway the power exists for. The
        // distance floor keeps it from being burned a stride from the goal line, where two seconds
        // of extra speed changes nothing that the shot was not already going to do.
        Holder::Me => {
            let to_goal = (goal - cast.me.pos).length();
            // Already going that way. Speed added to a player who is turning, shielding or
            // playing backwards buys nothing, and without this the power goes off on more or
            // less every possession -- measured at 17 casts of a possible 24 in a two-minute
            // match, which is the fire button with extra steps.
            let running_at_goal = cast.me.vel.x * (cast.opp_goal_x()).signum();
            to_goal > CUBE_MAX_SPEED * BOOST_SECS * 0.5
                && running_at_goal > THREAT_CLOSING_SPEED
                && lane_clearance(cast.me.pos, goal, &opponents, RUN_LANE_RADIUS) > 0.6
        }
        // An opponent is carrying and the caster is the nearest man to him but behind the play --
        // a recovery run, which is the one defensive errand raw speed actually settles.
        Holder::Opponent(carrier) => {
            let them = cast.opponents[carrier].pos;
            let chase = (them - cast.me.pos).length();
            let goal_side = (them.x - cast.me.pos.x) * (cast.opp_goal_x() - them.x).signum() > 0.0;
            let nearest = cast
                .mates
                .iter()
                .all(|mate| (them - mate.pos).length() >= chase);
            chase < SLOW_RANGE && !goal_side && nearest
        }
        // A loose ball we are losing the race for, but only one the extra speed actually flips.
        // Without that second test this fires on every ball the caster is not winning, including
        // the ones on the far side of the pitch.
        Holder::Loose => {
            let mine = intercept_time(cast.me.pos, cast.ball, cast.ball_vel);
            cast.opponent_favourite().is_some_and(|(_, theirs)| {
                mine > theirs && mine / BOOST_FACTOR < theirs && mine < BOOST_SECS * 2.0
            })
        }
        Holder::Mate => false,
    }
}

/// Slow: the patient one.
///
/// It takes the nearest opponent inside twelve metres whatever the caster is pointing at, so the
/// caster never chooses the victim -- it only chooses whether the nearest one happens to be the
/// one worth three seconds at 40% speed. In a crowd that is usually somebody irrelevant, which is
/// why this refuses far more often than it fires.
///
/// Twelve metres is a wide net and the carrier is very often the nearest body inside it, so
/// "has the ball" on its own is not much of a filter -- it fired on four possessions in five.
/// What earns the cooldown is a carrier who is actually going somewhere with it.
fn slow(cast: &Cast) -> bool {
    let positions = cast.opponent_positions();
    nearest_within(cast.me.pos, &positions, SLOW_RANGE)
        .is_some_and(|target| cast.worth_hitting(target) && cast.threatening(target))
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    use crate::game::PLAYERS_PER_TEAM;

    /// The slots the formation actually authors, so the expectations below are readable as roles
    /// rather than as numbers. Slot 0 stands nearest the halfway line and slot 3 nearest its own
    /// goal -- the array is ordered by nothing in particular, so this is worth pinning.
    fn role_of(slot: usize) -> &'static str {
        match slot {
            0 => "central forward",
            1 | 2 => "central midfielder",
            3 => "deep anchor",
            4 => "wide winger",
            _ => "spare",
        }
    }

    #[test]
    fn the_formation_is_ordered_the_way_the_placement_assumes() {
        let traits = slot_traits(PLAYERS_PER_TEAM);
        assert_eq!(traits[0].advancement, 1.0, "slot 0 is the most advanced");
        assert_eq!(traits[3].advancement, 0.0, "slot 3 is the deepest");
        assert!(traits[4].wideness > traits[1].wideness, "slot 4 is the wide one");
        assert!(traits[3].steadiness > traits[0].steadiness, "the anchor faces one way");
    }

    #[test]
    fn each_power_goes_to_the_player_its_mechanics_want() {
        let bearer = |kind| bearer_slot(kind, PLAYERS_PER_TEAM).unwrap();

        assert_eq!(role_of(bearer(SuperpowerKind::BeamBlast)), "central forward");
        assert_eq!(role_of(bearer(SuperpowerKind::FreezeRay)), "deep anchor");
        assert_eq!(role_of(bearer(SuperpowerKind::Boost)), "wide winger");
        assert_eq!(role_of(bearer(SuperpowerKind::Slow)), "central midfielder");
    }

    #[test]
    fn the_four_powers_do_not_pile_onto_one_player() {
        let mut seen: Vec<usize> = SuperpowerKind::ALL
            .iter()
            .map(|kind| bearer_slot(*kind, PLAYERS_PER_TEAM).unwrap())
            .collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 4, "each power should pick a different slot");
    }

    #[test]
    fn placement_is_stable_across_calls() {
        for kind in SuperpowerKind::ALL {
            let once = bearer_slot(kind, PLAYERS_PER_TEAM);
            assert_eq!(once, bearer_slot(kind, PLAYERS_PER_TEAM), "{kind:?} wandered");
            assert!(once.unwrap() < PLAYERS_PER_TEAM, "{kind:?} picked a player who is not there");
        }
    }

    #[test]
    fn a_side_of_one_arms_the_only_player_and_a_side_of_none_arms_nobody() {
        for kind in SuperpowerKind::ALL {
            assert_eq!(bearer_slot(kind, 1), Some(0));
            assert_eq!(bearer_slot(kind, 0), None, "nobody to arm");
        }
    }

    #[test]
    fn countering_is_self_inverse_and_never_answers_a_power_with_itself() {
        for kind in SuperpowerKind::ALL {
            let answer = counter_to(kind);
            assert_ne!(answer, kind, "{kind:?} should not counter itself");
            assert_eq!(counter_to(answer), kind, "{kind:?} does not pair back");
        }
    }
}

#[cfg(test)]
mod firing_tests {
    use super::*;

    const AWAY: Vec3 = Vec3::new(0.0, 0.0, 100.0);

    fn at(x: f32, z: f32) -> Actor {
        Actor { pos: Vec3::new(x, 1.0, z), vel: Vec3::ZERO }
    }

    /// Orange, facing +x (the way it attacks), standing on the origin.
    fn cast<'a>(
        kind: SuperpowerKind,
        ball: Vec3,
        mates: &'a [Actor],
        opponents: &'a [Actor],
    ) -> Cast<'a> {
        Cast {
            kind,
            team: Team::Orange,
            // Running at the goal it attacks. Boost asks whether the caster is actually going
            // somewhere before it spends ten seconds making them go there faster, so a caster
            // standing still is not the neutral default it looks like.
            me: Actor { pos: Vec3::new(0.0, 1.0, 0.0), vel: Vec3::new(8.0, 0.0, 0.0) },
            facing: Vec3::X,
            ball,
            ball_vel: Vec3::ZERO,
            mates,
            opponents,
        }
    }

    // --- Beam Blast ---

    #[test]
    fn blast_fires_on_two_in_the_cone_whoever_has_the_ball() {
        let opponents = [at(4.0, 0.5), at(5.0, -0.5)];
        assert!(should_fire(&cast(SuperpowerKind::BeamBlast, AWAY, &[], &opponents)));
    }

    #[test]
    fn blast_holds_for_a_lone_opponent_who_is_not_contesting_the_ball() {
        let opponents = [at(4.0, 0.0)];
        assert!(!should_fire(&cast(SuperpowerKind::BeamBlast, AWAY, &[], &opponents)));
    }

    #[test]
    fn blast_clears_the_path_when_the_caster_is_carrying() {
        let opponents = [at(4.0, 0.0)];
        // The ball is at the caster's feet, so the caster is the holder.
        let ball = Vec3::new(0.5, 1.0, 0.0);
        assert!(should_fire(&cast(SuperpowerKind::BeamBlast, ball, &[], &opponents)));
    }

    #[test]
    fn blast_ignores_opponents_behind_it() {
        let opponents = [at(-4.0, 0.0), at(-5.0, 0.5)];
        assert!(!should_fire(&cast(SuperpowerKind::BeamBlast, AWAY, &[], &opponents)));
    }

    #[test]
    fn blast_respects_its_own_range() {
        let opponents = [at(BLAST_RANGE + 2.0, 0.0), at(BLAST_RANGE + 3.0, 0.5)];
        assert!(!should_fire(&cast(SuperpowerKind::BeamBlast, AWAY, &[], &opponents)));
    }

    // --- Freeze Ray ---

    #[test]
    fn freeze_fires_at_the_opponent_carrying_the_ball() {
        let opponents = [at(6.0, 0.0)];
        let ball = Vec3::new(6.0, 1.0, 0.5);
        assert!(should_fire(&cast(SuperpowerKind::FreezeRay, ball, &[], &opponents)));
    }

    #[test]
    fn freeze_holds_when_the_carrier_is_outside_the_narrow_cone() {
        // Well inside 10 m, but ~37 degrees off the facing: the 15-degree cone does not reach it,
        // and the engine would find no target at all.
        let opponents = [at(6.0, 4.5)];
        let ball = Vec3::new(6.0, 1.0, 4.5);
        assert!(!should_fire(&cast(SuperpowerKind::FreezeRay, ball, &[], &opponents)));
    }

    #[test]
    fn freeze_holds_when_the_nearest_in_cone_is_not_the_one_that_matters() {
        // A body at 3 m screens the carrier at 8 m. The engine freezes the near one, so the
        // policy must not treat this as a shot at the carrier.
        let opponents = [at(3.0, 0.0), at(8.0, 0.0)];
        let ball = Vec3::new(8.0, 1.0, 0.3);
        assert!(!should_fire(&cast(SuperpowerKind::FreezeRay, ball, &[], &opponents)));
    }

    #[test]
    fn freeze_fires_at_whoever_is_about_to_reach_a_loose_ball() {
        // Loose ball ahead, an opponent much nearer to it than anyone of ours.
        let opponents = [at(7.0, 0.0)];
        let mates = [at(-20.0, 0.0)];
        let ball = Vec3::new(9.0, 1.0, 0.0);
        assert!(should_fire(&cast(SuperpowerKind::FreezeRay, ball, &mates, &opponents)));
    }

    // --- Boost ---

    #[test]
    fn boost_fires_on_a_clear_run_at_goal() {
        let ball = Vec3::new(0.5, 1.0, 0.0);
        let opponents = [at(-10.0, 0.0)];
        assert!(should_fire(&cast(SuperpowerKind::Boost, ball, &[], &opponents)));
    }

    #[test]
    fn boost_holds_for_a_carrier_who_is_not_running_at_the_goal() {
        // Same clear lane as the test above, but shielding the ball rather than driving at goal.
        // Multiplying a standing start by 1.5 spends the longest cooldown in the game on nothing.
        let ball = Vec3::new(0.5, 1.0, 0.0);
        let opponents = [at(-10.0, 0.0)];
        let mut stalled = cast(SuperpowerKind::Boost, ball, &[], &opponents);
        stalled.me.vel = Vec3::new(0.5, 0.0, 2.0);
        assert!(!should_fire(&stalled));
    }

    #[test]
    fn boost_holds_when_the_lane_is_blocked() {
        let ball = Vec3::new(0.5, 1.0, 0.0);
        let opponents = [at(6.0, 0.0), at(10.0, 0.0)];
        assert!(!should_fire(&cast(SuperpowerKind::Boost, ball, &[], &opponents)));
    }

    #[test]
    fn boost_holds_on_a_loose_ball_the_caster_is_already_winning() {
        let ball = Vec3::new(3.0, 1.0, 0.0);
        let opponents = [at(25.0, 0.0)];
        assert!(!should_fire(&cast(SuperpowerKind::Boost, ball, &[], &opponents)));
    }

    #[test]
    fn boost_holds_for_a_race_that_extra_speed_would_not_flip() {
        // Theirs is hopelessly closer: 1.5x does not bring the caster back into it, and the
        // cooldown would buy nothing.
        let ball = Vec3::new(40.0, 1.0, 0.0);
        let opponents = [at(41.0, 0.0)];
        assert!(!should_fire(&cast(SuperpowerKind::Boost, ball, &[], &opponents)));
    }

    #[test]
    fn boost_never_fires_while_a_teammate_has_the_ball() {
        let mates = [at(2.0, 0.0)];
        let ball = Vec3::new(2.0, 1.0, 0.4);
        let opponents = [at(-15.0, 0.0)];
        assert!(!should_fire(&cast(SuperpowerKind::Boost, ball, &mates, &opponents)));
    }

    // --- Slow ---

    #[test]
    fn slow_fires_at_a_carrier_it_is_not_even_facing() {
        // Directly behind the caster, which no cone power could reach. Slow does not care.
        let opponents = [at(-8.0, 0.0)];
        let ball = Vec3::new(-8.0, 1.0, 0.4);
        assert!(should_fire(&cast(SuperpowerKind::Slow, ball, &[], &opponents)));
    }

    #[test]
    fn slow_holds_when_the_nearest_opponent_is_not_the_carrier() {
        // The engine would slow the man at 2 m, not the carrier at 11 m.
        let opponents = [at(2.0, 0.0), at(11.0, 0.0)];
        let ball = Vec3::new(11.0, 1.0, 0.4);
        assert!(!should_fire(&cast(SuperpowerKind::Slow, ball, &[], &opponents)));
    }

    #[test]
    fn slow_holds_with_nobody_in_range() {
        let opponents = [at(SLOW_RANGE + 4.0, 0.0)];
        let ball = Vec3::new(SLOW_RANGE + 4.0, 1.0, 0.4);
        assert!(!should_fire(&cast(SuperpowerKind::Slow, ball, &[], &opponents)));
    }

    #[test]
    fn slow_holds_while_our_own_side_has_the_ball() {
        let opponents = [at(3.0, 0.0)];
        let ball = Vec3::new(0.5, 1.0, 0.0);
        assert!(!should_fire(&cast(SuperpowerKind::Slow, ball, &[], &opponents)));
    }

    // --- Across the board ---

    #[test]
    fn nobody_fires_at_an_empty_pitch() {
        for kind in SuperpowerKind::ALL {
            // No opponents, ball miles away and nobody in control: there is nothing any of the
            // four could usefully do, Boost included.
            assert!(!should_fire(&cast(kind, AWAY, &[], &[])), "{kind:?} fired at nothing");
        }
    }

    #[test]
    fn the_targeted_powers_agree_with_the_engine_about_who_gets_hit() {
        // Two opponents, the near one irrelevant and the far one carrying. Freeze and Slow both
        // auto-target the near one, so neither should claim this is a shot worth taking; Blast
        // hits both and is happy to.
        let opponents = [at(3.0, 0.0), at(7.0, 0.0)];
        let ball = Vec3::new(7.0, 1.0, 0.4);
        assert!(!should_fire(&cast(SuperpowerKind::FreezeRay, ball, &[], &opponents)));
        assert!(!should_fire(&cast(SuperpowerKind::Slow, ball, &[], &opponents)));
        assert!(should_fire(&cast(SuperpowerKind::BeamBlast, ball, &[], &opponents)));
    }
}
