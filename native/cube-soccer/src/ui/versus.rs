//! The wait before kickoff, and the VS screen it leads into.
//!
//! A character is forged from a player's drawing while they are still coaching, which takes about
//! a minute against a coaching phase that takes longer. So by the time the match opens the models
//! are normally already there and this screen never appears.
//!
//! "Normally" is not "always", so the intro is three states rather than an assumption:
//!
//! 1. [`IntroState::Forging`] — a loading bar, and only if something is still being built. If both
//!    sides have settled by the time the match opens, this state exits on its first frame and
//!    nobody sees it.
//! 2. [`IntroState::Versus`] — both characters, face to face, held for a beat.
//! 3. [`IntroState::Done`] — the match proper.
//!
//! The bar is always skippable, and skipping is not the same as cancelling. The server keeps
//! forging after a skip, because the cost is already spent and
//! [`WornCharacters`](crate::entities::character::WornCharacters) can re-dress a side mid-match —
//! so a model that misses the VS screen still arrives during the first round. Whatever has genuinely
//! finished is worn; anything unfinished stays the base monkey.
//!
//! # Why a file and not a request
//!
//! Progress is read from `assets/characters/forge-status.json`, which the Node server writes on
//! every stage change. The game has no HTTP client, and adding one would mean an async runtime
//! inside a Bevy schedule for a file that changes a handful of times per match. The binary already
//! runs with the repo root as its working directory, so a file in the asset root is the cheapest
//! channel that is actually correct. A missing or malformed file is not an error: the bar simply
//! has nothing to say and the cap below still moves things along.

use std::fs;
use std::time::Duration;

use bevy::prelude::*;

use crate::entities::character::WornCharacters;
use crate::game::config::Team;

/// Where the server publishes forge progress, relative to the working directory.
const STATUS_PATH: &str = "assets/characters/forge-status.json";

/// How often to re-read it. The file changes a handful of times a match; polling faster would
/// only burn syscalls.
const POLL_INTERVAL: Duration = Duration::from_millis(400);

/// The longest the loading bar will ever hold, skip or no skip.
///
/// A generation that has not landed by now is not going to save the kickoff, and a match that
/// refuses to start is far worse than one that starts in the base monkey. The forge keeps running
/// regardless, so the model still arrives in play.
const FORGING_CAP: Duration = Duration::from_secs(90);

/// How long the VS screen holds once it appears.
const VERSUS_HOLD: Duration = Duration::from_millis(2600);

/// What the intro is doing.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IntroState {
    /// Waiting on a character that is still being forged. Skippable, and skipped automatically
    /// when there is nothing left to wait for.
    #[default]
    Forging,
    /// Both characters, face to face.
    Versus,
    /// Out of the way; the match is running.
    Done,
}

/// One side's forge job, as last published by the server.
#[derive(Debug, Clone, Default)]
pub struct TeamForge {
    pub status: String,
    pub label: String,
    /// Path under `assets/`, already in the `scene.glb#Scene0` form the loader wants.
    pub asset: String,
    pub forged: bool,
    pub summary: String,
}

impl TeamForge {
    /// Whether this side has stopped changing — forged, or failed and never coming.
    fn settled(&self) -> bool {
        self.status == "ready" || self.status == "failed"
    }
}

/// The latest published progress for both sides.
#[derive(Resource, Default)]
pub struct ForgeProgress {
    pub orange: Option<TeamForge>,
    pub blue: Option<TeamForge>,
    /// Set when the player skips, so the bar stops waiting. The server is not told to stop.
    pub skipped: bool,
    elapsed: Duration,
    since_poll: Duration,
}

impl ForgeProgress {
    fn side(&self, team: Team) -> Option<&TeamForge> {
        match team {
            Team::Orange => self.orange.as_ref(),
            Team::Blue => self.blue.as_ref(),
        }
    }

    /// Nothing left worth waiting for: every side we know about has settled.
    ///
    /// A side with no job at all counts as settled — a solo match only ever forges one character,
    /// and waiting for a drawing nobody made would hang every single-player kickoff.
    fn quiet(&self) -> bool {
        [self.orange.as_ref(), self.blue.as_ref()]
            .into_iter()
            .flatten()
            .all(TeamForge::settled)
    }

    /// Rough fraction done, for the bar. Stage-weighted from measured durations rather than a
    /// linear guess, so the bar does not sit at 40% through the longest stage.
    fn fraction(&self) -> f32 {
        let of = |job: Option<&TeamForge>| -> f32 {
            match job.map(|forge| forge.status.as_str()) {
                Some("queued") => 0.02,
                Some("reading") => 0.08,
                Some("generating") => 0.45,
                Some("assembling") => 0.80,
                Some("ready") | Some("failed") => 1.0,
                _ => 1.0,
            }
        };
        // The pair moves at the pace of the slower side, which is what the player is waiting on.
        of(self.orange.as_ref()).min(of(self.blue.as_ref()))
    }

    /// What to put under the bar.
    fn caption(&self) -> String {
        for job in [self.orange.as_ref(), self.blue.as_ref()].into_iter().flatten() {
            if !job.settled() && !job.label.is_empty() {
                return job.label.clone();
            }
        }
        "Preparing the teams".to_owned()
    }
}

#[derive(Component)]
struct ForgingScreen;

#[derive(Component)]
struct VersusScreen;

#[derive(Component)]
struct ForgeBar;

#[derive(Component)]
struct ForgeCaption;

#[derive(Component)]
struct SkipButton;

/// Read the published status, and dress each side the moment its model is genuinely ready.
///
/// Dressing here rather than on the way out of the state matters: it starts Bevy loading the glTF
/// while the bar is still up, so the VS screen shows a scene that is already resident instead of
/// beginning a load when it opens.
fn poll_forge_status(
    time: Res<Time>,
    mut progress: ResMut<ForgeProgress>,
    mut worn: ResMut<WornCharacters>,
) {
    progress.elapsed += time.delta();
    progress.since_poll += time.delta();
    if progress.since_poll < POLL_INTERVAL {
        return;
    }
    progress.since_poll = Duration::ZERO;

    let Ok(raw) = fs::read_to_string(STATUS_PATH) else {
        // No server, or it has not written yet. Not a failure: the cap still moves things on.
        return;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };

    for team in [Team::Orange, Team::Blue] {
        let key = match team {
            Team::Orange => "orange",
            Team::Blue => "blue",
        };
        let Some(node) = parsed.get("teams").and_then(|teams| teams.get(key)) else {
            continue;
        };
        let text = |field: &str| {
            node.get(field)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let forge = TeamForge {
            status: text("status"),
            label: text("label"),
            asset: text("asset"),
            forged: node.get("forged").and_then(serde_json::Value::as_bool).unwrap_or(false),
            summary: text("summary"),
        };

        // Only a genuinely forged model replaces anything. A failed job leaves the base monkey,
        // which is already what the team is wearing.
        if forge.forged && !forge.asset.is_empty() && worn.get(team) != Some(forge.asset.as_str()) {
            info!("wearing forged character for {team:?}: {}", forge.asset);
            worn.set(team, Some(forge.asset.clone()));
        }

        match team {
            Team::Orange => progress.orange = Some(forge),
            Team::Blue => progress.blue = Some(forge),
        }
    }
}

/// Leave the bar as soon as there is nothing to wait for, the player says so, or the cap expires.
fn leave_forging(
    mut progress: ResMut<ForgeProgress>,
    keys: Res<ButtonInput<KeyCode>>,
    mut buttons: Query<&Interaction, (Changed<Interaction>, With<SkipButton>)>,
    mut next: ResMut<NextState<IntroState>>,
) {
    let pressed = keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Escape)
        || buttons.iter_mut().any(|interaction| *interaction == Interaction::Pressed);

    if pressed {
        progress.skipped = true;
    }

    if progress.quiet() || pressed || progress.elapsed >= FORGING_CAP {
        next.set(IntroState::Versus);
    }
}

fn spawn_forging_screen(mut commands: Commands) {
    commands
        .spawn((
            ForgingScreen,
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(18.0),
                    ..default()
                },
                background_color: Color::rgba(0.03, 0.05, 0.04, 0.92).into(),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn(TextBundle::from_section(
                "FORGING YOUR TEAM",
                TextStyle { font_size: 44.0, color: Color::rgb(0.98, 0.86, 0.45), ..default() },
            ));

            // The bar's track. The fill inside it is what `update_forging_screen` drives.
            root.spawn(NodeBundle {
                style: Style {
                    width: Val::Px(520.0),
                    height: Val::Px(16.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                border_color: Color::rgb(0.12, 0.10, 0.08).into(),
                background_color: Color::rgba(1.0, 1.0, 1.0, 0.10).into(),
                ..default()
            })
            .with_children(|track| {
                track.spawn((
                    ForgeBar,
                    NodeBundle {
                        style: Style { width: Val::Percent(2.0), height: Val::Percent(100.0), ..default() },
                        background_color: Color::rgb(0.98, 0.72, 0.22).into(),
                        ..default()
                    },
                ));
            });

            root.spawn((
                ForgeCaption,
                TextBundle::from_section(
                    "Preparing the teams",
                    TextStyle { font_size: 20.0, color: Color::rgb(0.86, 0.88, 0.84), ..default() },
                ),
            ));

            root.spawn((
                SkipButton,
                ButtonBundle {
                    style: Style {
                        padding: UiRect::axes(Val::Px(22.0), Val::Px(10.0)),
                        margin: UiRect::top(Val::Px(14.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    border_color: Color::rgb(0.98, 0.72, 0.22).into(),
                    background_color: Color::rgba(0.0, 0.0, 0.0, 0.35).into(),
                    ..default()
                },
            ))
            .with_children(|button| {
                button.spawn(TextBundle::from_section(
                    "SKIP  (space)",
                    TextStyle { font_size: 18.0, color: Color::rgb(0.98, 0.86, 0.45), ..default() },
                ));
            });
        });
}

fn update_forging_screen(
    progress: Res<ForgeProgress>,
    mut bars: Query<&mut Style, With<ForgeBar>>,
    mut captions: Query<&mut Text, With<ForgeCaption>>,
) {
    let fraction = progress.fraction().clamp(0.0, 1.0);
    for mut style in &mut bars {
        style.width = Val::Percent(fraction * 100.0);
    }
    let caption = progress.caption();
    for mut text in &mut captions {
        if let Some(section) = text.sections.first_mut() {
            if section.value != caption {
                section.value = caption.clone();
            }
        }
    }
}

fn spawn_versus_screen(mut commands: Commands, progress: Res<ForgeProgress>) {
    let name = |team: Team| {
        progress
            .side(team)
            .filter(|forge| forge.forged && !forge.summary.is_empty())
            .map(|forge| forge.summary.to_uppercase())
            .unwrap_or_else(|| "THE MONKEYS".to_owned())
    };

    commands
        .spawn((
            VersusScreen,
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceEvenly,
                    ..default()
                },
                background_color: Color::rgba(0.02, 0.03, 0.03, 0.72).into(),
                ..default()
            },
        ))
        .with_children(|root| {
            for (team, tint) in [
                (Team::Orange, Color::rgb(0.97, 0.55, 0.16)),
                (Team::Blue, Color::rgb(0.32, 0.62, 0.95)),
            ] {
                if team == Team::Blue {
                    root.spawn(TextBundle::from_section(
                        "VS",
                        TextStyle { font_size: 88.0, color: Color::rgb(0.98, 0.86, 0.45), ..default() },
                    ));
                }
                root.spawn(TextBundle::from_section(
                    name(team),
                    TextStyle { font_size: 34.0, color: tint, ..default() },
                ));
            }
        });
}

/// Hold the VS screen for a beat, then get out of the way.
fn leave_versus(
    time: Res<Time>,
    mut held: Local<Duration>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<IntroState>>,
) {
    *held += time.delta();
    if *held >= VERSUS_HOLD || keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
        next.set(IntroState::Done);
    }
}

fn despawn<M: Component>(mut commands: Commands, screens: Query<Entity, With<M>>) {
    for screen in &screens {
        commands.entity(screen).despawn_recursive();
    }
}

/// The intro: a skippable wait on the forge, then the VS screen.
pub struct VersusPlugin;

impl Plugin for VersusPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<IntroState>()
            .init_resource::<ForgeProgress>()
            // Polling runs in Forging *and* Versus so a model that lands late still gets worn,
            // and keeps running after Done for the same reason: a skipped job is still coming.
            .add_systems(Update, poll_forge_status)
            .add_systems(OnEnter(IntroState::Forging), spawn_forging_screen)
            .add_systems(
                Update,
                (update_forging_screen, leave_forging).run_if(in_state(IntroState::Forging)),
            )
            .add_systems(OnExit(IntroState::Forging), despawn::<ForgingScreen>)
            .add_systems(OnEnter(IntroState::Versus), spawn_versus_screen)
            .add_systems(Update, leave_versus.run_if(in_state(IntroState::Versus)))
            .add_systems(OnExit(IntroState::Versus), despawn::<VersusScreen>);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forge(status: &str) -> TeamForge {
        TeamForge { status: status.to_owned(), ..Default::default() }
    }

    #[test]
    fn a_side_with_no_job_does_not_hold_the_kickoff() {
        // A solo match forges one character. Waiting on a drawing nobody made would hang forever.
        let progress = ForgeProgress { orange: Some(forge("ready")), ..Default::default() };
        assert!(progress.quiet());
    }

    #[test]
    fn a_failed_forge_counts_as_settled() {
        // An unreachable GPU box must not keep the bar up; the base monkey plays.
        let progress = ForgeProgress { orange: Some(forge("failed")), ..Default::default() };
        assert!(progress.quiet());
    }

    #[test]
    fn work_still_running_holds_the_bar() {
        let progress = ForgeProgress {
            orange: Some(forge("ready")),
            blue: Some(forge("assembling")),
            ..Default::default()
        };
        assert!(!progress.quiet());
    }

    #[test]
    fn the_bar_moves_at_the_pace_of_the_slower_side() {
        let progress = ForgeProgress {
            orange: Some(forge("ready")),
            blue: Some(forge("reading")),
            ..Default::default()
        };
        assert!(progress.fraction() < 0.2, "a finished side must not imply the pair is nearly done");
    }

    #[test]
    fn nothing_pending_reads_as_complete() {
        assert_eq!(ForgeProgress::default().fraction(), 1.0);
    }

    #[test]
    fn the_caption_names_the_side_still_working() {
        let mut blue = forge("generating");
        blue.label = "Drawing the suit with trousers and tie".to_owned();
        let progress = ForgeProgress {
            orange: Some(forge("ready")),
            blue: Some(blue),
            ..Default::default()
        };
        assert_eq!(progress.caption(), "Drawing the suit with trousers and tie");
    }
}
