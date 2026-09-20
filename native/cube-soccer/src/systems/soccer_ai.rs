//! The team that actually plays soccer.
//!
//! Every decision here is a pure function of the world and of the coached tactic. There is no
//! sampling and no learned component anywhere in it, so what a tactic will do can be worked
//! out by reading this file, and the same world state always produces the same decision.
//!
//! That is a weaker claim than a reproducible *match*. A match is a chaotic physics
//! simulation, and rapier's bookkeeping is not bit-identical from one process to the next, so
//! the same build can play out two noticeably different matches. Anything asserted about a
//! whole match therefore has to be asserted loosely; see the tests at the bottom of this file.
//!
//! What it does that the older positional AI ([`crate::systems::heuristic_ai`], kept for the
//! training environment's scripted opponent) did not:
//!
//! - **Strikes the ball.** A carrier shoots at the open part of the goal mouth, passes to the
//!   best-placed teammate, clears when pinned in its own third, and dribbles with touches
//!   rather than shoving the ball ahead of it.
//! - **Moves off the ball.** Supports run into open receiving positions instead of standing on
//!   formation dots, so there is someone to pass to.
//! - **Presses.** How many players hunt the opposing carrier, and from how far, comes from the
//!   tactic, so an aggressive coached play looks aggressive.
//! - **Gets unstuck.** Players wedged against a wall or each other sidestep and jump free, and
//!   a ball that stops moving is put back into play.
//!
//! The tactic reaches this module as [`TacticParams`] -- the same seven numbers the coaching
//! payload already resolves to. Those seven are a fixed interface shared with the training
//! environment's observation, so the extra knobs play needs are *derived* from them here, in
//! [`PlayStyle`], rather than added to them.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use std::collections::HashMap;

use crate::entities::{Ball, CubePlayer, KickRequest, PlayerInput};
use crate::game::config::{
    BALL_LINEAR_DAMPING, CLEAR_SPEED, CUBE_MAX_SPEED, CUBE_SIZE, FIELD_DEPTH, FIELD_WIDTH,
    GOAL_DEPTH, PASS_SPEED, SHOT_SPEED,
};
use crate::game::Team;
use crate::systems::heuristic_ai::{AiControlled, HeuristicDifficulty, TacticParams, TeamTactics};
use crate::systems::kick::{ground_ball_height, within_kick_range};

// === Tuning ===

/// Half-width of a passing or shooting lane. A blocker nearer the line than this spoils it.
const LANE_RADIUS: f32 = 2.2;
/// How far behind the ball a dribbler stands to push it, measured centre to centre.
const PUSH_OFFSET: f32 = CUBE_SIZE / 2.0 + 0.9;
/// A dribbler touches the ball forward when it is slower than this.
const TOUCH_BALL_SPEED: f32 = 9.0;
/// Speed of a dribbler's touch, as a fraction of a pass.
const TOUCH_FRACTION: f32 = 0.7;
/// A carrier counts as pressured by an opponent this close.
const PRESSURE_RADIUS: f32 = 4.5;
/// How far goal-side of the ball a pressing player stands, rather than diving into it.
const PRESS_STANDOFF: f32 = 2.6;
/// Beyond this distance from the centre line the ball is too wide to do anything with.
///
/// The arena is seven metres deeper than the pitch on each side, and nothing stops the ball
/// rolling out there. Nobody supports a ball off the pitch, so the carrier ends up alone,
/// shuffling it along the arena wall toward a goal it has no angle on. Measured, the ball was
/// spending up to half of some matches out there, and those were exactly the matches that went
/// a hundred seconds without a goal. Everything the carrier does when this far out is aimed
/// back toward the middle.
const WIDE_Z: f32 = FIELD_DEPTH / 2.0 * 0.72;
/// How close a teammate must be to the ball, relative to the carrier, to take the role over.
const CARRIER_MARGIN: f32 = 0.25;
/// Shortest time a player holds the carrier role before it can move, in seconds. Together with
/// `CARRIER_MARGIN` this is what stops two near-equidistant cubes trading the role every frame
/// and spinning on the spot.
const CARRIER_DWELL: f32 = 0.35;
/// Teammates nearer than this push each other apart.
const SEPARATION_RADIUS: f32 = 4.0;
/// How near a target a player stops, and starts easing rather than running flat out.
const ARRIVE_RADIUS: f32 = 2.4;
const STOP_RADIUS: f32 = 0.5;
/// Below this movement magnitude a player is not trying to go anywhere.
const INTENT_EPSILON: f32 = 0.15;

/// How long a player may want to move but not move before it counts as stuck.
const STUCK_SECS: f32 = 0.7;
/// How long the freeing sidestep lasts once it starts.
const EVADE_SECS: f32 = 0.55;
/// Speed below which a player is not making progress.
const STUCK_SPEED: f32 = 1.2;
/// Speed below which the ball counts as stopped.
const STALL_SPEED: f32 = 2.0;
/// How long a stopped ball is left alone before the nearest player is sent to hit it.
const STALL_KICK_SECS: f32 = 1.0;
/// How long a stopped ball is left alone before it is put back into play directly.
const STALL_RESCUE_SECS: f32 = 2.4;
/// The same two stages for a ball that is still moving but out beyond the touchlines. Longer,
/// because the carrier is already being told to bring it back in and usually manages to.
const STRANDED_KICK_SECS: f32 = 1.8;
const STRANDED_RESCUE_SECS: f32 = 3.5;

fn xz(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn own_goal_x(team: Team) -> f32 {
    match team {
        Team::Orange => -FIELD_WIDTH / 2.0,
        Team::Blue => FIELD_WIDTH / 2.0,
    }
}

fn opp_goal_x(team: Team) -> f32 {
    -own_goal_x(team)
}

/// `+1` if this team attacks toward `+x`, `-1` otherwise.
fn attack_dir(team: Team) -> f32 {
    opp_goal_x(team).signum()
}

fn clamp_to_pitch(mut p: Vec3) -> Vec3 {
    p.x = p.x.clamp(-FIELD_WIDTH / 2.0 + 1.0, FIELD_WIDTH / 2.0 - 1.0);
    p.z = p.z.clamp(-FIELD_DEPTH / 2.0 + 1.0, FIELD_DEPTH / 2.0 - 1.0);
    p
}

// === The derived style ===

/// The behavioural knobs play needs, derived from the coached [`TacticParams`].
///
/// These are deliberately *not* fields on `TacticParams`. Those seven numbers are a fixed
/// interface -- the training environment's observation is sized from them -- so anything new
/// is computed from them here instead, where it costs nothing outside this module.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayStyle {
    /// Distance from the opponent goal within which the carrier will shoot.
    pub shoot_range: f32,
    /// Appetite for a tight passing lane, `0` cautious to `1` reckless.
    pub pass_risk: f32,
    /// Preference for a pass that gains ground over one that keeps the ball.
    pub directness: f32,
    /// Distance at which off-ball players leave their shape to hunt the ball.
    pub press_trigger: f32,
    /// How many players besides the carrier hunt the ball.
    pub hunters: usize,
    /// Whole-block shift toward the opponent goal, in metres, signed for the team.
    pub line_x: f32,
    /// Lateral spread of the support lanes.
    pub width: f32,
    /// Teammate separation strength.
    pub spacing: f32,
    /// Fraction of `(own goal - ball)` that cover players drop toward.
    pub defender_depth: f32,
    /// Fraction of `(opp goal - ball)` that attackers push toward.
    pub attacker_push: f32,
    /// Fraction of off-ball players who play as attackers.
    pub commitment: f32,
}

impl PlayStyle {
    pub fn from_params(p: &TacticParams, team: Team) -> Self {
        Self {
            // Generous, but not a licence to shoot from your own half. A side that will only
            // shoot from close in has to walk the ball through five defenders in a small
            // space, and that produced a goalless midfield scrum; at twenty metres and up,
            // both sides simply shell the empty net from the halfway line and the match stops
            // being soccer. Sixteen or so puts the shot at the edge of the attacking third.
            shoot_range: 12.0 + p.attacker_push * 10.0 + p.line_height * 6.0,
            // A pressing, committed side gambles on passes; a low block does not.
            pass_risk: (0.25 + p.press * 0.5 + (p.commitment - 0.5) * 0.6).clamp(0.05, 0.95),
            directness: (0.35 + p.line_height * 0.8 + (p.attacker_push - 0.4) * 0.8)
                .clamp(0.10, 1.0),
            press_trigger: 6.0 + p.press * 14.0,
            // Rounded from a base of a little over half a player, so every tactic sends
            // someone to contest and a pressing one sends a pack.
            hunters: (0.6 + p.press * 2.6).round().max(0.0) as usize,
            // Damped from the raw parameter. Taken literally, a low block's -0.3 line and 0.85
            // depth together park all five players on their own goal line, where they have
            // nobody to pass to and simply concede the pitch. Compressed, the tactic still
            // reads as the deepest of the four without taking itself out of the game.
            line_x: p.line_height * (FIELD_WIDTH / 2.0) * 0.55 * attack_dir(team),
            width: p.width,
            spacing: p.spacing,
            defender_depth: 0.25 + p.defender_depth * 0.5,
            attacker_push: p.attacker_push,
            commitment: p.commitment,
        }
    }
}

// === World snapshot ===

/// One player, as the brain sees it.
#[derive(Clone, Copy, Debug)]
pub struct PlayerView {
    pub team: Team,
    pub index: usize,
    pub pos: Vec3,
    pub vel: Vec3,
}

/// Everything the brain reads in a frame.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub ball_pos: Vec3,
    pub ball_vel: Vec3,
    pub players: Vec<PlayerView>,
}

impl Snapshot {
    fn team_of(&self, team: Team) -> Vec<PlayerView> {
        self.players.iter().copied().filter(|p| p.team == team).collect()
    }

    fn positions_of(&self, team: Team) -> Vec<Vec3> {
        self.players.iter().filter(|p| p.team == team).map(|p| p.pos).collect()
    }
}

/// What one player does this frame.
#[derive(Clone, Copy, Debug)]
pub struct Decision {
    pub role: Role,
    pub movement: Vec2,
    pub jump: bool,
    pub fire: bool,
    pub kick: Option<KickRequest>,
}

impl Default for Decision {
    fn default() -> Self {
        Self { role: Role::Support, movement: Vec2::ZERO, jump: false, fire: false, kick: None }
    }
}

/// The role a player is filling this frame.
///
/// Carried on the [`Decision`] rather than kept private, because "how many players left the
/// shape to chase" is the thing a pressing tactic is judged by, and reading it back is the
/// only honest way to assert that a tactic does what it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The one player going to the ball.
    Carrier,
    /// Leaving shape to contest the ball.
    Hunter,
    /// Offering a forward passing option.
    Support,
    /// Holding the lane back to the team's own goal.
    Cover,
}

// === Prediction and geometry ===

/// Where the ball will be in `t` seconds, given its linear damping.
///
/// Integrating the damping matters: a ball struck at shot speed travels a good deal less far
/// than `pos + vel * t` claims, and a receiver who runs to that spot arrives well past it.
pub fn predict_ball(pos: Vec3, vel: Vec3, t: f32) -> Vec3 {
    let d = BALL_LINEAR_DAMPING;
    let factor = if d.abs() < 1e-4 { t } else { (1.0 - (-d * t).exp()) / d };
    let flat = Vec3::new(vel.x, 0.0, vel.z);
    Vec3::new(pos.x, pos.y, pos.z) + flat * factor
}

/// Roughly how long this player needs to reach the ball, chasing its predicted path.
///
/// Distance alone picks the wrong chaser: a player running away from a ball that is rolling
/// toward a teammate is often the nearest one, and taking the role locks the teammate out of
/// it. A few fixed-point passes converge well enough for a decision.
pub fn intercept_time(player_pos: Vec3, ball_pos: Vec3, ball_vel: Vec3) -> f32 {
    let mut t = 0.0;
    for _ in 0..4 {
        let aim = predict_ball(ball_pos, ball_vel, t);
        t = (xz(aim) - xz(player_pos)).length() / CUBE_MAX_SPEED.max(1e-3);
    }
    t
}

/// How open the line from `from` to `to` is, `0` blocked to `1` clear.
///
/// A blocker only counts while it stands between the two points: one behind the passer, or
/// beyond the target, blocks nothing.
pub fn lane_clearance(from: Vec3, to: Vec3, blockers: &[Vec3], radius: f32) -> f32 {
    let a = xz(from);
    let b = xz(to);
    let ab = b - a;
    let len = ab.length();
    if len < 1e-3 {
        return 1.0;
    }
    let dir = ab / len;
    let mut worst = 1.0f32;
    for blocker in blockers {
        let rel = xz(*blocker) - a;
        let along = rel.dot(dir);
        if along <= 0.3 || along >= len {
            continue;
        }
        let perp = (rel - dir * along).length();
        worst = worst.min((perp / radius).clamp(0.0, 1.0));
    }
    worst
}

fn nearest_distance(point: Vec3, others: &[Vec3]) -> f32 {
    others
        .iter()
        .map(|o| (xz(*o) - xz(point)).length())
        .fold(f32::INFINITY, f32::min)
}

/// The best point of the goal mouth to shoot at, and how open its lane is.
///
/// Candidates are spaced across the scorable width -- inside `GOAL_DEPTH/2`, which is the
/// window `detect_goals` actually counts -- so a shot aimed here is a shot that can score.
///
/// Several candidates are usually equally unobstructed, and ranking on lane clearance alone
/// then settles every one of those ties the same way, which aims a whole match's shots at the
/// same post. A small bonus for standing clear of the nearest body breaks the tie by aiming at
/// the part of the goal least well covered, which is also the better shot.
pub fn shot_target(team: Team, from: Vec3, blockers: &[Vec3]) -> (Vec3, f32) {
    let goal_x = opp_goal_x(team);
    let half = GOAL_DEPTH / 2.0 - 0.9;
    let mut best = (Vec3::new(goal_x, ground_ball_height(), 0.0), 0.0, f32::NEG_INFINITY);
    for step in -2..=2 {
        let z = half * step as f32 / 2.0;
        let target = Vec3::new(goal_x, ground_ball_height(), z);
        let clearance = lane_clearance(from, target, blockers, LANE_RADIUS);
        let uncovered = (nearest_distance(target, blockers) / 12.0).clamp(0.0, 1.0);
        let score = clearance + 0.15 * uncovered;
        if score > best.2 {
            best = (target, clearance, score);
        }
    }
    (best.0, best.1)
}

/// How good a pass from the ball to `receiver` would be, in `0..=1`.
///
/// Combines four things a passer actually weighs: whether the lane is open, whether the
/// receiver is marked, whether the distance is a sensible one to strike, and how much ground
/// the pass gains. `directness` decides how much that last one counts.
pub fn pass_score(
    team: Team,
    ball_pos: Vec3,
    receiver: Vec3,
    opponents: &[Vec3],
    directness: f32,
) -> f32 {
    let clearance = lane_clearance(ball_pos, receiver, opponents, LANE_RADIUS);
    if clearance <= 0.01 {
        return 0.0;
    }
    let distance = (xz(receiver) - xz(ball_pos)).length();
    // Too short is not a pass; too long will not arrive. Peak around a ball played into the
    // next third.
    let range = if distance < 3.5 {
        0.15
    } else {
        (1.0 - ((distance - 14.0).abs() / 22.0)).clamp(0.15, 1.0)
    };
    let openness = (nearest_distance(receiver, opponents) / 8.0).clamp(0.0, 1.0);
    let gain = ((receiver.x - ball_pos.x) * attack_dir(team)) / (FIELD_WIDTH * 0.5);
    let forward = (0.5 + 0.5 * gain).clamp(0.0, 1.0);
    let weight = 0.25 + 0.75 * directness;
    let shape = (1.0 - weight) + weight * forward;
    // A ball played further from the middle than it already is heads for the run-off beside
    // the pitch, where it stops being playable at all.
    let infield = if receiver.z.abs() <= ball_pos.z.abs() {
        1.0
    } else {
        (1.0 - (receiver.z.abs() - ball_pos.z.abs()) / (FIELD_DEPTH * 0.5)).clamp(0.25, 1.0)
    };

    clearance * clearance * range * (0.30 + 0.70 * openness) * shape * infield
}

// === Shape ===

/// Whether support slot `slot` of `n` plays as an attacker under `commitment`.
///
/// The deepest slots convert first, so raising commitment pushes the back of the block up
/// rather than shuffling the middle.
fn is_attacker(slot: usize, n: usize, commitment: f32) -> bool {
    if n == 0 {
        return false;
    }
    let attackers = (commitment * n as f32).round().clamp(0.0, n as f32) as usize;
    slot >= n - attackers
}

/// Where an off-ball player is supposed to be, before it looks for space.
///
/// Lanes are spread across the pitch by `width` and anchored on the ball's half, so the team
/// keeps a shape instead of forming a column down the middle.
pub fn shape_home(
    team: Team,
    slot: usize,
    slots: usize,
    attacker: bool,
    ball_pos: Vec3,
    style: &PlayStyle,
) -> Vec3 {
    let lane = if slots <= 1 {
        0.0
    } else {
        (slot as f32 / (slots - 1) as f32) * 2.0 - 1.0
    };
    // Capped, so even the widest tactic keeps its lanes inside the touchlines. Uncapped,
    // Wing Play's 1.6 puts the outside lanes past the edge of the pitch, and the players told
    // to stand there take the ball out there with them.
    let spread = (FIELD_DEPTH * 0.28 * style.width).min(FIELD_DEPTH * 0.40);
    // Lean toward the ball's side of the pitch without abandoning the far lane.
    let z = lane * spread + ball_pos.z * 0.25;

    let x = if attacker {
        ball_pos.x + (opp_goal_x(team) - ball_pos.x) * style.attacker_push
    } else {
        ball_pos.x + (own_goal_x(team) - ball_pos.x) * style.defender_depth
    } + style.line_x;

    clamp_to_pitch(Vec3::new(x, ball_pos.y, z))
}

/// Pick the best receiving spot near `home`.
///
/// Candidates are a fixed cross plus diagonals -- no sampling, so the choice is reproducible.
/// A spot is good when the carrier can find it, it is not crowded, and it gains ground.
fn find_space(
    team: Team,
    home: Vec3,
    ball_pos: Vec3,
    teammates: &[Vec3],
    opponents: &[Vec3],
    style: &PlayStyle,
) -> Vec3 {
    const OFFSETS: [(f32, f32); 9] = [
        (0.0, 0.0),
        (3.5, 0.0),
        (-3.5, 0.0),
        (0.0, 4.0),
        (0.0, -4.0),
        (2.5, 3.0),
        (2.5, -3.0),
        (-2.5, 3.0),
        (-2.5, -3.0),
    ];
    let attack = attack_dir(team);
    let mut best = (home, f32::NEG_INFINITY);
    for (dx, dz) in OFFSETS {
        let candidate = clamp_to_pitch(Vec3::new(home.x + dx * attack, home.y, home.z + dz));
        let open_to_ball = lane_clearance(ball_pos, candidate, opponents, LANE_RADIUS);
        let room = (nearest_distance(candidate, opponents) / 9.0).clamp(0.0, 1.0);
        let apart = (nearest_distance(candidate, teammates) / SEPARATION_RADIUS).clamp(0.0, 1.0);
        let gain = ((candidate.x - home.x) * attack) / 8.0;
        let score = open_to_ball * 1.1 + room * 0.8 + apart * 0.7 + gain * 0.4 * style.directness;
        if score > best.1 {
            best = (candidate, score);
        }
    }
    best.0
}

// === Steering ===

/// Movement toward `target`, easing in as it is approached so a player settles instead of
/// jittering across its spot, plus separation from `crowd`.
fn steer(pos: Vec3, target: Vec3, crowd: &[Vec3], spacing: f32) -> Vec2 {
    let to_target = xz(target) - xz(pos);
    let distance = to_target.length();
    let mut desired = if distance <= STOP_RADIUS {
        Vec2::ZERO
    } else {
        let speed = (distance / ARRIVE_RADIUS).clamp(0.0, 1.0);
        to_target.normalize_or_zero() * speed
    };

    let mut push = Vec2::ZERO;
    for other in crowd {
        let away = xz(pos) - xz(*other);
        let d = away.length();
        if d > 1e-3 && d < SEPARATION_RADIUS {
            push += away.normalize() * ((SEPARATION_RADIUS - d) / SEPARATION_RADIUS);
        }
    }
    desired += push * (0.7 * spacing);

    if desired.length() > 1.0 {
        desired.normalize()
    } else {
        desired
    }
}

// === Memory ===

#[derive(Clone, Copy, Debug, Default)]
struct StuckMemory {
    still_for: f32,
    evade_for: f32,
    evade_sign: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct TeamMemory {
    carrier: Option<usize>,
    held_for: f32,
}

/// Everything the brain remembers between frames.
///
/// A resource rather than a `Local` so a test can read what it decided, and so the same state
/// is shared by the one system that needs it.
#[derive(Resource, Default)]
pub struct PlayMemory {
    teams: HashMap<Team, TeamMemory>,
    stuck: HashMap<(Team, usize), StuckMemory>,
    /// How long the ball has been sitting still.
    ball_stalled_for: f32,
}

impl PlayMemory {
    pub fn carrier(&self, team: Team) -> Option<usize> {
        self.teams.get(&team).and_then(|t| t.carrier)
    }

    pub fn ball_stalled_for(&self) -> f32 {
        self.ball_stalled_for
    }

    /// Forget everything. Called on a goal or round reset, where positions jump and any
    /// remembered role or stall is about a match state that no longer exists.
    pub fn clear(&mut self) {
        self.teams.clear();
        self.stuck.clear();
        self.ball_stalled_for = 0.0;
    }
}

/// Choose the player going to the ball.
///
/// Sticky on purpose: the role only moves when a teammate is clearly quicker to the ball *and*
/// the current carrier has held it long enough. Without both, two cubes at similar range swap
/// the role every frame, and what that looks like on the pitch is a pair of players spinning
/// beside a ball neither of them takes.
fn pick_carrier(
    players: &[PlayerView],
    ball_pos: Vec3,
    ball_vel: Vec3,
    memory: &mut TeamMemory,
    dt: f32,
) -> Option<usize> {
    if players.is_empty() {
        return None;
    }
    let mut quickest = players[0];
    let mut quickest_time = intercept_time(quickest.pos, ball_pos, ball_vel);
    for player in &players[1..] {
        let t = intercept_time(player.pos, ball_pos, ball_vel);
        if t < quickest_time {
            quickest = *player;
            quickest_time = t;
        }
    }

    let current = memory.carrier.filter(|i| players.iter().any(|p| p.index == *i));
    match current {
        Some(index) => {
            memory.held_for += dt;
            let held = players.iter().find(|p| p.index == index).expect("filtered above");
            let current_time = intercept_time(held.pos, ball_pos, ball_vel);
            let clearly_quicker = quickest_time + CARRIER_MARGIN < current_time;
            if clearly_quicker && memory.held_for >= CARRIER_DWELL {
                memory.carrier = Some(quickest.index);
                memory.held_for = 0.0;
            }
        }
        None => {
            memory.carrier = Some(quickest.index);
            memory.held_for = 0.0;
        }
    }
    memory.carrier
}

// === The on-ball ladder ===

/// What the carrier does with the ball. Evaluated in order; the first that applies wins.
fn carrier_decision(
    team: Team,
    me: &PlayerView,
    snapshot: &Snapshot,
    style: &PlayStyle,
    can_kick: bool,
) -> Decision {
    let ball = snapshot.ball_pos;
    let opponents = snapshot.positions_of(team.opponent());
    let attack = attack_dir(team);
    let in_range = can_kick && within_kick_range(me.pos, ball);

    // 1. Shoot. Close enough to the opponent goal, with a lane into the mouth.
    let to_goal = (opp_goal_x(team) - ball.x).abs();
    if in_range && to_goal <= style.shoot_range {
        let (target, clearance) = shot_target(team, ball, &opponents);
        // The nearer the goal, the worse a lane the shot is worth taking anyway.
        let needed = (to_goal / style.shoot_range.max(1e-3)) * 0.35;
        if clearance >= needed {
            return Decision {
                movement: steer(me.pos, ball, &[], style.spacing),
                kick: Some(KickRequest { dir: target - ball, speed: SHOT_SPEED }),
                ..Default::default()
            };
        }
    }

    // 2. Pass, to the best-placed teammate.
    if in_range {
        let mut best: Option<(Vec3, f32)> = None;
        for mate in snapshot.team_of(team) {
            if mate.index == me.index {
                continue;
            }
            let flight = (xz(mate.pos) - xz(ball)).length() / PASS_SPEED;
            let lead = clamp_to_pitch(mate.pos + Vec3::new(mate.vel.x, 0.0, mate.vel.z) * flight);
            let score = pass_score(team, ball, lead, &opponents, style.directness);
            if best.map_or(true, |(_, s)| score > s) {
                best = Some((lead, score));
            }
        }
        // A bolder tactic settles for a tighter lane. Low overall: a side that only plays the
        // certain pass keeps the ball in its own half all match, and the ball moving between
        // players is what the tactic is visible in.
        let threshold = 0.05 + 0.20 * (1.0 - style.pass_risk);
        if let Some((target, score)) = best {
            if score >= threshold {
                return Decision {
                    movement: steer(me.pos, ball, &[], style.spacing),
                    kick: Some(KickRequest { dir: target - ball, speed: PASS_SPEED }),
                    ..Default::default()
                };
            }
        }
    }

    // 3. Clear. Pinned in our own third with nothing on, so put it up the pitch rather than
    //    trying to play through the pressure. Up the middle, not down the line: with no
    //    throw-ins, a ball put into the run-off beside the pitch is a ball out of the game.
    let own_third = (ball.x - own_goal_x(team)) * attack < FIELD_WIDTH / 3.0;
    let pressured = nearest_distance(ball, &opponents) < PRESSURE_RADIUS;
    if in_range && own_third && pressured {
        let target = clamp_to_pitch(Vec3::new(
            own_goal_x(team) + attack * FIELD_WIDTH * 0.7,
            ground_ball_height(),
            ball.z * 0.25,
        ));
        return Decision {
            movement: steer(me.pos, ball, &[], style.spacing),
            kick: Some(KickRequest { dir: target - ball, speed: CLEAR_SPEED }),
            ..Default::default()
        };
    }

    // 4. Come inside. Too wide to shoot and with no pass on, the only thing worth doing with
    //    the ball is getting it back into the middle of the pitch where the team is.
    if in_range && ball.z.abs() > WIDE_Z {
        let target = clamp_to_pitch(Vec3::new(
            ball.x + attack * 9.0,
            ground_ball_height(),
            ball.z * 0.15,
        ));
        return Decision {
            movement: steer(me.pos, ball, &[], style.spacing),
            kick: Some(KickRequest { dir: target - ball, speed: PASS_SPEED }),
            ..Default::default()
        };
    }

    // 5. Dribble. Stand behind the ball relative to where it should go and push it there,
    //    steering off the nearest opponent, with a touch to keep it moving.
    let (goal_target, _) = shot_target(team, ball, &opponents);
    let mut desired = (xz(goal_target) - xz(ball)).normalize_or_zero();
    // Drift back toward the middle as the touchline nears, so a dribble never ends up running
    // the ball along the wall.
    let wideness = ((ball.z.abs() - WIDE_Z * 0.6) / (FIELD_DEPTH / 2.0)).clamp(0.0, 1.0);
    if wideness > 0.0 {
        let inward = Vec2::new(0.0, -ball.z.signum());
        desired = (desired + inward * wideness * 1.3).normalize_or_zero();
    }
    let nearest_opp = opponents
        .iter()
        .copied()
        .min_by(|a, b| {
            (xz(*a) - xz(ball))
                .length()
                .partial_cmp(&(xz(*b) - xz(ball)).length())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(Vec3::new(goal_target.x, ball.y, goal_target.z));
    let to_opp = xz(nearest_opp) - xz(ball);
    if to_opp.length() < 6.0 && to_opp.length() > 1e-3 {
        // Slide around, not through: push perpendicular to the opponent, on the side with
        // more pitch left.
        let side = Vec2::new(-to_opp.y, to_opp.x).normalize();
        let side = if (ball.z + side.y * 4.0).abs() > FIELD_DEPTH / 2.0 - 2.0 { -side } else { side };
        desired = (desired + side * 0.9).normalize_or_zero();
    }
    let desired3 = Vec3::new(desired.x, 0.0, desired.y);
    let push_point = ball - desired3 * PUSH_OFFSET;

    let behind_ball = (xz(me.pos) - xz(push_point)).length() < 1.1;
    let movement = if behind_ball {
        steer(me.pos, ball + desired3 * 4.0, &[], style.spacing)
    } else {
        steer(me.pos, push_point, &[], style.spacing)
    };

    let ball_speed = Vec2::new(snapshot.ball_vel.x, snapshot.ball_vel.z).length();
    let touch = if in_range && behind_ball && ball_speed < TOUCH_BALL_SPEED {
        Some(KickRequest { dir: desired3, speed: PASS_SPEED * TOUCH_FRACTION })
    } else {
        None
    };

    Decision { role: Role::Carrier, movement, jump: false, fire: false, kick: touch }
}

// === The whole team ===

/// Decide what every player on `team` does this frame.
///
/// Returned by `player.index`, so the caller can write it back without depending on ordering.
pub fn decide_team(
    team: Team,
    snapshot: &Snapshot,
    style_for: &dyn Fn(usize) -> PlayStyle,
    base_style: &PlayStyle,
    memory: &mut PlayMemory,
    dt: f32,
    can_kick: bool,
) -> HashMap<usize, Decision> {
    let mut out = HashMap::new();
    let mates = snapshot.team_of(team);
    if mates.is_empty() {
        return out;
    }

    let ball = snapshot.ball_pos;
    let opponents = snapshot.positions_of(team.opponent());
    let team_memory = memory.teams.entry(team).or_default();
    let carrier = pick_carrier(&mates, ball, snapshot.ball_vel, team_memory, dt);

    // Off-ball players, ordered by index so lane assignment is stable frame to frame.
    let mut off_ball: Vec<PlayerView> =
        mates.iter().copied().filter(|p| Some(p.index) != carrier).collect();
    off_ball.sort_by_key(|p| p.index);

    // The nearest few hunt the ball; the rest hold shape. Hunting is what a pressing tactic
    // looks like from the stands.
    let mut by_distance: Vec<usize> = (0..off_ball.len()).collect();
    by_distance.sort_by(|&a, &b| {
        (xz(off_ball[a].pos) - xz(ball))
            .length()
            .partial_cmp(&(xz(off_ball[b].pos) - xz(ball)).length())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let hunters: Vec<usize> = by_distance
        .iter()
        .copied()
        .filter(|&i| (xz(off_ball[i].pos) - xz(ball)).length() <= base_style.press_trigger)
        .take(base_style.hunters)
        .collect();

    let slots = off_ball.len();
    for (slot, me) in off_ball.iter().enumerate() {
        let style = style_for(me.index);
        let crowd: Vec<Vec3> =
            mates.iter().filter(|p| p.index != me.index).map(|p| p.pos).collect();

        let (role, target) = if hunters.contains(&slot) {
            // Goal-side and standing off, not straight at the ball. Three players converging
            // on the same point is not a press, it is a scrum, and what a scrum does is squirt
            // the ball off the side of the pitch -- which is where the pressing matchups were
            // spending nearly forty percent of the match. Taking the ball is the carrier's
            // job; these two cut off the way forward while it does.
            let aim = predict_ball(ball, snapshot.ball_vel, 0.25);
            let goal_side = Vec3::new(own_goal_x(team) - aim.x, 0.0, 0.0).normalize_or_zero();
            (Role::Hunter, clamp_to_pitch(aim + goal_side * PRESS_STANDOFF))
        } else {
            let attacker = is_attacker(slot, slots, base_style.commitment);
            let home = shape_home(team, slot, slots, attacker, ball, &style);
            let spot = find_space(team, home, ball, &crowd, &opponents, &style);
            (if attacker { Role::Support } else { Role::Cover }, spot)
        };

        let movement = steer(me.pos, target, &crowd, style.spacing);
        out.insert(
            me.index,
            Decision { role, movement, jump: false, fire: false, kick: None },
        );
    }

    if let Some(index) = carrier {
        if let Some(me) = mates.iter().find(|p| p.index == index) {
            let style = style_for(index);
            let mut decision = carrier_decision(team, me, snapshot, &style, can_kick);
            decision.role = Role::Carrier;
            // Jump for a ball bouncing overhead, the one case where height helps.
            decision.jump = ball.y > me.pos.y + 0.9 && (xz(ball) - xz(me.pos)).length() < 3.0;
            out.insert(index, decision);
        }
    }

    // Superpowers, aimed rather than sprayed: fire only with an opponent close and roughly
    // where the player is heading, which is the cone every offensive power uses.
    for me in &mates {
        let Some(decision) = out.get_mut(&me.index) else { continue };
        let heading = decision.movement;
        if heading.length() < INTENT_EPSILON {
            continue;
        }
        let heading = heading.normalize();
        decision.fire = opponents.iter().any(|opp| {
            let to_opp = xz(*opp) - xz(me.pos);
            let d = to_opp.length();
            d > 1e-3 && d < 9.0 && to_opp.normalize().dot(heading) > 0.65
        });
    }

    out
}

// === Stuck recovery ===

/// Redirect a player that wants to move but is not moving.
///
/// Returns the movement to use and whether to jump. The sidestep direction comes from the
/// player's index, not from a random number, so a replay of the same match recovers the same
/// way and two wedged teammates break apart instead of shuffling in step.
fn unstick(
    key: (Team, usize),
    wanted: Vec2,
    speed: f32,
    memory: &mut PlayMemory,
    dt: f32,
) -> (Vec2, bool) {
    let entry = memory.stuck.entry(key).or_default();

    if entry.evade_for > 0.0 {
        entry.evade_for -= dt;
        let base = if wanted.length() > INTENT_EPSILON { wanted.normalize() } else { Vec2::X };
        let side = Vec2::new(-base.y, base.x) * entry.evade_sign;
        return ((base * 0.35 + side).normalize_or_zero(), true);
    }

    let trying = wanted.length() > INTENT_EPSILON;
    if trying && speed < STUCK_SPEED {
        entry.still_for += dt;
    } else {
        entry.still_for = 0.0;
    }

    if entry.still_for >= STUCK_SECS {
        entry.still_for = 0.0;
        entry.evade_for = EVADE_SECS;
        entry.evade_sign = if key.1 % 2 == 0 { 1.0 } else { -1.0 };
        let base = wanted.normalize_or_zero();
        let side = Vec2::new(-base.y, base.x) * entry.evade_sign;
        return ((base * 0.35 + side).normalize_or_zero(), true);
    }

    (wanted, false)
}

/// Whether a stopped ball should be hit, rescued, or left alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stall {
    None,
    /// Send the nearest player to strike it.
    Kick,
    /// Put it back into play directly -- nobody has managed to.
    Rescue,
}

/// Track a ball that is out of the game and say what to do about it.
///
/// Two ways a ball leaves the game, and both used to last until the round timer ran out.
/// It can stop -- wedged in a corner or trapped under a pile of cubes. Or it can keep moving
/// but out in the run-off beside the pitch, where nobody supports it and the carrier just
/// shuffles it along the arena wall; that one does not look stuck at all, which is why it went
/// unnoticed, and it cost whole minutes of some matches.
///
/// Either way: first ask the nearest player to hit it, and if that has not worked, put it back
/// into play outright, so a match can never deadlock.
pub fn note_ball_stall(memory: &mut PlayMemory, ball_pos: Vec3, ball_vel: Vec3, dt: f32) -> Stall {
    let stopped = Vec2::new(ball_vel.x, ball_vel.z).length() < STALL_SPEED;
    let stranded = ball_pos.z.abs() > FIELD_DEPTH / 2.0;
    if !stopped && !stranded {
        memory.ball_stalled_for = 0.0;
        return Stall::None;
    }
    // A ball that is merely out wide is given longer than one that has stopped dead: it is
    // still in play, and the carrier is already being told to bring it back in.
    let (kick_after, rescue_after) = if stopped {
        (STALL_KICK_SECS, STALL_RESCUE_SECS)
    } else {
        (STRANDED_KICK_SECS, STRANDED_RESCUE_SECS)
    };

    memory.ball_stalled_for += dt;
    if memory.ball_stalled_for >= rescue_after {
        memory.ball_stalled_for = 0.0;
        Stall::Rescue
    } else if memory.ball_stalled_for >= kick_after {
        Stall::Kick
    } else {
        Stall::None
    }
}

// === The system ===

/// System: drive every `AiControlled` cube.
pub fn apply_soccer_ai(
    time: Res<Time>,
    tactics: Option<Res<TeamTactics>>,
    difficulty: Option<Res<HeuristicDifficulty>>,
    mut memory: ResMut<PlayMemory>,
    mut ball_query: Query<(&Transform, &mut Velocity), (With<Ball>, Without<CubePlayer>)>,
    mut player_query: Query<
        (&mut PlayerInput, &Transform, &Velocity, &CubePlayer),
        With<AiControlled>,
    >,
) {
    let dt = time.delta_seconds();
    let diff = difficulty.map(|d| d.0).unwrap_or(1.0).clamp(0.0, 1.0);
    let Ok((ball_transform, mut ball_velocity)) = ball_query.get_single_mut() else { return };
    let ball_pos = ball_transform.translation;
    let ball_vel = ball_velocity.linvel;

    let snapshot = Snapshot {
        ball_pos,
        ball_vel,
        players: player_query
            .iter()
            .map(|(_, transform, velocity, player)| PlayerView {
                team: player.team,
                index: player.index,
                pos: transform.translation,
                vel: velocity.linvel,
            })
            .collect(),
    };

    let stall = note_ball_stall(&mut memory, ball_pos, ball_vel, dt);
    if stall == Stall::Rescue {
        // Nobody freed it. Send it back toward the middle of the pitch, away from whichever
        // wall it is against, so play restarts without a reset. The lateral component is what
        // matters -- that is the direction it is stuck in -- so it is weighted much harder
        // than bringing it back up the pitch.
        let toward_centre = Vec3::new(-ball_pos.x * 0.25, 0.0, -ball_pos.z).normalize_or_zero();
        let direction = if toward_centre.length_squared() < 1e-6 { Vec3::X } else { toward_centre };
        ball_velocity.linvel = direction * (CLEAR_SPEED * 0.6) + Vec3::Y * 2.0;
        memory.clear();
        return;
    }

    let default_tactics = TeamTactics::default();
    let tactics = tactics.as_deref().unwrap_or(&default_tactics);

    let mut decisions: HashMap<(Team, usize), Decision> = HashMap::new();
    for team in [Team::Orange, Team::Blue] {
        let directive = match team {
            Team::Orange => &tactics.orange,
            Team::Blue => &tactics.blue,
        };
        let base_style = PlayStyle::from_params(&directive.base_params(), team);
        let style_for =
            |index: usize| PlayStyle::from_params(&directive.params_for(index), team);
        let team_decisions = decide_team(
            team,
            &snapshot,
            &style_for,
            &base_style,
            &mut memory,
            dt,
            diff > 0.0,
        );
        for (index, decision) in team_decisions {
            decisions.insert((team, index), decision);
        }
    }

    // A ball nobody has reached: whoever is closest strikes it toward the middle. This runs
    // after the normal decisions so it overrides whatever that player was going to do.
    if stall == Stall::Kick {
        if let Some(nearest) = snapshot
            .players
            .iter()
            .filter(|p| within_kick_range(p.pos, ball_pos))
            .min_by(|a, b| {
                a.pos
                    .distance(ball_pos)
                    .partial_cmp(&b.pos.distance(ball_pos))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            let target = Vec3::new(
                opp_goal_x(nearest.team) * 0.5,
                ground_ball_height(),
                -ball_pos.z * 0.5,
            );
            if let Some(decision) = decisions.get_mut(&(nearest.team, nearest.index)) {
                decision.kick =
                    Some(KickRequest { dir: target - ball_pos, speed: CLEAR_SPEED });
            }
        }
    }

    for (mut input, transform, velocity, player) in player_query.iter_mut() {
        let Some(decision) = decisions.get(&(player.team, player.index)) else { continue };
        let speed = Vec2::new(velocity.linvel.x, velocity.linvel.z).length();
        let (movement, evade_jump) = unstick(
            (player.team, player.index),
            decision.movement,
            speed,
            &mut memory,
            dt,
        );
        let _ = transform;

        input.movement = movement * diff;
        input.jump = decision.jump || evade_jump;
        // Weak opponents don't use superpowers; they come online past half strength.
        input.fire = decision.fire && diff > 0.5;
        input.kick = if diff > 0.0 { decision.kick } else { None };
    }
}

/// System: forget roles and stalls, for a goal or round reset.
pub fn clear_play_memory(mut memory: ResMut<PlayMemory>) {
    memory.clear();
}

/// A headless match, for asserting on how the game actually plays.
///
/// The unit tests above pin individual judgements; this pins the thing those judgements are
/// for. `rl::sim::build_headless_app` is the same idea for the training environment, but it
/// chains that environment's controller and reward systems, so this builds its own app with
/// the same plugin set rather than bending that one.
#[cfg(test)]
pub mod harness {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::core::{TaskPoolOptions, TaskPoolPlugin};
    use bevy::ecs::schedule::{ExecutorKind, ScheduleLabel};
    use bevy::hierarchy::HierarchyPlugin;
    use bevy::scene::ScenePlugin;
    use bevy::time::TimeUpdateStrategy;
    use bevy::transform::TransformPlugin;
    use std::time::Duration;

    use crate::entities::{
        get_ball_spawn_position, get_spawn_position, spawn_arena, spawn_ball, spawn_field,
        spawn_goals, spawn_players,
    };
    use crate::game::{
        BallTouchedEvent, GameState, GoalScoredEvent, PHYSICS_TIMESTEP, PLAYERS_PER_TEAM,
    };
    use crate::systems::heuristic_ai::{Tactic, TeamDirective};
    use crate::systems::kick::{apply_kicks, tick_kick_cooldowns, KickCooldowns};
    use crate::systems::movement::{apply_player_movement, clamp_velocities};
    use crate::systems::physics::configure_physics;
    use crate::systems::possession::{tick_cooldowns, update_possession, Possession};
    use crate::systems::reset::kickoff_velocity;
    use crate::systems::scoring::{detect_goals_by_position, GoalHalfWidth};
    use crate::systems::status_effects::{
        apply_status_forces, tick_status_effects, ImpulseEvent,
    };
    use crate::systems::superpowers::{activate_superpowers, tick_superpower_cooldowns};

    /// What a simulated match looked like.
    #[derive(Debug, Default, Clone)]
    pub struct MatchReport {
        pub seconds: f32,
        pub goals_orange: u32,
        pub goals_blue: u32,
        /// Longest unbroken stretch with the ball barely moving.
        pub longest_ball_stall: f32,
        /// Longest stretch with no goal at either end.
        pub longest_goal_drought: f32,
        /// Metres the ball travelled per second of play.
        pub ball_metres_per_second: f32,
        /// Mean x of each team, signed so positive is toward the opponent goal.
        pub orange_mean_advance: f32,
        pub blue_mean_advance: f32,
        /// Mean distance of the ball from the centre line of the pitch.
        pub ball_mean_abs_z: f32,
        /// Fraction of play with the ball outside the touchlines, in the arena run-off.
        pub ball_off_pitch_fraction: f32,
        /// Fraction of play with the ball in the third Orange is attacking.
        ///
        /// Orange's end specifically, not "either final third": a side pinned in its own third
        /// is not the same as a side camped in its opponent's, and a symmetric measure scores
        /// both alike.
        pub ball_in_orange_third_fraction: f32,
    }

    impl MatchReport {
        pub fn goals(&self) -> u32 {
            self.goals_orange + self.goals_blue
        }

        pub fn seconds_per_goal(&self) -> f32 {
            if self.goals() == 0 { f32::INFINITY } else { self.seconds / self.goals() as f32 }
        }
    }

    #[derive(Resource, Default)]
    struct Tally {
        orange: u32,
        blue: u32,
        scored_this_tick: bool,
    }

    /// Count a goal and restart, the headless equivalent of `reset_after_goal` without the
    /// visual effects or the one-second pause. It takes the same kickoff, because a restart
    /// that leaves the ball dead is what made a match repeat one goal over and over.
    fn score_and_restart(
        mut goals: EventReader<GoalScoredEvent>,
        mut tally: ResMut<Tally>,
        mut state: ResMut<GameState>,
        mut memory: ResMut<PlayMemory>,
        mut possession: ResMut<Possession>,
        mut kicks: ResMut<KickCooldowns>,
        mut ball: Query<(&mut Transform, &mut Velocity), (With<Ball>, Without<CubePlayer>)>,
        mut players: Query<(&mut Transform, &mut Velocity, &CubePlayer), Without<Ball>>,
    ) {
        tally.scored_this_tick = false;
        let mut scored = false;
        for goal in goals.read() {
            match goal.scoring_team {
                Team::Orange => tally.orange += 1,
                Team::Blue => tally.blue += 1,
            }
            state.add_goal(goal.scoring_team);
            scored = true;
        }
        if !scored {
            return;
        }
        tally.scored_this_tick = true;

        if let Ok((mut transform, mut velocity)) = ball.get_single_mut() {
            transform.translation = get_ball_spawn_position();
            velocity.linvel = kickoff_velocity(
                state.last_scorer.map(|scorer| scorer.opponent()),
                state.goals_scored(),
            );
            velocity.angvel = Vec3::ZERO;
        }
        for (mut transform, mut velocity, player) in players.iter_mut() {
            transform.translation = get_spawn_position(player.team, player.index);
            velocity.linvel = Vec3::ZERO;
            velocity.angvel = Vec3::ZERO;
        }
        memory.clear();
        possession.holder = None;
        possession.cooldowns.clear();
        possession.steal_progress.clear();
        kicks.0.clear();
    }

    fn tag_everyone_ai(mut commands: Commands, players: Query<Entity, With<CubePlayer>>) {
        for entity in &players {
            commands.entity(entity).insert(AiControlled);
        }
    }

    fn set_fixed_timestep(mut config: ResMut<RapierConfiguration>) {
        config.timestep_mode =
            TimestepMode::Fixed { dt: PHYSICS_TIMESTEP, substeps: 1 };
    }

    /// Build the app. One `update()` is one deterministic physics tick.
    pub fn build_match_app(orange: Tactic, blue: Tactic) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(TaskPoolPlugin {
            task_pool_options: TaskPoolOptions::with_num_threads(1),
        }))
        .add_plugins(TransformPlugin)
        .add_plugins(HierarchyPlugin)
        .add_plugins(AssetPlugin::default())
        .add_plugins(ScenePlugin)
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default());

        for label in [
            First.intern(),
            PreUpdate.intern(),
            Update.intern(),
            PostUpdate.intern(),
            Last.intern(),
        ] {
            app.edit_schedule(label, |schedule| {
                schedule.set_executor_kind(ExecutorKind::SingleThreaded);
            });
        }

        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            PHYSICS_TIMESTEP,
        )));

        app.add_systems(
            Startup,
            (
                configure_physics,
                set_fixed_timestep,
                spawn_arena,
                spawn_field,
                spawn_goals,
                spawn_players,
                spawn_ball,
            ),
        )
        .add_systems(PostStartup, tag_everyone_ai);

        app.insert_resource(TeamTactics {
            orange: TeamDirective::uniform(orange.params()),
            blue: TeamDirective::uniform(blue.params()),
        })
        .init_resource::<GameState>()
        .init_resource::<Possession>()
        .init_resource::<KickCooldowns>()
        .init_resource::<PlayMemory>()
        .init_resource::<HeuristicDifficulty>()
        .init_resource::<GoalHalfWidth>()
        .init_resource::<Tally>()
        .add_event::<GoalScoredEvent>()
        .add_event::<BallTouchedEvent>()
        .add_event::<ImpulseEvent>();

        app.add_systems(
            Update,
            (
                apply_soccer_ai,
                tick_superpower_cooldowns,
                activate_superpowers,
                tick_status_effects,
                apply_player_movement,
                apply_status_forces,
                clamp_velocities,
                tick_kick_cooldowns,
                apply_kicks,
                tick_cooldowns,
                update_possession,
                detect_goals_by_position,
                score_and_restart,
            )
                .chain(),
        );

        app
    }

    /// Play a match and report on it.
    ///
    /// `variant` picks a different opening ball, which is the only way to get an independent
    /// sample: two matches played from the identical opening inside one process play out
    /// identically, so repeating one tells you nothing you did not already know.
    pub fn play(orange: Tactic, blue: Tactic, seconds: f32, variant: u32) -> MatchReport {
        let mut app = build_match_app(orange, blue);
        let ticks = (seconds / PHYSICS_TIMESTEP) as usize;
        // A few ticks to spawn, tag and settle before anything is measured.
        for _ in 0..10 {
            app.update();
        }
        if variant > 0 {
            let across = ((variant % 3) as f32 - 1.0) * 4.0;
            let along = if variant % 2 == 0 { 1.0 } else { -1.0 };
            let world = &mut app.world;
            let mut q = world.query_filtered::<(&mut Transform, &mut Velocity), With<Ball>>();
            if let Some((mut transform, mut velocity)) = q.iter_mut(world).next() {
                transform.translation = get_ball_spawn_position() + Vec3::new(0.0, 0.0, across);
                velocity.linvel = Vec3::new(along * 6.0, 0.0, -across);
            }
        }

        let mut report = MatchReport { seconds, ..Default::default() };
        let mut stall = 0.0f32;
        let mut drought = 0.0f32;
        let mut travelled = 0.0f32;
        let mut orange_advance = 0.0f64;
        let mut blue_advance = 0.0f64;
        let mut samples = 0u32;
        let mut abs_z = 0.0f64;
        let mut off_pitch = 0u32;
        let mut final_third = 0u32;
        let mut previous_ball: Option<Vec3> = None;

        for _ in 0..ticks {
            app.update();
            let world = &mut app.world;

            let (ball_pos, ball_vel) = {
                let mut q = world.query_filtered::<(&Transform, &Velocity), With<Ball>>();
                let Some((t, v)) = q.iter(world).next() else { continue };
                (t.translation, v.linvel)
            };
            let restarted = world.resource::<Tally>().scored_this_tick;

            if let Some(previous) = previous_ball {
                // A restart teleports the ball; that is not distance it travelled.
                if !restarted {
                    travelled += (xz(ball_pos) - xz(previous)).length();
                }
            }
            previous_ball = Some(ball_pos);

            if Vec2::new(ball_vel.x, ball_vel.z).length() < STALL_SPEED && !restarted {
                stall += PHYSICS_TIMESTEP;
                report.longest_ball_stall = report.longest_ball_stall.max(stall);
            } else {
                stall = 0.0;
            }

            if restarted {
                drought = 0.0;
            } else {
                drought += PHYSICS_TIMESTEP;
                report.longest_goal_drought = report.longest_goal_drought.max(drought);
            }

            abs_z += ball_pos.z.abs() as f64;
            if ball_pos.z.abs() > FIELD_DEPTH / 2.0 {
                off_pitch += 1;
            }
            if ball_pos.x * attack_dir(Team::Orange) > FIELD_WIDTH / 2.0 - 16.0 {
                final_third += 1;
            }

            let mut q = world.query::<(&Transform, &CubePlayer)>();
            for (transform, player) in q.iter(world) {
                let advance = transform.translation.x * attack_dir(player.team);
                match player.team {
                    Team::Orange => orange_advance += advance as f64,
                    Team::Blue => blue_advance += advance as f64,
                }
            }
            samples += 1;
        }

        let tally = app.world.resource::<Tally>();
        report.goals_orange = tally.orange;
        report.goals_blue = tally.blue;
        report.ball_metres_per_second = travelled / seconds.max(1e-3);
        if samples > 0 {
            let per_team = (samples as f64) * PLAYERS_PER_TEAM as f64;
            report.orange_mean_advance = (orange_advance / per_team) as f32;
            report.blue_mean_advance = (blue_advance / per_team) as f32;
            report.ball_mean_abs_z = (abs_z / samples as f64) as f32;
            report.ball_off_pitch_fraction = off_pitch as f32 / samples as f32;
            report.ball_in_orange_third_fraction = final_third as f32 / samples as f32;
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::heuristic_ai::Tactic;

    fn style(tactic: Tactic) -> PlayStyle {
        PlayStyle::from_params(&tactic.params(), Team::Orange)
    }

    /// The matchups the match-level assertions are made over. Every tactic against the
    /// Balanced opponent a solo coached match actually plays, plus the two extremes against
    /// each other.
    const MATCHUPS: [(Tactic, Tactic); 5] = [
        (Tactic::Balanced, Tactic::Balanced),
        (Tactic::HighPress, Tactic::Balanced),
        (Tactic::LowBlock, Tactic::Balanced),
        (Tactic::WingPlay, Tactic::Balanced),
        (Tactic::HighPress, Tactic::LowBlock),
    ];

    /// Two simulated minutes is long enough for the numbers to settle and short enough to run
    /// with the rest of the suite.
    const MATCH_SECONDS: f32 = 120.0;

    /// A coached match has to be a high-scoring game of soccer, whatever the two sides play.
    ///
    /// This is the floor the whole module exists to hold up, and the failure it guards against
    /// is a silent drift back to the goalless midfield scrum the positional AI produced.
    ///
    /// The bounds are deliberately loose. The decision logic is a pure function of the world,
    /// but a match is a chaotic physics simulation, and rapier's own bookkeeping is not
    /// bit-reproducible from one process to the next -- a single last-bit difference in the
    /// first second is a different match by the last. Measured repeatedly, the same build
    /// swings between about seven and twelve goals a match; these assertions sit well under
    /// that, so they catch a regression rather than the weather. The aggregate across all five
    /// matchups is the stable number, which is why the real rate is asserted there.
    #[test]
    fn a_coached_match_plays_high_scoring_soccer() {
        let mut total_goals = 0;
        for (orange, blue) in MATCHUPS {
            let report = super::harness::play(orange, blue, MATCH_SECONDS, 0);
            let label = format!("{} v {}", orange.name(), blue.name());
            total_goals += report.goals();

            assert!(
                report.goals() >= 2,
                "{label}: scored only {} in {MATCH_SECONDS}s",
                report.goals(),
            );
            assert!(
                report.longest_ball_stall <= 3.5,
                "{label}: ball sat still for {:.1}s",
                report.longest_ball_stall,
            );
            assert!(
                report.ball_off_pitch_fraction <= 0.45,
                "{label}: ball was outside the touchlines {:.0}% of the match",
                report.ball_off_pitch_fraction * 100.0,
            );
            assert!(
                report.ball_metres_per_second >= 6.0,
                "{label}: ball only travelled {:.1}m/s, play is not flowing",
                report.ball_metres_per_second,
            );
            assert!(
                report.longest_goal_drought <= MATCH_SECONDS * 0.8,
                "{label}: went {:.0}s without a goal",
                report.longest_goal_drought,
            );
        }

        // The arcade rate the game is tuned for: a goal every twenty to thirty seconds.
        let seconds_per_goal =
            MATCH_SECONDS * MATCHUPS.len() as f32 / total_goals.max(1) as f32;
        assert!(
            seconds_per_goal <= 30.0,
            "a goal only every {seconds_per_goal:.0}s across {} matches ({total_goals} goals)",
            MATCHUPS.len(),
        );
    }

    /// The coached tactic has to be visible in the match, not just in the parameters.
    ///
    /// Against the same Balanced opponent, the aggressive play must actually camp further up
    /// the pitch, and put the ball in the end it is attacking more often, than the defensive
    /// one. Every unit test above would still pass if the tactic had no effect on play at all;
    /// this is the one that would not.
    #[test]
    fn an_aggressive_play_is_visible_on_the_pitch() {
        // Averaged over several openings. Where the ball spends its time swings a good deal
        // match to match, and one match of it is a small enough sample to come out backwards.
        const OPENINGS: u32 = 4;
        let average = |tactic: Tactic| {
            let mut advance = 0.0;
            let mut third = 0.0;
            for variant in 0..OPENINGS {
                let report =
                    super::harness::play(tactic, Tactic::Balanced, MATCH_SECONDS, variant);
                advance += report.orange_mean_advance;
                third += report.ball_in_orange_third_fraction;
            }
            (advance / OPENINGS as f32, third / OPENINGS as f32)
        };

        let (pressing_advance, pressing_third) = average(Tactic::HighPress);
        let (sitting_advance, sitting_third) = average(Tactic::LowBlock);

        assert!(
            pressing_advance > sitting_advance + 5.0,
            "high press held x={pressing_advance:+.1} but low block held x={sitting_advance:+.1}; \
             the tactic barely shows",
        );
        assert!(
            pressing_third > sitting_third,
            "high press put the ball in the end it attacks {:.0}% of the time, low block {:.0}%",
            pressing_third * 100.0,
            sitting_third * 100.0,
        );
    }

    #[test]
    #[ignore = "tuning aid: prints how a match plays, asserts nothing"]
    fn print_match_reports() {
        for (orange, blue) in MATCHUPS {
            let report = super::harness::play(orange, blue, MATCH_SECONDS, 0);
            println!(
                "{:>10} v {:<10} goals {}-{} ({:.0}s/goal)  stall {:.1}s  drought {:.0}s  \
                 ball {:.1}m/s  advance {:+.1}/{:+.1}  |z| {:.1} off {:.0}%  third {:.0}%",
                orange.name(),
                blue.name(),
                report.goals_orange,
                report.goals_blue,
                report.seconds_per_goal(),
                report.longest_ball_stall,
                report.longest_goal_drought,
                report.ball_metres_per_second,
                report.orange_mean_advance,
                report.blue_mean_advance,
                report.ball_mean_abs_z,
                report.ball_off_pitch_fraction * 100.0,
                report.ball_in_orange_third_fraction * 100.0,
            );
        }
    }

    #[test]
    fn an_aggressive_tactic_plays_higher_and_hunts_in_numbers() {
        let press = style(Tactic::HighPress);
        let block = style(Tactic::LowBlock);
        assert!(press.line_x > block.line_x, "high press should hold a higher line");
        assert!(press.hunters > block.hunters, "high press should send more players at the ball");
        assert!(
            press.press_trigger > block.press_trigger,
            "high press should leave its shape from further out"
        );
        assert!(press.shoot_range > block.shoot_range, "high press should shoot from further");
        assert!(press.pass_risk > block.pass_risk, "high press should try tighter passes");
    }

    #[test]
    fn every_tactic_sends_someone_to_contest_the_ball() {
        for tactic in Tactic::ALL {
            assert!(
                style(tactic).hunters >= 1,
                "{} should still contest the ball",
                tactic.name()
            );
        }
    }

    #[test]
    fn wing_play_spreads_wider_than_a_low_block() {
        assert!(style(Tactic::WingPlay).width > style(Tactic::LowBlock).width);
    }

    #[test]
    fn a_damped_ball_stops_short_of_where_a_straight_line_would_put_it() {
        let pos = Vec3::new(0.0, 1.6, 0.0);
        let vel = Vec3::new(30.0, 0.0, 0.0);
        let predicted = predict_ball(pos, vel, 1.0);
        assert!(predicted.x < pos.x + vel.x, "damping should shorten the prediction");
        assert!(predicted.x > pos.x, "but the ball still travels forward");
    }

    /// The chaser is whoever gets there first, not whoever is nearest now.
    ///
    /// Picking by raw distance hands the role to a player the ball is rolling away from, while
    /// the teammate it is rolling toward is told to hold shape. The ball then runs past both.
    #[test]
    fn the_player_the_ball_is_running_to_takes_it_over_the_nearer_one() {
        let ball = Vec3::new(0.0, 1.6, 0.0);
        let rolling_away = Vec3::new(20.0, 0.0, 0.0);
        let near_behind = Vec3::new(-3.0, 1.75, 0.0);
        let far_ahead = Vec3::new(12.0, 1.75, 0.0);
        assert!(
            near_behind.distance(ball) < far_ahead.distance(ball),
            "the behind player really is nearer"
        );
        assert!(
            intercept_time(far_ahead, ball, rolling_away)
                < intercept_time(near_behind, ball, rolling_away),
            "but the one ahead of the roll reaches it first"
        );
    }

    #[test]
    fn a_blocked_lane_is_not_clear_and_an_empty_one_is() {
        let from = Vec3::new(0.0, 1.6, 0.0);
        let to = Vec3::new(10.0, 1.6, 0.0);
        assert_eq!(lane_clearance(from, to, &[], LANE_RADIUS), 1.0);
        let blocked = lane_clearance(from, to, &[Vec3::new(5.0, 1.75, 0.0)], LANE_RADIUS);
        assert!(blocked < 0.05, "a body on the line blocks it, got {blocked}");
    }

    #[test]
    fn a_body_behind_the_passer_blocks_nothing() {
        let from = Vec3::new(0.0, 1.6, 0.0);
        let to = Vec3::new(10.0, 1.6, 0.0);
        let behind = lane_clearance(from, to, &[Vec3::new(-4.0, 1.75, 0.0)], LANE_RADIUS);
        assert_eq!(behind, 1.0, "an opponent behind the ball is not in the lane");
    }

    /// A shot has to be aimed somewhere that scores.
    ///
    /// `detect_goals` only counts a ball inside `GOAL_DEPTH/2 - 0.2` in z and under the bar.
    /// Aiming at the goal's centre line, or at a post, is aiming at a miss.
    #[test]
    fn a_shot_is_aimed_inside_the_scoring_window() {
        let scorable = GOAL_DEPTH / 2.0 - 0.2;
        for z in [-6.0, 0.0, 6.0] {
            let from = Vec3::new(14.0, 1.6, z);
            let (target, _) = shot_target(Team::Orange, from, &[]);
            assert!(
                target.z.abs() < scorable,
                "aimed at z={} which is outside the {scorable} scoring window",
                target.z
            );
            assert!(
                (target.x - FIELD_WIDTH / 2.0).abs() < 1e-3,
                "orange should shoot at the +x goal"
            );
        }
    }

    /// Shots must not all be aimed at the same post.
    ///
    /// Most of the goal is usually unobstructed, so ranking targets on lane clearance alone
    /// leaves several tied and hands every tie to whichever the loop saw first. A whole match
    /// of shots then arrives at one corner. The tie-break is distance from the nearest body,
    /// so the aim moves to the part of the goal that is least well covered.
    #[test]
    fn a_shot_avoids_the_covered_part_of_the_goal() {
        let from = Vec3::new(16.0, 1.6, 0.0);
        let covering_low_side = Vec3::new(20.0, 1.75, -2.5);
        let (low, _) = shot_target(Team::Orange, from, &[covering_low_side]);
        assert!(low.z > 0.0, "cover at -z should push the aim to +z, got z={}", low.z);

        let covering_high_side = Vec3::new(20.0, 1.75, 2.5);
        let (high, _) = shot_target(Team::Orange, from, &[covering_high_side]);
        assert!(high.z < 0.0, "cover at +z should push the aim to -z, got z={}", high.z);
    }

    #[test]
    fn an_open_forward_pass_beats_a_blocked_closer_one() {
        let ball = Vec3::new(-6.0, 1.6, 0.0);
        let opponents = vec![Vec3::new(-2.0, 1.75, 4.0)];
        let open_forward = Vec3::new(6.0, 1.75, -3.0);
        let blocked_near = Vec3::new(2.0, 1.75, 4.0);
        let open = pass_score(Team::Orange, ball, open_forward, &opponents, 0.5);
        let blocked = pass_score(Team::Orange, ball, blocked_near, &opponents, 0.5);
        assert!(open > blocked, "open {open} should beat blocked {blocked}");
    }

    #[test]
    fn a_direct_tactic_values_ground_gained_more_than_a_cautious_one() {
        let ball = Vec3::new(-6.0, 1.6, 0.0);
        let forward = Vec3::new(8.0, 1.75, 1.0);
        let backward = Vec3::new(-16.0, 1.75, 1.0);
        let direct_gap = pass_score(Team::Orange, ball, forward, &[], 1.0)
            - pass_score(Team::Orange, ball, backward, &[], 1.0);
        let cautious_gap = pass_score(Team::Orange, ball, forward, &[], 0.1)
            - pass_score(Team::Orange, ball, backward, &[], 0.1);
        assert!(
            direct_gap > cautious_gap,
            "directness should widen the preference for a forward ball"
        );
    }

    #[test]
    fn a_low_block_covers_deeper_and_a_high_press_pushes_further_up() {
        let ball = Vec3::new(0.0, 1.6, 0.0);
        let low = shape_home(Team::Orange, 0, 4, false, ball, &style(Tactic::LowBlock));
        let balanced = shape_home(Team::Orange, 0, 4, false, ball, &style(Tactic::Balanced));
        assert!(low.x < balanced.x, "low block covers deeper: {} vs {}", low.x, balanced.x);

        let press = shape_home(Team::Orange, 3, 4, true, ball, &style(Tactic::HighPress));
        let balanced_att = shape_home(Team::Orange, 3, 4, true, ball, &style(Tactic::Balanced));
        assert!(press.x > balanced_att.x, "high press attacks higher");
    }

    #[test]
    fn support_lanes_do_not_stack_on_one_spot() {
        let ball = Vec3::new(0.0, 1.6, 0.0);
        let s = style(Tactic::Balanced);
        let lanes: Vec<f32> =
            (0..4).map(|i| shape_home(Team::Orange, i, 4, i % 2 == 1, ball, &s).z).collect();
        for i in 0..lanes.len() {
            for j in (i + 1)..lanes.len() {
                assert!(
                    (lanes[i] - lanes[j]).abs() > 1.0,
                    "lanes {i} and {j} overlap at {} and {}",
                    lanes[i],
                    lanes[j]
                );
            }
        }
    }

    #[test]
    fn everything_stays_on_the_pitch() {
        let ball = Vec3::new(20.0, 1.6, 14.0);
        for tactic in Tactic::ALL {
            let s = PlayStyle::from_params(&tactic.params(), Team::Orange);
            for slot in 0..4 {
                let home = shape_home(Team::Orange, slot, 4, slot % 2 == 1, ball, &s);
                assert!(home.x.abs() <= FIELD_WIDTH / 2.0, "{} ran off in x", tactic.name());
                assert!(home.z.abs() <= FIELD_DEPTH / 2.0, "{} ran off in z", tactic.name());
            }
        }
    }

    #[test]
    fn commitment_decides_how_many_go_forward() {
        let none: Vec<bool> = (0..4).map(|s| is_attacker(s, 4, 0.0)).collect();
        let all: Vec<bool> = (0..4).map(|s| is_attacker(s, 4, 1.0)).collect();
        assert!(none.iter().all(|a| !a), "no commitment, nobody forward");
        assert!(all.iter().all(|a| *a), "full commitment, everybody forward");
        assert_eq!((0..4).filter(|&s| is_attacker(s, 4, 0.5)).count(), 2);
    }

    /// A wedged player has to free itself, and then stop trying to.
    ///
    /// There was no recovery at all before this: a cube pinned against a wall by a teammate
    /// kept pushing into the wall for the rest of the round, because the only thing that ever
    /// changed its movement was where the ball was.
    #[test]
    fn a_player_that_cannot_move_sidesteps_and_then_settles() {
        let mut memory = PlayMemory::default();
        let key = (Team::Orange, 0);
        let wanted = Vec2::new(1.0, 0.0);

        let (early, _) = unstick(key, wanted, 0.0, &mut memory, 0.3);
        assert_eq!(early, wanted, "a moment of no progress is not being stuck");

        let mut freed = None;
        for _ in 0..4 {
            let (movement, jump) = unstick(key, wanted, 0.0, &mut memory, 0.3);
            if movement != wanted {
                freed = Some((movement, jump));
                break;
            }
        }
        let (movement, jump) = freed.expect("should break out after STUCK_SECS of no progress");
        assert!(movement.y.abs() > 0.5, "the escape should be sideways, got {movement:?}");
        assert!(jump, "and should jump");

        // Once it is moving again the sidestep has to end, or it never goes where it was sent.
        for _ in 0..6 {
            unstick(key, wanted, 10.0, &mut memory, 0.3);
        }
        let (settled, _) = unstick(key, wanted, 10.0, &mut memory, 0.3);
        assert_eq!(settled, wanted, "a moving player should follow its target again");
    }

    #[test]
    fn stuck_teammates_break_apart_rather_than_shuffling_together() {
        let mut memory = PlayMemory::default();
        let wanted = Vec2::new(1.0, 0.0);
        let mut escapes = Vec::new();
        for index in [0usize, 1] {
            let key = (Team::Orange, index);
            let mut found = None;
            for _ in 0..6 {
                let (movement, _) = unstick(key, wanted, 0.0, &mut memory, 0.3);
                if movement != wanted {
                    found = Some(movement);
                    break;
                }
            }
            escapes.push(found.expect("both should break out"));
        }
        assert!(
            escapes[0].y * escapes[1].y < 0.0,
            "neighbours should sidestep opposite ways, got {escapes:?}"
        );
    }

    /// A ball that stops has to be put back into play.
    ///
    /// Wedged into a corner it is reachable by nobody and moved by nothing, and the round
    /// simply ran out. First the nearest player is sent to hit it; only if that fails is it
    /// moved directly.
    #[test]
    fn a_stopped_ball_is_first_kicked_and_then_rescued() {
        let mut memory = PlayMemory::default();
        let centre = Vec3::new(0.0, 1.6, 0.0);
        assert_eq!(note_ball_stall(&mut memory, centre, Vec3::ZERO, 0.5), Stall::None);
        let mut saw_kick = false;
        let mut saw_rescue = false;
        for _ in 0..10 {
            match note_ball_stall(&mut memory, centre, Vec3::ZERO, 0.5) {
                Stall::Kick => saw_kick = true,
                Stall::Rescue => {
                    saw_rescue = true;
                    break;
                }
                Stall::None => {}
            }
        }
        assert!(saw_kick, "should ask for a kick first");
        assert!(saw_rescue, "and rescue the ball if that did not work");
    }

    #[test]
    fn a_moving_ball_in_play_is_never_treated_as_stalled() {
        let mut memory = PlayMemory::default();
        let centre = Vec3::new(0.0, 1.6, 0.0);
        for _ in 0..20 {
            assert_eq!(
                note_ball_stall(&mut memory, centre, Vec3::new(9.0, 0.0, 0.0), 0.5),
                Stall::None
            );
        }
    }

    /// A ball can be out of the game without having stopped.
    ///
    /// The arena runs seven metres past each touchline and nothing keeps the ball inside it.
    /// Out there nobody supports the carrier, so it shuffles the ball along the wall at a
    /// perfectly healthy speed toward a goal it has no angle on -- which is why watching the
    /// ball's speed alone never noticed, and why some matches spent half their time like that.
    #[test]
    fn a_ball_stranded_past_the_touchline_is_brought_back_even_while_moving() {
        let mut memory = PlayMemory::default();
        let outside = Vec3::new(0.0, 1.6, FIELD_DEPTH / 2.0 + 4.0);
        let rolling = Vec3::new(8.0, 0.0, 0.0);
        let mut rescued = false;
        for _ in 0..20 {
            if note_ball_stall(&mut memory, outside, rolling, 0.25) == Stall::Rescue {
                rescued = true;
                break;
            }
        }
        assert!(rescued, "a ball left out in the run-off should be put back into play");
    }

    #[test]
    fn a_carrier_stuck_out_wide_plays_the_ball_back_infield() {
        let me = PlayerView {
            team: Team::Orange,
            index: 0,
            pos: Vec3::new(0.0, 1.75, 14.0),
            vel: Vec3::ZERO,
        };
        let snapshot = Snapshot {
            ball_pos: Vec3::new(0.0, 1.6, 15.0),
            ball_vel: Vec3::ZERO,
            players: vec![me],
        };
        let decision =
            carrier_decision(Team::Orange, &me, &snapshot, &style(Tactic::Balanced), true);
        let kick = decision.kick.expect("should play the ball");
        assert!(kick.dir.z < 0.0, "a ball out at the touchline should come back in, got {kick:?}");
    }

    #[test]
    fn the_carrier_role_does_not_flicker_between_equals() {
        let ball = Vec3::new(0.0, 1.6, 0.0);
        let players = vec![
            PlayerView { team: Team::Orange, index: 0, pos: Vec3::new(-2.0, 1.75, 0.0), vel: Vec3::ZERO },
            PlayerView { team: Team::Orange, index: 1, pos: Vec3::new(-2.1, 1.75, 0.0), vel: Vec3::ZERO },
        ];
        let mut memory = TeamMemory::default();
        let first = pick_carrier(&players, ball, Vec3::ZERO, &mut memory, 0.016);
        for _ in 0..60 {
            let again = pick_carrier(&players, ball, Vec3::ZERO, &mut memory, 0.016);
            assert_eq!(again, first, "the role should not change hands between equals");
        }
    }

    #[test]
    fn the_carrier_role_moves_when_a_teammate_is_clearly_quicker() {
        let ball = Vec3::new(0.0, 1.6, 0.0);
        let mut memory = TeamMemory::default();
        let near = vec![
            PlayerView { team: Team::Orange, index: 0, pos: Vec3::new(-1.0, 1.75, 0.0), vel: Vec3::ZERO },
            PlayerView { team: Team::Orange, index: 1, pos: Vec3::new(-14.0, 1.75, 0.0), vel: Vec3::ZERO },
        ];
        assert_eq!(pick_carrier(&near, ball, Vec3::ZERO, &mut memory, 0.016), Some(0));

        // Index 0 is now stranded and index 1 is on top of the ball.
        let moved = vec![
            PlayerView { team: Team::Orange, index: 0, pos: Vec3::new(-20.0, 1.75, 0.0), vel: Vec3::ZERO },
            PlayerView { team: Team::Orange, index: 1, pos: Vec3::new(-1.0, 1.75, 0.0), vel: Vec3::ZERO },
        ];
        let mut handed_over = None;
        for _ in 0..60 {
            handed_over = pick_carrier(&moved, ball, Vec3::ZERO, &mut memory, 0.016);
        }
        assert_eq!(handed_over, Some(1), "the role should hand over once it is clear");
    }

    #[test]
    fn a_carrier_in_front_of_an_open_goal_shoots_at_it() {
        let me = PlayerView {
            team: Team::Orange,
            index: 0,
            pos: Vec3::new(17.0, 1.75, 0.0),
            vel: Vec3::ZERO,
        };
        let snapshot = Snapshot {
            ball_pos: Vec3::new(18.0, 1.6, 0.0),
            ball_vel: Vec3::ZERO,
            players: vec![me],
        };
        let decision =
            carrier_decision(Team::Orange, &me, &snapshot, &style(Tactic::Balanced), true);
        let kick = decision.kick.expect("should strike the ball");
        assert!(kick.dir.x > 0.0, "should shoot toward the +x goal");
        assert!((kick.speed - SHOT_SPEED).abs() < 1e-3, "and shoot, not pass");
    }

    #[test]
    fn a_carrier_out_of_range_of_goal_looks_for_a_teammate() {
        let me = PlayerView {
            team: Team::Orange,
            index: 0,
            pos: Vec3::new(-14.0, 1.75, 0.0),
            vel: Vec3::ZERO,
        };
        let mate = PlayerView {
            team: Team::Orange,
            index: 1,
            pos: Vec3::new(0.0, 1.75, 4.0),
            vel: Vec3::ZERO,
        };
        let snapshot = Snapshot {
            ball_pos: Vec3::new(-13.0, 1.6, 0.0),
            ball_vel: Vec3::ZERO,
            players: vec![me, mate],
        };
        let decision =
            carrier_decision(Team::Orange, &me, &snapshot, &style(Tactic::Balanced), true);
        let kick = decision.kick.expect("should play the ball");
        assert!(
            (kick.speed - PASS_SPEED).abs() < 1e-3,
            "an open teammate upfield should be passed to, got speed {}",
            kick.speed
        );
        assert!(kick.dir.x > 0.0, "and the pass should go forward");
    }

    #[test]
    fn a_carrier_with_nobody_on_and_no_shot_keeps_the_ball_moving() {
        let me = PlayerView {
            team: Team::Orange,
            index: 0,
            pos: Vec3::new(-1.0, 1.75, 0.0),
            vel: Vec3::ZERO,
        };
        let snapshot = Snapshot {
            ball_pos: Vec3::new(0.0, 1.6, 0.0),
            ball_vel: Vec3::ZERO,
            players: vec![me],
        };
        let decision =
            carrier_decision(Team::Orange, &me, &snapshot, &style(Tactic::Balanced), true);
        assert!(
            decision.movement.length() > 0.1 || decision.kick.is_some(),
            "a lone carrier should still be doing something"
        );
    }

    #[test]
    fn a_pinned_defender_clears_upfield() {
        let me = PlayerView {
            team: Team::Orange,
            index: 0,
            pos: Vec3::new(-19.0, 1.75, 0.0),
            vel: Vec3::ZERO,
        };
        // Two opponents right on top of the ball, and a marked teammate, so nothing is on.
        let snapshot = Snapshot {
            ball_pos: Vec3::new(-20.0, 1.6, 0.0),
            ball_vel: Vec3::ZERO,
            players: vec![
                me,
                PlayerView { team: Team::Orange, index: 1, pos: Vec3::new(-18.0, 1.75, 1.0), vel: Vec3::ZERO },
                PlayerView { team: Team::Blue, index: 0, pos: Vec3::new(-18.5, 1.75, 0.5), vel: Vec3::ZERO },
                PlayerView { team: Team::Blue, index: 1, pos: Vec3::new(-18.2, 1.75, 1.2), vel: Vec3::ZERO },
            ],
        };
        let decision =
            carrier_decision(Team::Orange, &me, &snapshot, &style(Tactic::LowBlock), true);
        let kick = decision.kick.expect("should get rid of it");
        assert!(kick.dir.x > 0.0, "a clearance goes up the pitch, not across its own goal");
    }

    #[test]
    fn nobody_kicks_when_kicking_is_switched_off() {
        let me = PlayerView {
            team: Team::Orange,
            index: 0,
            pos: Vec3::new(17.0, 1.75, 0.0),
            vel: Vec3::ZERO,
        };
        let snapshot = Snapshot {
            ball_pos: Vec3::new(18.0, 1.6, 0.0),
            ball_vel: Vec3::ZERO,
            players: vec![me],
        };
        let decision =
            carrier_decision(Team::Orange, &me, &snapshot, &style(Tactic::Balanced), false);
        assert!(decision.kick.is_none(), "kicking disabled means no kick request");
    }

    #[test]
    fn steering_eases_in_rather_than_running_flat_out_to_the_last_inch() {
        let pos = Vec3::ZERO;
        let far = steer(pos, Vec3::new(30.0, 0.0, 0.0), &[], 1.0);
        let close = steer(pos, Vec3::new(1.0, 0.0, 0.0), &[], 1.0);
        let arrived = steer(pos, Vec3::new(0.2, 0.0, 0.0), &[], 1.0);
        assert!((far.length() - 1.0).abs() < 1e-3, "flat out when far away");
        assert!(close.length() < far.length(), "easing in when close");
        assert!(arrived.length() < 1e-6, "stopped once there");
    }

    #[test]
    fn a_whole_team_gets_exactly_one_decision_each() {
        let mut players: Vec<PlayerView> = (0..5)
            .map(|i| PlayerView {
                team: Team::Orange,
                index: i,
                pos: Vec3::new(-10.0 + i as f32 * 2.0, 1.75, i as f32 * 2.0 - 4.0),
                vel: Vec3::ZERO,
            })
            .collect();
        players.extend((0..5).map(|i| PlayerView {
            team: Team::Blue,
            index: i,
            pos: Vec3::new(10.0 - i as f32 * 2.0, 1.75, i as f32 * 2.0 - 4.0),
            vel: Vec3::ZERO,
        }));
        let snapshot = Snapshot { ball_pos: Vec3::new(0.0, 1.6, 0.0), ball_vel: Vec3::ZERO, players };
        let mut memory = PlayMemory::default();
        let s = style(Tactic::Balanced);
        let decisions =
            decide_team(Team::Orange, &snapshot, &|_| s, &s, &mut memory, 0.016, true);
        assert_eq!(decisions.len(), 5, "every player should be told what to do");
        for index in 0..5 {
            assert!(decisions.contains_key(&index), "player {index} had no decision");
        }
    }

    /// Off-ball players must not pile onto the ball.
    ///
    /// A swarm is what the pitch looked like whenever pressing was turned up: everybody chased,
    /// nobody was open, and the carrier had no pass. Only `hunters` may leave the shape.
    #[test]
    fn only_the_tactic_s_hunters_leave_the_shape() {
        let mut players: Vec<PlayerView> = (0..5)
            .map(|i| PlayerView {
                team: Team::Orange,
                index: i,
                pos: Vec3::new(-6.0, 1.75, i as f32 * 2.0 - 4.0),
                vel: Vec3::ZERO,
            })
            .collect();
        players.push(PlayerView {
            team: Team::Blue,
            index: 0,
            pos: Vec3::new(14.0, 1.75, 0.0),
            vel: Vec3::ZERO,
        });
        let snapshot = Snapshot { ball_pos: Vec3::new(0.0, 1.6, 0.0), ball_vel: Vec3::ZERO, players };

        for tactic in [Tactic::Balanced, Tactic::HighPress, Tactic::LowBlock] {
            let s = style(tactic);
            let mut memory = PlayMemory::default();
            let decisions =
                decide_team(Team::Orange, &snapshot, &|_| s, &s, &mut memory, 0.016, true);
            let hunting =
                decisions.values().filter(|d| d.role == Role::Hunter).count();
            assert!(
                hunting <= s.hunters,
                "{} sent {hunting} players at the ball but allows {}",
                tactic.name(),
                s.hunters
            );
            assert_eq!(
                decisions.values().filter(|d| d.role == Role::Carrier).count(),
                1,
                "{} should have exactly one player on the ball",
                tactic.name()
            );
        }

        // And the aggressive tactic really does commit more of them than the cautious one.
        let mut press_memory = PlayMemory::default();
        let press = style(Tactic::HighPress);
        let pressing = decide_team(
            Team::Orange, &snapshot, &|_| press, &press, &mut press_memory, 0.016, true,
        );
        let mut block_memory = PlayMemory::default();
        let block = style(Tactic::LowBlock);
        let sitting = decide_team(
            Team::Orange, &snapshot, &|_| block, &block, &mut block_memory, 0.016, true,
        );
        let count = |d: &HashMap<usize, Decision>| {
            d.values().filter(|d| d.role == Role::Hunter).count()
        };
        assert!(
            count(&pressing) > count(&sitting),
            "high press should hunt with more players than a low block"
        );
    }
}
