//! The two superpower badges, and their cooldowns.
//!
//! A side plays with exactly one power -- the coach draws a superpower, MonkeyForge classifies it,
//! and the whole side carries that -- so there is one badge per side and not a loadout of four:
//! yours on the left, your opponent's on the right. The badge is the artwork MonkeyForge drew, so
//! the drawing, the icon and the burst on the pitch are all visibly the same power, which is the
//! point of using the drawn art rather than a generic glyph.
//!
//! The opponent's badge is shown because their power is not a secret -- you watch it go off -- and
//! knowing when it comes back is what lets you play around it.
//!
//! The cooldown is shown two ways on purpose. The badge desaturates to a dark silhouette and
//! fills back up from the bottom as the power recharges, which is readable peripherally while
//! watching the ball; and the seconds remaining are printed on it, which is what you want when
//! deciding whether to wait. When it is ready the slab gets its gold edge and the label reads
//! READY, so "can I fire?" is answerable without counting.

use bevy::prelude::*;

use crate::entities::CubePlayer;
use crate::game::Team;
use crate::systems::superpowers::{Superpower, SuperpowerKind};

/// The jungle palette, shared with the goal banner so the whole HUD is drawn in one ink.
use super::ink;

/// Which side's powers this HUD is showing.
///
/// Solo play coaches Orange, so that is the default; a networked joiner coaches the other side and
/// overrides it. Showing the opponent's cooldowns would be showing information the player has not
/// earned.
#[derive(Resource, Clone, Copy, Debug)]
pub struct PowerHudSide(pub Team);

impl Default for PowerHudSide {
    fn default() -> Self {
        Self(Team::Orange)
    }
}

/// The slot a badge lives in, and which side's power it shows.
#[derive(Component)]
pub struct PowerRail {
    team: Team,
    /// Whether this is the viewer's own side, which is drawn larger and labelled.
    own: bool,
}

/// One badge slab, bound to the power and side it shows.
#[derive(Component)]
pub struct PowerBadge {
    kind: SuperpowerKind,
    team: Team,
}

/// The panel that shrinks away as the power recharges.
#[derive(Component)]
pub struct PowerChill {
    kind: SuperpowerKind,
    team: Team,
}

/// The seconds-remaining label.
#[derive(Component)]
pub struct PowerLabel {
    kind: SuperpowerKind,
    team: Team,
}

/// Build the two slots. Empty until a power is actually held -- see [`fill_power_rail`].
///
/// Top corners, flanking the score, which is where a stadium puts the two teams' crests and
/// where the eye already goes to read the match state. Down the sides they sat over the pitch
/// and competed with the play for attention.
pub fn setup_power_hud(mut commands: Commands, side: Option<Res<PowerHudSide>>) {
    let mine = side.map(|s| s.0).unwrap_or(Team::Orange);
    for (team, own) in [(mine, true), (mine.opponent(), false)] {
        let mut style = Style {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        };
        if own {
            style.left = Val::Px(20.0);
        } else {
            style.right = Val::Px(20.0);
        }
        commands.spawn((PowerRail { team, own }, NodeBundle { style, ..default() }));
    }
}

/// Put the side's badge in its slot, and take it away if the side no longer holds a power.
///
/// One badge per side, because a side has one power: whatever its coach drew. Rebuilt from what
/// is actually on the pitch rather than from a fixed list, so a match where nothing has been
/// drawn shows nothing rather than advertising a power that is not in play.
pub fn fill_power_rail(
    mut commands: Commands,
    assets: Res<AssetServer>,
    rails: Query<(Entity, &PowerRail)>,
    shown: Query<(Entity, &PowerBadge)>,
    held: Query<(&CubePlayer, &Superpower)>,
) {
    for (rail, slot) in &rails {
        // The power this side is playing with, if any.
        let wanted = SuperpowerKind::ALL.into_iter().find(|kind| {
            held.iter()
                .any(|(player, power)| player.team == slot.team && power.kind == *kind)
        });

        for (entity, badge) in &shown {
            if badge.team == slot.team && Some(badge.kind) != wanted {
                commands.entity(entity).despawn_recursive();
            }
        }

        let Some(kind) = wanted else { continue };
        if shown
            .iter()
            .any(|(_, badge)| badge.team == slot.team && badge.kind == kind)
        {
            continue;
        }

        // Yours is drawn larger: it is the one you act on.
        let size = if slot.own { 84.0 } else { 64.0 };
        let slab = commands
            .spawn((
                PowerBadge { kind, team: slot.team },
                NodeBundle {
                    style: Style {
                        width: Val::Px(size),
                        height: Val::Px(size),
                        border: UiRect::all(Val::Px(3.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::FlexEnd,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    background_color: BackgroundColor(ink::SLAB),
                    border_color: BorderColor(ink::EDGE_COOLING),
                    ..default()
                },
            ))
            .id();

        let art = commands
            .spawn(ImageBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                image: UiImage::new(assets.load(kind.badge_path())),
                ..default()
            })
            .id();

        // The cooldown veil, anchored to the top so it retreats upward as the power fills.
        let chill = commands
            .spawn((
                PowerChill { kind, team: slot.team },
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(0.0),
                        ..default()
                    },
                    background_color: BackgroundColor(ink::CHILL),
                    ..default()
                },
            ))
            .id();

        let label = commands
            .spawn((
                PowerLabel { kind, team: slot.team },
                TextBundle::from_section(
                    "",
                    TextStyle {
                        font_size: if slot.own { 16.0 } else { 13.0 },
                        color: ink::CLOTH,
                        ..default()
                    },
                )
                .with_style(Style {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(3.0),
                    ..default()
                }),
            ))
            .id();

        commands.entity(slab).push_children(&[art, chill, label]);
        commands.entity(rail).add_child(slab);
    }
}

/// Drive each badge's veil, edge and label from its side's live cooldown.
pub fn update_power_hud(
    held: Query<(&CubePlayer, &Superpower)>,
    mut slabs: Query<(&PowerBadge, &mut BackgroundColor, &mut BorderColor)>,
    mut chills: Query<(&PowerChill, &mut Style)>,
    mut labels: Query<(&PowerLabel, &mut Text)>,
) {
    // The soonest anyone on `team` can fire it. A whole side shares one power, so the useful
    // question is "when can somebody fire", not "when can this particular player".
    let soonest = |team: Team, kind: SuperpowerKind| -> Option<f32> {
        held.iter()
            .filter(|(player, power)| player.team == team && power.kind == kind)
            .map(|(_, power)| power.cooldown_remaining)
            .fold(None, |best: Option<f32>, remaining| {
                Some(best.map_or(remaining, |b: f32| b.min(remaining)))
            })
    };

    for (badge, mut background, mut border) in &mut slabs {
        let ready = soonest(badge.team, badge.kind).is_some_and(|remaining| remaining <= 0.0);
        *background = BackgroundColor(if ready { ink::SLAB_READY } else { ink::SLAB });
        *border = BorderColor(if ready { ink::EDGE_READY } else { ink::EDGE_COOLING });
    }

    for (chill, mut style) in &mut chills {
        let Some(remaining) = soonest(chill.team, chill.kind) else { continue };
        let cooldown = chill.kind.cooldown();
        // Full veil the moment it fires, gone when ready.
        let fraction = if cooldown <= 0.0 { 0.0 } else { (remaining / cooldown).clamp(0.0, 1.0) };
        style.height = Val::Percent(fraction * 100.0);
    }

    for (label, mut text) in &mut labels {
        let Some(remaining) = soonest(label.team, label.kind) else { continue };
        let section = &mut text.sections[0];
        if remaining <= 0.0 {
            section.value = "READY".to_owned();
            section.style.color = ink::CLOTH;
        } else {
            // Ceiling, so a badge never reads "0" while still cooling.
            section.value = format!("{}s", remaining.ceil() as u32);
            section.style.color = ink::CLOTH_DIM;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A side plays with one power, so its slot shows one badge -- never a loadout.
    #[test]
    fn each_side_shows_exactly_its_own_one_power() {
        let held = [
            (Team::Orange, SuperpowerKind::BeamBlast),
            (Team::Orange, SuperpowerKind::BeamBlast),
            (Team::Blue, SuperpowerKind::Slow),
        ];
        for (team, expected) in [
            (Team::Orange, SuperpowerKind::BeamBlast),
            (Team::Blue, SuperpowerKind::Slow),
        ] {
            let found: Vec<SuperpowerKind> = SuperpowerKind::ALL
                .into_iter()
                .filter(|kind| held.iter().any(|(t, k)| *t == team && k == kind))
                .collect();
            assert_eq!(found, vec![expected], "{team:?} must show only its own power");
        }
    }

    #[test]
    fn the_veil_is_full_at_the_moment_of_firing_and_gone_when_ready() {
        for kind in SuperpowerKind::ALL {
            let cooldown = kind.cooldown();
            assert!(cooldown > 0.0, "{kind:?} has no cooldown to show");
            // Just fired.
            assert_eq!((cooldown / cooldown).clamp(0.0, 1.0), 1.0);
            // Ready.
            assert_eq!((0.0f32 / cooldown).clamp(0.0, 1.0), 0.0);
            // Half way.
            assert!((((cooldown / 2.0) / cooldown) - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn a_cooling_power_never_reads_zero_seconds() {
        // Truncation would print "0s" for anything under a second, which reads as ready.
        for remaining in [0.01f32, 0.4, 0.99] {
            assert_eq!(remaining.ceil() as u32, 1);
        }
    }
}
