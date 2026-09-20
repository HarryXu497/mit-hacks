use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use std::collections::HashMap;
use crate::entities::{Ball, CubePlayer, PlayerInput};
use crate::game::{Team, FIELD_WIDTH, PLAYERS_PER_TEAM, PLAYER_GROUP, BARRIER_GROUP};

/// Marker: cubes with this component are driven by the built-in heuristic AI.
#[derive(Component)]
pub struct AiControlled;

// Tuning for the role-based team AI.
// Engagement distances are quoted for the pitch they were tuned on and scaled to whatever
// pitch is actually in play -- see `pitch_scale`. Support *positions* derive from the goal
// lines (`own_goal_x`/`opp_goal_x` = +/-FIELD_WIDTH/2) and so already scale with the pitch;
// these thresholds did not, and the mismatch is what broke the AI when the field grew from
// 36 to 48 wide. Supports ended up standing a third further out than these radii could
// reach, so no teammate was ever close enough to take the ball role over, and a handler that
// overshot the ball was never relieved.
const SUPPORT_STOP_DIST_TUNED: f32 = 0.6; // support players stop when this close to their target
const REPULSION_RADIUS_TUNED: f32 = 2.5;  // teammates within this distance push each other apart
const REPULSION_STRENGTH: f32 = 0.8;      // how hard the spacing push is (dimensionless)
const HANDOFF_MARGIN_TUNED: f32 = 1.5;    // a teammate must be this much closer to steal the ball role
/// Distance at which the handler stops chasing the ball and starts driving it goalward.
const ENGAGE_RADIUS_TUNED: f32 = 2.5;
/// Distance within which the handler will jump for a ball above it.
const JUMP_RADIUS_TUNED: f32 = 3.0;

/// The pitch width every distance above was tuned on.
///
/// Chloe Nguyen's heuristic was authored against a 36-wide field. Harry Xu's RL branch widened
/// it to 48 so the curriculum has room to scale down from full size, which silently stretched
/// every *position* the AI computes while leaving every *threshold* where it was.
const TUNED_FIELD_WIDTH: f32 = 36.0;

/// How much larger the pitch in play is than the one these numbers were tuned on.
fn pitch_scale() -> f32 {
    FIELD_WIDTH / TUNED_FIELD_WIDTH
}

fn support_stop_dist() -> f32 {
    SUPPORT_STOP_DIST_TUNED * pitch_scale()
}
fn repulsion_radius() -> f32 {
    REPULSION_RADIUS_TUNED * pitch_scale()
}
fn handoff_margin() -> f32 {
    HANDOFF_MARGIN_TUNED * pitch_scale()
}
fn engage_radius() -> f32 {
    ENGAGE_RADIUS_TUNED * pitch_scale()
}
fn jump_radius() -> f32 {
    JUMP_RADIUS_TUNED * pitch_scale()
}
const SECONDARY_PRESS_FACTOR: f32 = 0.4; // non-nearest supports press this fraction as hard

/// Tunable positioning parameters for the heuristic team AI.
/// `Balanced` (see [`Tactic::params`]) reproduces the original hardcoded values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TacticParams {
    /// Fraction of (own_goal - ball) toward which defenders drop (0.5 = legacy).
    pub defender_depth: f32,
    /// Fraction of (opp_goal - ball) toward which attackers push (0.40 = legacy).
    pub attacker_push: f32,
    /// Lateral (z) spread multiplier for support positions (1.0 = legacy).
    pub width: f32,
    /// Teammate-repulsion strength multiplier (1.0 = legacy).
    pub spacing: f32,
    /// Off-ball players' collapse toward the ball (0.0 = hold shape / legacy).
    pub press: f32,
    /// Whole-block forward/back bias toward the opp goal (0.0 = legacy).
    pub line_height: f32,
    /// Fraction of supports playing as attackers (0.5 = legacy alternation).
    pub commitment: f32,
}

impl TacticParams {
    /// The 7 fields normalized to ~[-1, 1] for the observation vector. Each axis is
    /// affine-mapped around its neutral value so the preset range lands near the unit
    /// box (like the other obs features); blends interpolate within it. Order matches
    /// the struct: defender_depth, attacker_push, width, spacing, press, line_height,
    /// commitment.
    pub fn normalized(&self) -> [f32; 7] {
        [
            (self.defender_depth - 0.5) * 2.0,
            (self.attacker_push - 0.5) * 2.0,
            (self.width - 1.0) / 0.5,
            (self.spacing - 1.0) / 0.5,
            (self.press - 0.5) * 2.0,
            self.line_height / 0.5,
            (self.commitment - 0.5) * 2.0,
        ]
    }

    /// Weighted blend of several param sets ("70% Low Block, 30% Wide").
    /// Weights are normalized by their sum; empty input or zero total weight
    /// returns `Balanced`.
    pub fn blend(parts: &[(TacticParams, f32)]) -> TacticParams {
        let total: f32 = parts.iter().map(|(_, w)| *w).sum();
        if parts.is_empty() || total.abs() < 1e-6 {
            return Tactic::Balanced.params();
        }
        let mut acc = TacticParams {
            defender_depth: 0.0, attacker_push: 0.0, width: 0.0, spacing: 0.0,
            press: 0.0, line_height: 0.0, commitment: 0.0,
        };
        for (p, w) in parts {
            let f = w / total;
            acc.defender_depth += p.defender_depth * f;
            acc.attacker_push += p.attacker_push * f;
            acc.width += p.width * f;
            acc.spacing += p.spacing * f;
            acc.press += p.press * f;
            acc.line_height += p.line_height * f;
            acc.commitment += p.commitment * f;
        }
        acc
    }
}

/// The named tactic presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tactic {
    Balanced, HighPress, LowBlock, WingPlay,
}

impl Tactic {
    pub const ALL: [Tactic; 4] = [
        Tactic::Balanced, Tactic::HighPress, Tactic::LowBlock, Tactic::WingPlay,
    ];

    pub fn params(self) -> TacticParams {
        // fields: defender_depth, attacker_push, width, spacing, press, line_height, commitment
        match self {
            Tactic::Balanced       => TacticParams { defender_depth: 0.50, attacker_push: 0.40, width: 1.0, spacing: 1.0, press: 0.00, line_height:  0.00, commitment: 0.50 },
            Tactic::HighPress      => TacticParams { defender_depth: 0.30, attacker_push: 0.65, width: 1.0, spacing: 1.1, press: 0.70, line_height:  0.30, commitment: 0.60 },
            Tactic::LowBlock       => TacticParams { defender_depth: 0.85, attacker_push: 0.15, width: 0.8, spacing: 0.9, press: 0.00, line_height: -0.30, commitment: 0.25 },
            Tactic::WingPlay       => TacticParams { defender_depth: 0.50, attacker_push: 0.50, width: 1.6, spacing: 1.3, press: 0.10, line_height:  0.00, commitment: 0.55 },
        }
    }

    pub fn next(self) -> Tactic {
        let all = Tactic::ALL;
        let i = all.iter().position(|&t| t == self).unwrap();
        all[(i + 1) % all.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Tactic::Balanced => "Balanced",
            Tactic::HighPress => "High Press",
            Tactic::LowBlock => "Low Block",
            Tactic::WingPlay => "Wing Play",
        }
    }

    /// Resolve a preset by name, case-/space-/hyphen-insensitive; `None` if unknown.
    pub fn from_name(s: &str) -> Option<Tactic> {
        match s.trim().to_lowercase().replace([' ', '-'], "").as_str() {
            "balanced" => Some(Tactic::Balanced),
            "highpress" => Some(Tactic::HighPress),
            "lowblock" => Some(Tactic::LowBlock),
            "wingplay" => Some(Tactic::WingPlay),
            _ => None,
        }
    }
}

/// A team's tactical directive: a base applied to the whole team, plus optional
/// per-player overrides keyed by `CubePlayer.index`. Supports per-role / per-player
/// combos; each override may itself be a blend (see [`TacticParams::blend`]).
#[derive(Clone, Debug)]
pub struct TeamDirective {
    base: TacticParams,
    per_player: HashMap<usize, TacticParams>,
}

impl TeamDirective {
    pub fn uniform(p: TacticParams) -> Self {
        Self { base: p, per_player: HashMap::new() }
    }
    /// Params for a player index: its override if present, else the team base.
    pub fn params_for(&self, index: usize) -> TacticParams {
        self.per_player.get(&index).copied().unwrap_or(self.base)
    }
    pub fn set_base(&mut self, p: TacticParams) { self.base = p; }
    pub fn set_player(&mut self, index: usize, p: TacticParams) { self.per_player.insert(index, p); }
    pub fn clear_overrides(&mut self) { self.per_player.clear(); }
    /// The team-wide base params (ignoring per-player overrides).
    pub fn base_params(&self) -> TacticParams { self.base }
}

impl Default for TeamDirective {
    fn default() -> Self { Self::uniform(Tactic::Balanced.params()) }
}

/// Live per-team tactics. Registered in the visual app and the headless sim; a
/// missing resource defaults to Balanced (see `apply_heuristic_ai`).
#[derive(Resource, Clone, Debug)]
pub struct TeamTactics {
    pub orange: TeamDirective,
    pub blue: TeamDirective,
}

impl Default for TeamTactics {
    fn default() -> Self {
        Self { orange: TeamDirective::default(), blue: TeamDirective::default() }
    }
}

/// Difficulty knob for the heuristic-driven team(s): scales AI movement output and
/// gates superpower use. `1.0` = full-strength heuristic; `0.0` = frozen. Used by
/// training curricula to weaken the heuristic opponent early (so the RL team can
/// discover scoring) and then ramp back to full strength. A missing resource
/// defaults to `1.0`, so non-training consumers are unaffected.
#[derive(Resource, Clone, Copy, Debug)]
pub struct HeuristicDifficulty(pub f32);

impl Default for HeuristicDifficulty {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Player-count curriculum knob: how many players per team are "active" (1..=P).
/// Players with `index >= active` on *either* team are benched — made
/// non-collidable and frozen — so the match plays as a true NvN with a fixed
/// full-roster observation/action shape. Ramp this 1 -> P to grow 1v1 into full
/// 5v5 with the same policy. A missing resource defaults to the full roster.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ActiveRoster(pub usize);

impl Default for ActiveRoster {
    fn default() -> Self {
        Self(PLAYERS_PER_TEAM)
    }
}

/// Training knob: when `true`, the env samples a fresh Orange tactic on every
/// `reset()` (tactic domain-randomization, so the policy must *read* the tactic in
/// its observation to predict reward). Off by default so an explicitly-set coaching
/// tactic persists across resets (see `CubeSoccerEnv::set_tactic_randomization`).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct TacticRandomization(pub bool);

/// One teammate's identity + position, used to assign team roles.
pub struct TeamMate {
    pub index: usize,
    pub pos: Vec3,
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

/// Ball-handler heuristic: chase/predict the ball and push toward the opponent
/// goal when close. Used for the single teammate that currently holds the ball role.
pub fn heuristic_movement(team: Team, player_pos: Vec3, ball_pos: Vec3, ball_vel: Vec3) -> (Vec2, bool) {
    let target_goal_x = opp_goal_x(team);

    let to_ball = ball_pos - player_pos;
    let dist_to_ball = to_ball.length();
    let predicted = ball_pos + ball_vel * 0.5;
    let to_pred = predicted - player_pos;

    // Which way the opponent goal lies, and whether the ball is still between us and it.
    //
    // The goalward push below is a blind `signum` toward the goal. That is right while the ball
    // is ahead of us -- driving at the goal drives us through the ball -- but if we have
    // overshot, the very same push carries us *away* from the ball, and since the push is what
    // keeps us past it, the handler never comes back. Measured against the pre-merge build, the
    // handler spent 40% of the match orbiting 2.5 units off the ball in exactly this loop,
    // which is what "they rush at the ball and can't touch it" looked like. So: only push
    // goalward while the ball is ahead; once it is behind us, go back to chasing it.
    let forward = (target_goal_x - player_pos.x).signum();
    let ball_is_ahead = (ball_pos.x - player_pos.x) * forward >= 0.0;

    let movement = if dist_to_ball < engage_radius() && ball_is_ahead {
        Vec2::new(forward, (-ball_pos.z).clamp(-0.5, 0.5))
    } else {
        Vec2::new(to_pred.x.clamp(-1.0, 1.0), to_pred.z.clamp(-1.0, 1.0)).normalize_or_zero()
    };

    let jump = dist_to_ball < jump_radius() && ball_pos.y > player_pos.y + 0.5;
    (movement, jump)
}

/// Home position a support player should hold instead of chasing the ball.
/// `defender` covers the lane toward the team's own goal; otherwise the player
/// pushes up-field as an attacking outlet, offset in Z for width.
fn support_target(team: Team, defender: bool, ball_pos: Vec3, p: &TacticParams) -> Vec3 {
    let mut t = if defender {
        let gx = own_goal_x(team);
        Vec3::new(ball_pos.x + (gx - ball_pos.x) * p.defender_depth, ball_pos.y, ball_pos.z * 0.4 * p.width)
    } else {
        let gx = opp_goal_x(team);
        Vec3::new(ball_pos.x + (gx - ball_pos.x) * p.attacker_push, ball_pos.y, -ball_pos.z * 0.6 * p.width)
    };
    // Whole-block engagement line: shove the target toward the opp goal by
    // line_height * half-field, clamped to the pitch. Neutral 0.0 = no change.
    let forward = opp_goal_x(team).signum();
    t.x += p.line_height * (FIELD_WIDTH / 2.0) * forward;
    t.x = t.x.clamp(-FIELD_WIDTH / 2.0, FIELD_WIDTH / 2.0);
    t
}

/// Which support sorted-positions play as attackers, given `commitment`.
/// Neutral 0.5 reproduces the legacy alternation (odd positions = attackers).
/// Deviations convert the deepest defenders (low positions) up, or the
/// forwardmost attackers (high positions) back, to hit the target count.
fn attacker_positions(n_sup: usize, commitment: f32) -> Vec<bool> {
    let n_att = ((commitment * n_sup as f32).floor() as usize).min(n_sup);
    let mut is_att: Vec<bool> = (0..n_sup).map(|s| s % 2 == 1).collect(); // legacy alternation
    let alt = is_att.iter().filter(|&&a| a).count();
    if n_att > alt {
        let mut need = n_att - alt;
        for s in 0..n_sup {
            if need == 0 { break; }
            if !is_att[s] { is_att[s] = true; need -= 1; }
        }
    } else if n_att < alt {
        let mut need = alt - n_att;
        for s in (0..n_sup).rev() {
            if need == 0 { break; }
            if is_att[s] { is_att[s] = false; need -= 1; }
        }
    }
    is_att
}

/// Choose which player (by `player.index`) should hold the ball-handler role,
/// with hysteresis: the current handler keeps the role unless another teammate
/// is closer to the ball by at least `handoff_margin`. This prevents the
/// frame-to-frame role swapping that makes near-equidistant cubes spin.
fn pick_handler(players: &[TeamMate], ball_pos: Vec3, current_handler: Option<usize>) -> usize {
    // Nearest player (by array position), broken ties by first-seen.
    let mut nearest_pos = 0;
    for k in 1..players.len() {
        if players[k].pos.distance_squared(ball_pos) < players[nearest_pos].pos.distance_squared(ball_pos) {
            nearest_pos = k;
        }
    }
    let nearest_index = players[nearest_pos].index;

    match current_handler.and_then(|h| players.iter().position(|p| p.index == h)) {
        Some(h_pos) => {
            let dist_current = players[h_pos].pos.distance(ball_pos);
            let dist_nearest = players[nearest_pos].pos.distance(ball_pos);
            if dist_nearest + handoff_margin() < dist_current {
                nearest_index
            } else {
                players[h_pos].index
            }
        }
        None => nearest_index,
    }
}

/// Assign a `(movement, jump)` to every player on `team` (aligned with the input
/// `players` order), plus the `player.index` of the chosen ball-handler so the
/// caller can persist it across frames for hysteresis.
///
/// Anti-clumping: exactly one teammate (the sticky ball-handler) chases the ball;
/// the rest hold spread-out support positions (alternating defender /
/// attacking-outlet by index). A short-range teammate-repulsion term keeps them
/// from stacking on the same spot.
pub fn assign_team_movements(
    team: Team,
    players: &[TeamMate],
    ball_pos: Vec3,
    ball_vel: Vec3,
    current_handler: Option<usize>,
    directive: &TeamDirective,
) -> (Vec<(Vec2, bool)>, usize) {
    let n = players.len();
    let handler_index = pick_handler(players, ball_pos, current_handler);

    let mut support_order: Vec<usize> = (0..n).filter(|&k| players[k].index != handler_index).collect();
    support_order.sort_by_key(|&k| players[k].index);
    // Commitment (team-level): how many supports are attackers vs defenders.
    let commitment = directive.base_params().commitment;
    let is_att = attacker_positions(support_order.len(), commitment);
    let mut defender_of = vec![false; n];
    for (s, &k) in support_order.iter().enumerate() {
        defender_of[k] = !is_att[s];
    }

    // Press: the support nearest the ball presses hardest.
    let nearest_support = support_order
        .iter()
        .copied()
        .min_by(|&a, &b| {
            players[a].pos.distance_squared(ball_pos)
                .partial_cmp(&players[b].pos.distance_squared(ball_pos))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

    let mut out = vec![(Vec2::ZERO, false); n];
    for i in 0..n {
        let pos = players[i].pos;
        let p = directive.params_for(players[i].index);

        let (mut base, jump) = if players[i].index == handler_index {
            heuristic_movement(team, pos, ball_pos, ball_vel)
        } else {
            let mut target = support_target(team, defender_of[i], ball_pos, &p);
            let press_w = p.press * if Some(i) == nearest_support { 1.0 } else { SECONDARY_PRESS_FACTOR };
            target = target.lerp(ball_pos, press_w);
            let to_target = Vec2::new(target.x - pos.x, target.z - pos.z);
            let mv = if to_target.length() > support_stop_dist() {
                to_target.normalize_or_zero()
            } else {
                Vec2::ZERO
            };
            (mv, false)
        };

        let mut repulse = Vec2::ZERO;
        for j in 0..n {
            if j == i {
                continue;
            }
            let away = Vec2::new(pos.x - players[j].pos.x, pos.z - players[j].pos.z);
            let d = away.length();
            if d > 1e-3 && d < repulsion_radius() {
                repulse += away.normalize() * ((repulsion_radius() - d) / repulsion_radius());
            }
        }
        base += repulse * (REPULSION_STRENGTH * p.spacing);

        let movement = if base.length() > 1.0 { base.normalize() } else { base };
        out[i] = (movement, jump);
    }

    (out, handler_index)
}

/// The position each player is *supposed* to occupy under `directive`, given the
/// ball. Mirrors the role assignment in [`assign_team_movements`] (sticky-less: the
/// handler is picked fresh from proximity) but returns target **positions** rather
/// than movement directions — the reusable engine for the per-tactic positional
/// -imitation reward (`shape_match`). Result is aligned with the input `players`
/// order. The handler's target is the ball itself; each support's target is its
/// [`support_target`] after the press-lerp toward the ball.
pub fn prescribed_positions(
    team: Team,
    players: &[TeamMate],
    ball_pos: Vec3,
    directive: &TeamDirective,
) -> Vec<Vec3> {
    let n = players.len();
    if n == 0 {
        return Vec::new();
    }
    let handler_index = pick_handler(players, ball_pos, None);

    let mut support_order: Vec<usize> =
        (0..n).filter(|&k| players[k].index != handler_index).collect();
    support_order.sort_by_key(|&k| players[k].index);
    let commitment = directive.base_params().commitment;
    let is_att = attacker_positions(support_order.len(), commitment);
    let mut defender_of = vec![false; n];
    for (s, &k) in support_order.iter().enumerate() {
        defender_of[k] = !is_att[s];
    }

    let nearest_support = support_order.iter().copied().min_by(|&a, &b| {
        players[a].pos.distance_squared(ball_pos)
            .partial_cmp(&players[b].pos.distance_squared(ball_pos))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = vec![Vec3::ZERO; n];
    for i in 0..n {
        out[i] = if players[i].index == handler_index {
            ball_pos
        } else {
            let p = directive.params_for(players[i].index);
            let mut target = support_target(team, defender_of[i], ball_pos, &p);
            let press_w = p.press * if Some(i) == nearest_support { 1.0 } else { SECONDARY_PRESS_FACTOR };
            target = target.lerp(ball_pos, press_w);
            target
        };
    }
    out
}

/// System: gate collision by the player-count curriculum. Players with
/// `index >= active` (on either team) are made non-collidable with the ball and
/// other players — they still rest on the floor via `BARRIER_GROUP`, but the ball
/// and active players pass through them, so the field plays as a true NvN. At the
/// full roster every player is solid. A missing `ActiveRoster` = full roster.
pub fn apply_roster_gating(
    roster: Option<Res<ActiveRoster>>,
    mut query: Query<(&CubePlayer, &mut CollisionGroups)>,
) {
    let active = roster.map(|r| r.0).unwrap_or(PLAYERS_PER_TEAM).clamp(1, PLAYERS_PER_TEAM);
    let solid_filter = CollisionGroups::new(PLAYER_GROUP, Group::ALL);
    // Ghost: collide only with barriers/floor; pass through ball + other players.
    let ghost_filter = CollisionGroups::new(PLAYER_GROUP, BARRIER_GROUP);
    for (player, mut groups) in query.iter_mut() {
        *groups = if player.index < active { solid_filter } else { ghost_filter };
    }
}

/// System: freeze benched players (`index >= active`). Zeroes their control input
/// (so RL/heuristic can't move them or fire powers) and pins their velocity to zero.
/// Combined with [`apply_roster_gating`], benched players are effectively absent.
/// Runs after both controllers so it overrides whatever input they set.
pub fn freeze_inactive_players(
    roster: Option<Res<ActiveRoster>>,
    mut query: Query<(&CubePlayer, &mut PlayerInput, &mut Velocity)>,
) {
    let active = roster.map(|r| r.0).unwrap_or(PLAYERS_PER_TEAM).clamp(1, PLAYERS_PER_TEAM);
    for (player, mut input, mut vel) in query.iter_mut() {
        if player.index >= active {
            input.movement = Vec2::ZERO;
            input.jump = false;
            input.fire = false;
            vel.linvel = Vec3::ZERO;
            vel.angvel = Vec3::ZERO;
        }
    }
}

/// System: drive every `AiControlled` cube using team-aware role assignment.
/// The current ball-handler for each team is remembered in a `Local` so the role
/// is sticky (hysteresis) rather than recomputed from scratch each frame.
pub fn apply_heuristic_ai(
    mut handlers: Local<HashMap<Team, usize>>,
    tactics: Option<Res<TeamTactics>>,
    difficulty: Option<Res<HeuristicDifficulty>>,
    ball_query: Query<(&Transform, &Velocity), With<Ball>>,
    mut player_query: Query<(&mut PlayerInput, &Transform, &CubePlayer), With<AiControlled>>,
) {
    // Difficulty scales how fast the heuristic team moves and whether it uses
    // superpowers. Weak opponent = slow, no powers; full strength = the original.
    let diff = difficulty.map(|d| d.0).unwrap_or(1.0).clamp(0.0, 1.0);
    let Ok((ball_t, ball_v)) = ball_query.get_single() else { return; };
    let ball_pos = ball_t.translation;
    let ball_vel = ball_v.linvel;

    let mut teams: HashMap<Team, Vec<TeamMate>> = HashMap::new();
    for (_, transform, player) in player_query.iter() {
        teams
            .entry(player.team)
            .or_default()
            .push(TeamMate { index: player.index, pos: transform.translation });
    }

    let mut result: HashMap<(Team, usize), (Vec2, bool)> = HashMap::new();
    for (team, mates) in &teams {
        let dir = tactics
            .as_deref()
            .map(|t| match *team { Team::Orange => t.orange.clone(), Team::Blue => t.blue.clone() })
            .unwrap_or_default();
        let current = handlers.get(team).copied();
        let (moves, new_handler) = assign_team_movements(*team, mates, ball_pos, ball_vel, current, &dir);
        handlers.insert(*team, new_handler);
        for (mate, mv) in mates.iter().zip(moves) {
            result.insert((*team, mate.index), mv);
        }
    }

    for (mut input, _transform, player) in player_query.iter_mut() {
        if let Some((movement, jump)) = result.get(&(player.team, player.index)) {
            input.movement = *movement * diff;
            input.jump = *jump;
        }
        // Weak opponents don't use superpowers; they come online past half strength.
        input.fire = diff > 0.5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orange_pushes_toward_positive_x_when_on_ball() {
        let (movement, _) = heuristic_movement(
            Team::Orange, Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.2, 1.0, 0.0), Vec3::ZERO,
        );
        assert!(movement.x > 0.0, "orange should push toward +x goal");
    }

    #[test]
    fn moves_toward_far_ball() {
        let (movement, _) = heuristic_movement(
            Team::Blue, Vec3::new(0.0, 1.0, 0.0), Vec3::new(6.0, 1.0, 0.0), Vec3::ZERO,
        );
        assert!(movement.x > 0.0, "should move toward a ball that is at +x");
    }

    #[test]
    fn nearest_player_handles_ball_others_do_not_clump() {
        // Orange own goal is at -x. Ball at center; one cube near it, one behind.
        let players = vec![
            TeamMate { index: 0, pos: Vec3::new(-1.0, 1.0, 0.0) }, // near the ball
            TeamMate { index: 1, pos: Vec3::new(-3.0, 1.0, 0.0) }, // farther -> support
        ];
        let ball = Vec3::new(0.0, 1.0, 0.0);
        let (moves, handler) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, None, &TeamDirective::uniform(Tactic::Balanced.params()));
        assert_eq!(moves.len(), 2);
        assert_eq!(handler, 0, "nearest cube should take the ball with no prior handler");
        assert!(moves[0].0.x > 0.0, "handler should advance toward the ball/goal");
        assert!(moves[1].0.x < 0.0, "support should cover, not chase the ball");
    }

    #[test]
    fn close_teammates_are_pushed_apart() {
        let players = vec![
            TeamMate { index: 0, pos: Vec3::new(0.0, 1.0, 0.2) },
            TeamMate { index: 1, pos: Vec3::new(0.0, 1.0, -0.2) },
        ];
        let ball = Vec3::new(10.0, 1.0, 0.0);
        let (moves, _) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, None, &TeamDirective::uniform(Tactic::Balanced.params()));
        assert!(
            moves[0].0.y * moves[1].0.y < 0.0,
            "close teammates should be pushed in opposite z directions"
        );
    }

    #[test]
    fn handler_role_sticks_under_hysteresis() {
        // Index 1 is slightly closer to the ball than index 0.
        let players = vec![
            TeamMate { index: 0, pos: Vec3::new(-1.0, 1.0, 0.0) },
            TeamMate { index: 1, pos: Vec3::new(-0.5, 1.0, 0.0) },
        ];
        let ball = Vec3::new(0.0, 1.0, 0.0);
        // With no prior handler, the nearest (index 1) takes the ball.
        let (_, fresh) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, None, &TeamDirective::uniform(Tactic::Balanced.params()));
        assert_eq!(fresh, 1);
        // With index 0 already the handler, index 1 is closer by only 0.5 (< margin),
        // so the role must STICK with index 0 — no frame-to-frame flicker.
        let (_, stuck) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, Some(0), &TeamDirective::uniform(Tactic::Balanced.params()));
        assert_eq!(stuck, 0, "handler should stick despite a slightly closer teammate");
    }

    #[test]
    fn handler_switches_when_teammate_clearly_closer() {
        // Current handler (index 0) is now far; index 1 is much closer.
        let players = vec![
            TeamMate { index: 0, pos: Vec3::new(-5.0, 1.0, 0.0) },
            TeamMate { index: 1, pos: Vec3::new(-0.5, 1.0, 0.0) },
        ];
        let ball = Vec3::new(0.0, 1.0, 0.0);
        let (_, handler) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, Some(0), &TeamDirective::uniform(Tactic::Balanced.params()));
        assert_eq!(handler, 1, "handler should hand off when a teammate is clearly closer");
    }

    #[test]
    fn tactic_from_name_is_case_and_space_insensitive() {
        assert_eq!(Tactic::from_name("Balanced"), Some(Tactic::Balanced));
        assert_eq!(Tactic::from_name("high press"), Some(Tactic::HighPress));
        assert_eq!(Tactic::from_name("LOWBLOCK"), Some(Tactic::LowBlock));
        assert_eq!(Tactic::from_name("nonsense"), None);
    }

    #[test]
    fn directive_default_is_balanced() {
        let d = TeamDirective::default();
        assert_eq!(d.params_for(0), Tactic::Balanced.params());
        assert_eq!(d.params_for(3), Tactic::Balanced.params());
    }

    #[test]
    fn per_player_override_applies() {
        let mut d = TeamDirective::uniform(Tactic::Balanced.params());
        d.set_player(2, Tactic::LowBlock.params());
        assert_eq!(d.params_for(2), Tactic::LowBlock.params(), "overridden index uses override");
        assert_eq!(d.params_for(0), Tactic::Balanced.params(), "other indices use base");
        d.clear_overrides();
        assert_eq!(d.params_for(2), Tactic::Balanced.params(), "cleared -> back to base");
    }

    #[test]
    fn team_tactics_default_both_balanced() {
        let tt = TeamTactics::default();
        assert_eq!(tt.orange.params_for(0), Tactic::Balanced.params());
        assert_eq!(tt.blue.params_for(0), Tactic::Balanced.params());
    }

    #[test]
    fn low_block_defender_is_deeper() {
        // Orange own goal at -x: deeper = smaller x.
        let ball = Vec3::new(0.0, 1.0, 0.0);
        let bal = support_target(Team::Orange, true, ball, &Tactic::Balanced.params());
        let low = support_target(Team::Orange, true, ball, &Tactic::LowBlock.params());
        assert!(low.x < bal.x, "low block defender should sit deeper: low {} vs bal {}", low.x, bal.x);
    }

    #[test]
    fn high_press_attacker_more_advanced() {
        // Orange opp goal at +x: more advanced = larger x.
        let ball = Vec3::new(0.0, 1.0, 0.0);
        let bal = support_target(Team::Orange, false, ball, &Tactic::Balanced.params());
        let hp = support_target(Team::Orange, false, ball, &Tactic::HighPress.params());
        assert!(hp.x > bal.x, "high press attacker should be more advanced: hp {} vs bal {}", hp.x, bal.x);
    }

    #[test]
    fn wide_increases_lateral_spread() {
        let ball = Vec3::new(0.0, 1.0, 2.0);
        let bal = support_target(Team::Orange, true, ball, &Tactic::Balanced.params());
        let wide = support_target(Team::Orange, true, ball, &Tactic::WingPlay.params());
        assert!(wide.z.abs() > bal.z.abs(), "wide should spread laterally: wide {} vs bal {}", wide.z, bal.z);
    }

    #[test]
    fn balanced_params_match_legacy_seven_fields() {
        let p = Tactic::Balanced.params();
        assert_eq!(p, TacticParams {
            defender_depth: 0.50, attacker_push: 0.40, width: 1.0, spacing: 1.0,
            press: 0.0, line_height: 0.0, commitment: 0.5,
        });
    }

    #[test]
    fn tactic_next_cycles_all_ten() {
        let mut t = Tactic::Balanced;
        let mut seen = vec![t];
        for _ in 0..9 { t = t.next(); seen.push(t); }
        assert_eq!(seen.len(), 10);
        assert_eq!(t.next(), Tactic::Balanced, "wraps back to Balanced after 10");
        for i in 0..10 { for j in (i+1)..10 { assert_ne!(seen[i], seen[j]); } }
    }

    #[test]
    fn all_four_presets_roundtrip_names() {
        assert_eq!(Tactic::ALL.len(), 4);
        for t in Tactic::ALL {
            assert_eq!(Tactic::from_name(t.name()), Some(t), "{} must roundtrip", t.name());
        }
        assert_eq!(Tactic::from_name("wing play"), Some(Tactic::WingPlay));
        assert_eq!(Tactic::from_name("HIGH PRESS"), Some(Tactic::HighPress));
        assert_eq!(Tactic::from_name("nonsense"), None);
    }

    #[test]
    fn blend_averages_all_seven_axes() {
        let a = Tactic::Balanced.params();
        let b = Tactic::HighPress.params();
        let m = TacticParams::blend(&[(a, 1.0), (b, 1.0)]);
        assert!((m.press       - (a.press + b.press) / 2.0).abs() < 1e-6);
        assert!((m.line_height - (a.line_height + b.line_height) / 2.0).abs() < 1e-6);
        assert!((m.commitment  - (a.commitment + b.commitment) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn high_press_presses_harder_than_balanced() {
        assert!(Tactic::HighPress.params().press > Tactic::Balanced.params().press);
    }

    #[test]
    fn per_player_override_changes_one_supports_target() {
        // Two supports (indices 0,1) + one far handler (index 2). Overriding index 0
        // to LowBlock must change index 0's movement but not index 1's.
        let players = vec![
            TeamMate { index: 0, pos: Vec3::new(-2.0, 1.0, 1.0) },
            TeamMate { index: 1, pos: Vec3::new(-2.0, 1.0, -1.0) },
            TeamMate { index: 2, pos: Vec3::new(0.2, 1.0, 0.0) }, // nearest -> handler
        ];
        let ball = Vec3::new(0.0, 1.0, 0.0);
        let base = TeamDirective::uniform(Tactic::Balanced.params());
        let (moves_base, _) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, None, &base);

        let mut over = TeamDirective::uniform(Tactic::Balanced.params());
        over.set_player(0, Tactic::LowBlock.params());
        let (moves_over, _) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, None, &over);

        assert!(moves_base[0].0 != moves_over[0].0, "overridden support (index 0) should move differently");
        assert_eq!(moves_base[1].0, moves_over[1].0, "non-overridden support (index 1) should be unchanged");
    }

    #[test]
    fn line_height_shifts_block_forward() {
        let ball = Vec3::new(0.0, 1.0, 0.0);
        let mut hi = Tactic::Balanced.params();
        hi.line_height = 0.3;
        let bal = support_target(Team::Orange, true, ball, &Tactic::Balanced.params());
        let fwd = support_target(Team::Orange, true, ball, &hi);
        assert!(fwd.x > bal.x, "positive line_height shifts Orange support toward +x (opp goal): {} vs {}", fwd.x, bal.x);
    }

    #[test]
    fn line_height_neutral_reproduces_legacy_support_target() {
        let ball = Vec3::new(1.0, 1.0, 2.0);
        let t = support_target(Team::Orange, true, ball, &Tactic::Balanced.params());
        let gx = -FIELD_WIDTH / 2.0; // Orange own goal
        let expx = (ball.x + (gx - ball.x) * 0.5).clamp(-FIELD_WIDTH / 2.0, FIELD_WIDTH / 2.0);
        assert!((t.x - expx).abs() < 1e-5, "neutral line_height must match legacy x");
        assert!((t.z - ball.z * 0.4).abs() < 1e-5, "z unchanged");
    }

    #[test]
    fn commitment_reproduces_alternation_at_neutral() {
        assert_eq!(attacker_positions(4, 0.5), vec![false, true, false, true]);
        assert_eq!(attacker_positions(3, 0.5), vec![false, true, false]);
        assert_eq!(attacker_positions(1, 0.5), vec![false]);
    }

    #[test]
    fn commitment_high_sends_more_attackers() {
        let low = attacker_positions(4, 0.0).iter().filter(|&&a| a).count();
        let high = attacker_positions(4, 1.0).iter().filter(|&&a| a).count();
        assert_eq!(low, 0);
        assert_eq!(high, 4);
        assert!(high > low);
    }

    #[test]
    fn press_pulls_nearest_support_toward_ball() {
        let players = vec![
            TeamMate { index: 0, pos: Vec3::new(-3.0, 1.0, 0.5) },
            TeamMate { index: 1, pos: Vec3::new(-5.0, 1.0, -2.0) },
            TeamMate { index: 2, pos: Vec3::new(0.1, 1.0, 0.0) },
        ];
        let ball = Vec3::new(0.0, 1.0, 0.0);
        let base = TeamDirective::uniform(Tactic::Balanced.params());
        let mut pressing = Tactic::Balanced.params();
        pressing.press = 1.0;
        let dir = TeamDirective::uniform(pressing);
        let (m0, _) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, Some(2), &base);
        let (m1, _) = assign_team_movements(Team::Orange, &players, ball, Vec3::ZERO, Some(2), &dir);
        assert!(m1[0].0.x > m0[0].0.x, "press should pull the nearest support toward the ball: {} vs {}", m1[0].0.x, m0[0].0.x);
    }

    #[test]
    fn normalized_balanced_is_near_zero_center() {
        let n = Tactic::Balanced.params().normalized();
        // defender_depth 0.5, width 1.0, spacing 1.0, line_height 0.0, commitment 0.5
        // all sit at their neutral -> 0.
        assert!((n[0]).abs() < 1e-6, "defender_depth centered");
        assert!((n[2]).abs() < 1e-6, "width centered");
        assert!((n[3]).abs() < 1e-6, "spacing centered");
        assert!((n[5]).abs() < 1e-6, "line_height centered");
        assert!((n[6]).abs() < 1e-6, "commitment centered");
        // press 0.0 maps to -1.0 (its floor).
        assert!((n[4] - (-1.0)).abs() < 1e-6, "press floor -> -1");
    }

    #[test]
    fn normalized_stays_roughly_in_unit_box() {
        for t in Tactic::ALL {
            for (axis, v) in t.params().normalized().iter().enumerate() {
                assert!(v.abs() <= 1.3, "{} axis {axis} out of range: {v}", t.name());
            }
        }
    }

    #[test]
    fn prescribed_low_block_defender_sits_deeper_than_balanced() {
        if PLAYERS_PER_TEAM < 2 { return; }
        // Two supports + a far handler; the ball is central. Low Block must pull the
        // Orange support(s) deeper (smaller x, toward the -x own goal) than Balanced.
        let players = vec![
            TeamMate { index: 0, pos: Vec3::new(-3.0, 1.0, 1.0) },
            TeamMate { index: 1, pos: Vec3::new(-3.0, 1.0, -1.0) },
            TeamMate { index: 2, pos: Vec3::new(0.2, 1.0, 0.0) }, // nearest -> handler
        ];
        let ball = Vec3::new(0.0, 1.0, 0.0);
        let bal = prescribed_positions(Team::Orange, &players, ball, &TeamDirective::uniform(Tactic::Balanced.params()));
        let low = prescribed_positions(Team::Orange, &players, ball, &TeamDirective::uniform(Tactic::LowBlock.params()));
        // Handler (index 2) target is the ball in both.
        assert!((bal[2] - ball).length() < 1e-6);
        // At least one support sits deeper under Low Block.
        let deeper = (0..2).any(|i| low[i].x < bal[i].x - 1e-3);
        assert!(deeper, "low block should pull a support deeper: {bal:?} vs {low:?}");
    }

    #[test]
    fn prescribed_wingplay_is_wider_than_balanced() {
        if PLAYERS_PER_TEAM < 2 { return; }
        let players = vec![
            TeamMate { index: 0, pos: Vec3::new(-3.0, 1.0, 3.0) },
            TeamMate { index: 1, pos: Vec3::new(-3.0, 1.0, -3.0) },
            TeamMate { index: 2, pos: Vec3::new(0.2, 1.0, 0.0) },
        ];
        let ball = Vec3::new(0.0, 1.0, 4.0);
        let bal = prescribed_positions(Team::Orange, &players, ball, &TeamDirective::uniform(Tactic::Balanced.params()));
        let wide = prescribed_positions(Team::Orange, &players, ball, &TeamDirective::uniform(Tactic::WingPlay.params()));
        let spread = |v: &[Vec3]| (0..2).map(|i| v[i].z.abs()).sum::<f32>();
        assert!(spread(&wide) > spread(&bal), "wing play should be wider: {bal:?} vs {wide:?}");
    }

    #[test]
    fn blend_is_weighted_average() {
        let bal = Tactic::Balanced.params();
        let hp = Tactic::HighPress.params();
        let b = TacticParams::blend(&[(bal, 1.0), (hp, 1.0)]);
        assert!((b.defender_depth - (bal.defender_depth + hp.defender_depth) / 2.0).abs() < 1e-6);
        assert!((b.attacker_push  - (bal.attacker_push  + hp.attacker_push)  / 2.0).abs() < 1e-6);
        assert!((b.width          - (bal.width          + hp.width)          / 2.0).abs() < 1e-6);
        assert!((b.spacing        - (bal.spacing        + hp.spacing)        / 2.0).abs() < 1e-6);

        let b2 = TacticParams::blend(&[(bal, 3.0), (hp, 1.0)]);
        assert!((b2.defender_depth - (bal.defender_depth * 0.75 + hp.defender_depth * 0.25)).abs() < 1e-6);

        assert_eq!(TacticParams::blend(&[]), bal);
        assert_eq!(TacticParams::blend(&[(hp, 0.0)]), bal);
    }
}

#[cfg(test)]
mod regression {
    use super::*;

    /// A handler that has run past the ball must turn round, not keep going.
    ///
    /// `heuristic_movement`'s close-range branch drives blindly at the opponent goal. While the
    /// ball is ahead that is a shot; once the handler has overshot, the identical push carries it
    /// *away* from the ball, and because the push is what keeps it past the ball the handler never
    /// returns. Measured against the pre-merge build this cost 40% of the match, with the handler
    /// orbiting ~2.5 units off a ball it never touched.
    #[test]
    fn a_handler_that_overshoots_the_ball_turns_back_to_it() {
        let ball = Vec3::new(0.0, 0.5, 0.0);
        for team in [Team::Orange, Team::Blue] {
            // Just past the ball, on the goal side: the old code drove further away from here.
            let forward = opp_goal_x(team).signum();
            let overshot = Vec3::new(forward * 1.2, 0.5, 0.0);
            let (movement, _) = heuristic_movement(team, overshot, ball, Vec3::ZERO);
            let closes = movement.x * (ball.x - overshot.x) > 0.0;
            assert!(
                closes,
                "{team:?} at x={:+.2} moved x={:+.2}, away from a ball at x={:+.2}",
                overshot.x, movement.x, ball.x
            );
        }
    }

    /// With the ball still ahead, the close-range branch must keep driving at the goal.
    #[test]
    fn a_handler_behind_the_ball_still_drives_at_the_goal() {
        let ball = Vec3::new(0.0, 0.5, 0.0);
        for team in [Team::Orange, Team::Blue] {
            let forward = opp_goal_x(team).signum();
            let behind = Vec3::new(-forward * 1.2, 0.5, 0.0);
            let (movement, _) = heuristic_movement(team, behind, ball, Vec3::ZERO);
            assert_eq!(
                movement.x.signum(), forward,
                "{team:?} should push toward its opponent goal while the ball is ahead"
            );
        }
    }

    /// Engagement distances must track the pitch, not sit at whatever the pitch used to be.
    ///
    /// Support *positions* are fractions of the goal distance and so scale with `FIELD_WIDTH`
    /// already. When these thresholds did not, widening the pitch from 36 to 48 put every
    /// support a third further out than any of them could reach: no teammate was ever close
    /// enough to take the ball role over, so an overshooting handler was never relieved.
    #[test]
    fn engagement_distances_scale_with_the_pitch() {
        let scale = pitch_scale();
        assert!(scale > 0.0, "a pitch with no width is not a pitch");
        assert_eq!(repulsion_radius(), REPULSION_RADIUS_TUNED * scale);
        assert_eq!(handoff_margin(), HANDOFF_MARGIN_TUNED * scale);
        assert_eq!(engage_radius(), ENGAGE_RADIUS_TUNED * scale);
        assert_eq!(support_stop_dist(), SUPPORT_STOP_DIST_TUNED * scale);

        // The support a handoff has to reach must stay inside the radius that detects it,
        // whatever the pitch: this is the invariant whose breach broke the AI.
        let reachable = support_target(
            Team::Blue, true, Vec3::ZERO,
            &Tactic::Balanced.params(),
        );
        assert!(
            reachable.x.abs() <= FIELD_WIDTH / 2.0,
            "a support target outside the pitch cannot be defended"
        );
    }
}
