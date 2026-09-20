use bevy::prelude::*;
use bevy_rapier3d::prelude::Group;

// === COLLISION GROUPS ===
// Ball: collides with everything except barriers
// Players: collide with everything including barriers
// Barriers: only collide with players
pub const BALL_GROUP: Group = Group::GROUP_1;
pub const PLAYER_GROUP: Group = Group::GROUP_2;
pub const BARRIER_GROUP: Group = Group::GROUP_3;

// === ARENA (White environment) ===
pub const ARENA_WIDTH: f32 = 58.0;      // X - total width
pub const ARENA_DEPTH: f32 = 46.0;      // Z - depth (must be > extended field)
pub const ARENA_HEIGHT: f32 = 15.0;     // Y - wall height

pub const WALL_THICKNESS: f32 = 0.5;
pub const WALL_COLOR: Color = Color::rgb(0.95, 0.95, 0.95);  // Off-white
pub const GRID_COLOR: Color = Color::rgb(0.85, 0.85, 0.85);  // Grey lines

// === FIELD (Grey play area) ===
pub const FIELD_WIDTH: f32 = 48.0;      // X  (full/real size = max; curriculum scales down at runtime)
pub const FIELD_DEPTH: f32 = 32.0;      // Z
pub const FIELD_HEIGHT: f32 = 1.0;      // Y - platform thickness
pub const FIELD_COLOR: Color = Color::rgb(0.25, 0.25, 0.25);  // Dark grey
pub const FIELD_GRID_SPACING: f32 = 2.0;  // White grid spacing
pub const SIDE_EXTENSION: f32 = 5.0;    // Side zone extension (same as goal side area)
pub const FLUORESCENT_COLOR: Color = Color::rgb(0.0, 1.0, 0.5);  // Fluorescent green

// === GOALS ===
pub const GOAL_WIDTH: f32 = 0.5;        // Back thickness (legacy, kept for compatibility)
pub const GOAL_HEIGHT: f32 = 5.0;       // Height
pub const GOAL_DEPTH: f32 = 8.0;        // Opening width (Z direction)
pub const GOAL_NET_DEPTH: f32 = 3.0;    // How far the net extends behind the goal line
pub const GOAL_ORANGE: Color = Color::rgb(1.0, 0.6, 0.2);
pub const GOAL_BLUE: Color = Color::rgb(0.2, 0.6, 1.0);
pub const NET_COLOR: Color = Color::rgba(0.9, 0.9, 0.9, 0.4);  // White semi-transparent

// === CUBE PLAYERS ===
pub const CUBE_SIZE: f32 = 1.5;         // Cube size
pub const CUBE_MASS: f32 = 10.0;
pub const CUBE_FRICTION: f32 = 0.5;
pub const CUBE_RESTITUTION: f32 = 0.3;  // Bounce

pub const CUBE_MAX_SPEED: f32 = 15.0;   // Max speed
/// Absolute horizontal-speed safety ceiling = MAX_SPEED_SAFETY * CUBE_MAX_SPEED.
/// Never binds in normal play (control target is <= 1x); only bounds knockback/
/// force spikes so physics can't blow up.
pub const MAX_SPEED_SAFETY: f32 = 5.0;
pub const CUBE_ACCELERATION: f32 = 50.0;
pub const CUBE_JUMP_FORCE: f32 = 60.0;   // Small jump, can't jump over walls

pub const CUBE_ORANGE_COLOR: Color = Color::rgb(1.0, 0.6, 0.2);
pub const CUBE_BLUE_COLOR: Color = Color::rgb(0.2, 0.6, 1.0);

// === BALL ===
pub const BALL_RADIUS: f32 = 0.6;
pub const BALL_MASS: f32 = 1.0;
pub const BALL_FRICTION: f32 = 0.4;
pub const BALL_RESTITUTION: f32 = 0.85;  // Very bouncy
pub const BALL_LINEAR_DAMPING: f32 = 0.3;
pub const BALL_ANGULAR_DAMPING: f32 = 0.5;

// === PHYSICS ===
pub const GRAVITY: f32 = -20.0;         // Y gravity (snappy jump)
pub const PHYSICS_TIMESTEP: f32 = 1.0 / 30.0;  // 30Hz sim (training throughput); only the headless RL sim uses this

// === MATCH ===
pub const MATCH_DURATION_SECS: f32 = 300.0;  // 5 minutes total
/// Length of a round: positions reset to kickoff when it expires.
///
/// Fifteen seconds was too short to play soccer in. A move that starts with a cleared ball,
/// works it wide and comes back for a shot takes longer than that, so the reset kept landing
/// mid-attack and the match read as a series of scrambles rather than as play. Forty-five is
/// long enough for an attack to finish and short enough to stay a safety net against a stall.
pub const ROUND_DURATION_SECS: f32 = 45.0;
pub const GOALS_TO_WIN: u32 = 10;
pub const RESET_DELAY_SECS: f32 = 1.0;  // 1 second pause after goal

// === RL PARAMETERS ===
pub const MAX_EPISODE_STEPS: u32 = 1000;

/// Number of players on each team (both teams equal). Compile-time constant.
pub const PLAYERS_PER_TEAM: usize = 5;
/// Total number of agents across both teams.
pub const NUM_AGENTS: usize = 2 * PLAYERS_PER_TEAM;

/// Smallest effective field scale (1v1 plays on ~this fraction of the full field).
pub const FIELD_SCALE_MIN: f32 = 0.22;

/// Effective field scale for a given active roster (1..=PLAYERS_PER_TEAM): grows
/// linearly from `FIELD_SCALE_MIN` (1v1) to 1.0 (full roster). Couples pitch size to
/// player count — the curriculum shrinks the whole field for small rosters so short
/// finishes are learnable, then grows it to the real size as players are added.
pub fn field_scale(active: usize) -> f32 {
    if PLAYERS_PER_TEAM <= 1 {
        return 1.0;
    }
    let a = active.clamp(1, PLAYERS_PER_TEAM);
    let t = (a - 1) as f32 / (PLAYERS_PER_TEAM - 1) as f32;
    FIELD_SCALE_MIN + (1.0 - FIELD_SCALE_MIN) * t
}

/// Effective distance from field center to a goal line, given the active roster.
pub fn effective_goal_dist(active: usize) -> f32 {
    field_scale(active) * FIELD_WIDTH / 2.0
}

/// Number of tactic parameters (the `TacticParams` fields) appended to each
/// agent's observation so the RL policy can condition its behavior on the active
/// tactic. See `src/systems/heuristic_ai.rs::TacticParams`.
pub const TACTIC_PARAMS: usize = 7;

/// Per-agent observation length.
/// Layout: self pos+vel (6) + teammates (6*(N-1)) + opponents (6*N)
///         + ball pos+vel (6) + goal dists (2) + score_diff + time (2)
///         + superpower cooldown-ready fraction (1)
///         + own superpower one-hot [blast, freeze, boost, slow] (4)
///         + possession flags (3: self/teammate/opponent has ball)
///         + active tactic params (7, normalized) — the coach's directive for this agent
///       = 18 + 12*N + 7.
pub const OBSERVATION_SIZE: usize = 18 + 12 * PLAYERS_PER_TEAM + TACTIC_PARAMS;
/// Per-agent action length (move_x, move_z, jump, fire).
pub const ACTION_SIZE: usize = 4;

/// Physics ticks advanced per env.step() (action repeat / frame-skip).
/// 2 ticks at 30Hz = ~15 decisions/sec (same control rate as the old 4@60Hz)
/// while computing half as many physics ticks per step.
pub const ACTION_REPEAT: usize = 2;
/// Max seeded XZ jitter (meters) applied to player spawns on reset.
pub const RESET_POS_JITTER: f32 = 1.0;
/// Max seeded XZ offset (meters) applied to the ball spawn on reset.
pub const RESET_BALL_JITTER: f32 = 2.0;

// === SUPERPOWERS ===
pub const BLAST_RANGE: f32 = 8.0;
pub const BLAST_HALF_ANGLE_DEG: f32 = 30.0;
pub const BLAST_IMPULSE: f32 = 55.0;
pub const BLAST_COOLDOWN: f32 = 5.0;
pub const FREEZE_RANGE: f32 = 10.0;
pub const FREEZE_HALF_ANGLE_DEG: f32 = 15.0;
pub const FREEZE_SECS: f32 = 2.0;
pub const FREEZE_COOLDOWN: f32 = 8.0;
pub const BOOST_FACTOR: f32 = 1.5;
pub const BOOST_SECS: f32 = 2.0;
pub const BOOST_COOLDOWN: f32 = 10.0;
pub const SLOW_FACTOR: f32 = 0.4;
pub const SLOW_SECS: f32 = 3.0;
pub const SLOW_RANGE: f32 = 12.0;
pub const SLOW_COOLDOWN: f32 = 8.0;

/// What a cast that hits nobody costs, as a fraction of the power's full cooldown.
///
/// A miss used to be free: `activate_superpowers` only started the cooldown when the power
/// actually landed, so holding the fire button down cost nothing for the three powers that need a
/// target. That made aiming optional -- the cheapest strategy was to fire constantly and let the
/// cone find someone eventually -- and it is the reason a power had to be *decided* rather than
/// merely permitted. Firing into space now costs a real, shorter cooldown, so a bad cast is paid
/// for without being punished as hard as a wasted good one.
///
/// Boost is unaffected: it lands on the caster and so can never miss.
pub const WHIFF_COOLDOWN_FRACTION: f32 = 0.35;

// === POSSESSION / SHOOTING ===
/// Distance at which a player can gain or steal the ball (matches the touch-reward distance).
pub const TOUCH_RANGE: f32 = CUBE_SIZE / 2.0 + BALL_RADIUS + 0.5;
/// If the ball rolls farther than this from its holder, possession is released
/// (loose ball). Larger than `TOUCH_RANGE` so the possessed<->loose edge has a
/// hysteresis band and does not flicker.
pub const CONTROL_RADIUS: f32 = 2.5;
/// After losing possession, how long before the same player can re-grab.
pub const STEAL_COOLDOWN_SECS: f32 = 0.5;
/// How long an opponent must stay in range of a held ball before the steal
/// succeeds (a "tackle timer"). Leaving range resets their progress.
pub const STEAL_CONTACT_SECS: f32 = 0.4;

// --- Kicking ---
// The ball used to move only by being walked into, which is why there was no passing and no
// shooting: a cube could push the ball but never strike it. A kick sets the ball's velocity
// outright, the same way `apply_player_movement` sets a player's, so a pass arrives at a
// predictable speed and a shot is worth aiming.

/// How near the ball a player must be to strike it. A little beyond `TOUCH_RANGE` so a player
/// who is dribbling -- and therefore always a touch behind the ball -- can still shoot.
pub const KICK_RANGE: f32 = TOUCH_RANGE + 0.6;
/// Speed of a pass to a teammate. Fast enough to beat a covering opponent to the spot, slow
/// enough that the receiver can take it rather than watch it run away.
pub const PASS_SPEED: f32 = 21.0;
/// Speed of a shot on goal.
pub const SHOT_SPEED: f32 = 33.0;
/// Speed of a clearance out of the defensive third. Between the two: it has to travel, but a
/// clearance that leaves the pitch entirely just hands the ball back.
pub const CLEAR_SPEED: f32 = 27.0;
/// Fraction of a kick's speed added upward. Small on purpose -- it lifts the ball off the
/// surface so it is not shoved along under the cubes, but the apex stays far below
/// `GOAL_HEIGHT`, so lofting a shot never carries it over the bar.
pub const KICK_LOFT: f32 = 0.15;
/// How long after striking the ball before the same player may strike it again. Without this a
/// player in contact kicks on every frame and the ball simply vibrates.
pub const KICK_COOLDOWN_SECS: f32 = 0.45;
/// Speed the ball is restarted with after a goal or at a round boundary. Gentle: a kickoff is
/// a ball rolled into play, not a shot. See `systems::reset::kickoff_velocity`.
pub const KICKOFF_SPEED: f32 = 7.0;

// === REWARDS ===
pub const REWARD_GOAL: f32 = 30.0;
pub const REWARD_GOAL_AGAINST: f32 = -30.0;
pub const REWARD_BALL_PROGRESS: f32 = 0.5;  // reward per meter the ball nears the opp goal (potential-based)
/// Per-agent reward per meter an agent moves toward the ball (potential-based, so it
/// telescopes and can't be farmed). Bootstraps the behavior chain: without a reason
/// to approach the ball, a from-scratch policy collapses to passivity — it never
/// touches the ball, so the ball-progress signal never fires. Small so it guides
/// rather than dominates; anneals with the rest of the shaping.
pub const REWARD_BALL_APPROACH: f32 = 0.1;
/// Extra potential-based "finishing pull": a ramp that grows as the ball nears the
/// opp goal center (peaks at the mouth). Potential-based (telescopes), so camping in
/// the attacking third earns 0 — only approaching the net is rewarded.
pub const NEAR_GOAL_RADIUS: f32 = 6.0;
pub const NEAR_GOAL_BONUS: f32 = 4.0;
pub const REWARD_WIN: f32 = 5.0;
pub const REWARD_LOSE: f32 = -5.0;

// --- Per-tactic positional-imitation reward (Orange only; see docs/TACTICS.md §4c) ---
/// Default weight on the per-tactic `shape_match` reward: a per-step bump for each
/// Orange agent occupying the position its active tactic prescribes. This is THE
/// knob that trades scoring for visible style — leaned high so conditioned behaviors
/// are clearly distinct. Runtime-overridable via `CubeSoccerEnv::set_tactic_weight`.
/// Kept independent of `shaping_weight` so tactics don't fade as dense shaping anneals.
pub const TACTIC_WEIGHT: f32 = 0.1;
/// Gaussian width (meters) of the `shape_match` bump: reward peaks at `tactic_weight`
/// when the agent sits on its prescribed spot and falls off over ~this distance.
pub const TACTIC_MATCH_SIGMA: f32 = 5.0;

// --- Team-play shaping (produce positional roles instead of a ball-swarm) ---
/// Teammates closer than this (meters) count as "crowding" each other.
pub const CROWD_RADIUS: f32 = 3.0;
/// Per-step penalty per crowding teammate (individual) — pushes cubes to spread.
pub const REWARD_TEAMMATE_CROWD: f32 = -0.02;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum Team {
    #[default]
    Orange,
    Blue,
}

impl Team {
    pub fn opponent(&self) -> Team {
        match self {
            Team::Orange => Team::Blue,
            Team::Blue => Team::Orange,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Team::Orange => CUBE_ORANGE_COLOR,
            Team::Blue => CUBE_BLUE_COLOR,
        }
    }

    pub fn goal_color(&self) -> Color {
        match self {
            Team::Orange => GOAL_ORANGE,
            Team::Blue => GOAL_BLUE,
        }
    }
}

/// Flat agent ordering: Orange occupies indices `[0, PLAYERS_PER_TEAM)`,
/// Blue occupies `[PLAYERS_PER_TEAM, 2*PLAYERS_PER_TEAM)`.
pub fn agent_flat_index(team: Team, index: usize) -> usize {
    match team {
        Team::Orange => index,
        Team::Blue => PLAYERS_PER_TEAM + index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_size_includes_possession_flags_and_tactic() {
        assert_eq!(OBSERVATION_SIZE, 18 + 12 * PLAYERS_PER_TEAM + TACTIC_PARAMS);
        assert_eq!(TACTIC_PARAMS, 7);
    }

    #[test]
    fn agent_flat_index_orders_orange_then_blue() {
        assert_eq!(agent_flat_index(Team::Orange, 0), 0);
        assert_eq!(agent_flat_index(Team::Blue, 0), PLAYERS_PER_TEAM);
        if PLAYERS_PER_TEAM >= 2 {
            assert_eq!(agent_flat_index(Team::Orange, 1), 1);
            assert_eq!(agent_flat_index(Team::Blue, 1), PLAYERS_PER_TEAM + 1);
        }
    }
}
