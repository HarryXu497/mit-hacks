use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use std::collections::HashMap;
use crate::entities::{Ball, CubePlayer, PlayerInput};
use crate::game::{Team, FIELD_WIDTH};

/// Marker: cubes with this component are driven by the built-in heuristic AI.
#[derive(Component)]
pub struct AiControlled;

// Tuning for the role-based team AI.
const SUPPORT_STOP_DIST: f32 = 0.6;   // support players stop when this close to their target
const REPULSION_RADIUS: f32 = 2.5;    // teammates within this distance push each other apart
const REPULSION_STRENGTH: f32 = 0.8;  // how hard the spacing push is
const HANDOFF_MARGIN: f32 = 1.5;      // a teammate must be this much closer to steal the ball role

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
}

impl TacticParams {
    /// Weighted blend of several param sets ("70% Low Block, 30% Wide").
    /// Weights are normalized by their sum; empty input or zero total weight
    /// returns `Balanced`.
    pub fn blend(parts: &[(TacticParams, f32)]) -> TacticParams {
        let total: f32 = parts.iter().map(|(_, w)| *w).sum();
        if parts.is_empty() || total.abs() < 1e-6 {
            return Tactic::Balanced.params();
        }
        let mut acc = TacticParams { defender_depth: 0.0, attacker_push: 0.0, width: 0.0, spacing: 0.0 };
        for (p, w) in parts {
            let f = w / total;
            acc.defender_depth += p.defender_depth * f;
            acc.attacker_push += p.attacker_push * f;
            acc.width += p.width * f;
            acc.spacing += p.spacing * f;
        }
        acc
    }
}

/// The named tactic presets. Expand toward ~10 for the coaching-game end goal.
// TODO(vocab): add more presets (e.g. Counter, Tiki-Taka, Park-the-Bus, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tactic { Balanced, HighPress, LowBlock, Wide }

impl Tactic {
    pub const ALL: [Tactic; 4] = [Tactic::Balanced, Tactic::HighPress, Tactic::LowBlock, Tactic::Wide];

    pub fn params(self) -> TacticParams {
        match self {
            Tactic::Balanced  => TacticParams { defender_depth: 0.50, attacker_push: 0.40, width: 1.0, spacing: 1.0 },
            Tactic::HighPress => TacticParams { defender_depth: 0.25, attacker_push: 0.65, width: 1.0, spacing: 1.1 },
            Tactic::LowBlock  => TacticParams { defender_depth: 0.80, attacker_push: 0.15, width: 0.8, spacing: 0.9 },
            Tactic::Wide      => TacticParams { defender_depth: 0.50, attacker_push: 0.45, width: 1.5, spacing: 1.3 },
        }
    }

    pub fn next(self) -> Tactic {
        match self {
            Tactic::Balanced => Tactic::HighPress,
            Tactic::HighPress => Tactic::LowBlock,
            Tactic::LowBlock => Tactic::Wide,
            Tactic::Wide => Tactic::Balanced,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tactic::Balanced => "Balanced",
            Tactic::HighPress => "High Press",
            Tactic::LowBlock => "Low Block",
            Tactic::Wide => "Wide",
        }
    }

    /// Resolve a preset by name, case- and space-insensitive; `None` if unknown.
    pub fn from_name(s: &str) -> Option<Tactic> {
        match s.trim().to_lowercase().replace(' ', "").as_str() {
            "balanced" => Some(Tactic::Balanced),
            "highpress" => Some(Tactic::HighPress),
            "lowblock" => Some(Tactic::LowBlock),
            "wide" => Some(Tactic::Wide),
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

    let movement = if dist_to_ball < 2.5 {
        Vec2::new((target_goal_x - player_pos.x).signum(), (-ball_pos.z).clamp(-0.5, 0.5))
    } else {
        Vec2::new(to_pred.x.clamp(-1.0, 1.0), to_pred.z.clamp(-1.0, 1.0)).normalize_or_zero()
    };

    let jump = dist_to_ball < 3.0 && ball_pos.y > player_pos.y + 0.5;
    (movement, jump)
}

/// Home position a support player should hold instead of chasing the ball.
/// `defender` covers the lane toward the team's own goal; otherwise the player
/// pushes up-field as an attacking outlet, offset in Z for width.
fn support_target(team: Team, defender: bool, ball_pos: Vec3, p: &TacticParams) -> Vec3 {
    if defender {
        let gx = own_goal_x(team);
        Vec3::new(ball_pos.x + (gx - ball_pos.x) * p.defender_depth, ball_pos.y, ball_pos.z * 0.4 * p.width)
    } else {
        let gx = opp_goal_x(team);
        Vec3::new(ball_pos.x + (gx - ball_pos.x) * p.attacker_push, ball_pos.y, -ball_pos.z * 0.6 * p.width)
    }
}

/// Choose which player (by `player.index`) should hold the ball-handler role,
/// with hysteresis: the current handler keeps the role unless another teammate
/// is closer to the ball by at least `HANDOFF_MARGIN`. This prevents the
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
            if dist_nearest + HANDOFF_MARGIN < dist_current {
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
    let mut defender_of = vec![false; n];
    for (s, &k) in support_order.iter().enumerate() {
        defender_of[k] = s % 2 == 0;
    }

    let mut out = vec![(Vec2::ZERO, false); n];
    for i in 0..n {
        let pos = players[i].pos;
        let p = directive.params_for(players[i].index);

        let (mut base, jump) = if players[i].index == handler_index {
            heuristic_movement(team, pos, ball_pos, ball_vel)
        } else {
            let target = support_target(team, defender_of[i], ball_pos, &p);
            let to_target = Vec2::new(target.x - pos.x, target.z - pos.z);
            let mv = if to_target.length() > SUPPORT_STOP_DIST {
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
            if d > 1e-3 && d < REPULSION_RADIUS {
                repulse += away.normalize() * ((REPULSION_RADIUS - d) / REPULSION_RADIUS);
            }
        }
        base += repulse * (REPULSION_STRENGTH * p.spacing);

        let movement = if base.length() > 1.0 { base.normalize() } else { base };
        out[i] = (movement, jump);
    }

    (out, handler_index)
}

/// System: drive every `AiControlled` cube using team-aware role assignment.
/// The current ball-handler for each team is remembered in a `Local` so the role
/// is sticky (hysteresis) rather than recomputed from scratch each frame.
pub fn apply_heuristic_ai(
    mut handlers: Local<HashMap<Team, usize>>,
    tactics: Option<Res<TeamTactics>>,
    ball_query: Query<(&Transform, &Velocity), With<Ball>>,
    mut player_query: Query<(&mut PlayerInput, &Transform, &CubePlayer), With<AiControlled>>,
) {
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
            input.movement = *movement;
            input.jump = *jump;
        }
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
    fn balanced_params_match_legacy() {
        let p = Tactic::Balanced.params();
        assert_eq!(p, TacticParams { defender_depth: 0.50, attacker_push: 0.40, width: 1.0, spacing: 1.0 });
    }

    #[test]
    fn tactic_next_cycles() {
        assert_eq!(Tactic::Balanced.next(), Tactic::HighPress);
        assert_eq!(Tactic::HighPress.next(), Tactic::LowBlock);
        assert_eq!(Tactic::LowBlock.next(), Tactic::Wide);
        assert_eq!(Tactic::Wide.next(), Tactic::Balanced);
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
        let wide = support_target(Team::Orange, true, ball, &Tactic::Wide.params());
        assert!(wide.z.abs() > bal.z.abs(), "wide should spread laterally: wide {} vs bal {}", wide.z, bal.z);
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
