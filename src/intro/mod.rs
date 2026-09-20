//! Front of house: the cold open, the title card, the mode menu and the drop
//! into a match.
//!
//! Everything here is presentation. No system in this module writes to a
//! collider, a rigid body or the score, so the trained policy sees the same
//! world whether the player arrived through the menu or straight into a match.
//!
//! The scene itself is never rebuilt. The stadium is spawned once at startup
//! and the front end simply points a different camera at it under different
//! light, which is why the title card costs a camera path and some text rather
//! than a second level.
//!
//! # Modules
//!
//! - [`director`]: camera shots, the cold-open path and the idle attract tour
//! - [`atmosphere`]: letterbox, vignette and the golden-hour light shift
//! - [`ui`]: title card, mode list and the outlined text they are built from

pub mod atmosphere;
pub mod director;
pub mod ui;

use bevy::app::AppExit;
use bevy::prelude::*;

/// Where the player is in the front end. [`AppState::Playing`] is the only
/// variant in which match logic ticks.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    /// Scripted flight up out of the valley into the stadium bowl. Skippable.
    #[default]
    ColdOpen,
    /// Hero shot, logo, "press any key". Falls into an attract tour when idle.
    Title,
    /// Mode list. Each entry parks the camera somewhere different.
    Menu,
    /// The camera flies to the broadcast pose before the whistle, so handing
    /// over to the match camera is a continuation rather than a cut.
    DropIn,
    /// A match is running.
    Playing,
}

/// True while the front end owns the camera and match logic is idle.
pub fn in_front_end(state: Res<State<AppState>>) -> bool {
    *state.get() != AppState::Playing
}

/// True once a match has started.
pub fn playing(state: Res<State<AppState>>) -> bool {
    *state.get() == AppState::Playing
}

/// Anything that counts as "press any key". Deliberately excludes modifiers, so
/// alt-tabbing back into the window does not skip the opening.
fn any_key(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    mouse.get_just_pressed().len() > 0
        || keys.get_just_pressed().any(|key| {
            !matches!(
                key,
                KeyCode::ShiftLeft
                    | KeyCode::ShiftRight
                    | KeyCode::ControlLeft
                    | KeyCode::ControlRight
                    | KeyCode::AltLeft
                    | KeyCode::AltRight
                    | KeyCode::SuperLeft
                    | KeyCode::SuperRight
            )
        })
}

/// Resets the idle clock on any input, which is what pulls the camera back out
/// of the attract tour.
fn note_activity(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut director: ResMut<director::Director>,
) {
    if any_key(&keys, &mouse) {
        director.idle = 0.;
    }
}

/// The opening flight plays out, or ends early on any key.
fn advance_cold_open(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    director: Res<director::Director>,
    mut next: ResMut<NextState<AppState>>,
) {
    if any_key(&keys, &mouse) || director.elapsed >= director::COLD_OPEN_SECONDS {
        next.set(AppState::Title);
    }
}

/// On the title card, any key opens the menu -- unless the attract tour is
/// running, in which case the first key only dismisses the tour. Otherwise
/// walking past the machine and tapping a key would drop you straight into a
/// menu you never saw.
fn advance_title(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    director: Res<director::Director>,
    mut next: ResMut<NextState<AppState>>,
) {
    if any_key(&keys, &mouse) && !director.attracting() {
        next.set(AppState::Menu);
    }
}

fn drive_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<ui::Menu>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: EventWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) {
        menu.step(1);
    }
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
        menu.step(-1);
    }
    if keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Title);
        return;
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        match menu.selected() {
            ui::MenuItem::Play => next.set(AppState::DropIn),
            // The squad screen and the options screen are the seam where the
            // character-creation flow attaches. Until it lands, selecting these
            // leaves the camera parked on its anchor, which is a viewing angle
            // rather than a dead end.
            ui::MenuItem::Team | ui::MenuItem::Settings => {}
            ui::MenuItem::Quit => {
                exit.send(AppExit);
            }
        }
    }
}

fn finish_drop_in(director: Res<director::Director>, mut next: ResMut<NextState<AppState>>) {
    if director.elapsed >= director::DROP_IN_SECONDS {
        next.set(AppState::Playing);
    }
}

/// Escape during a match returns to the menu and clears the scoreline, so the
/// next kickoff is a fresh game rather than a resumed one.
fn leave_match(
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<AppState>>,
    mut match_state: ResMut<NextState<crate::game::MatchState>>,
    mut game: ResMut<crate::game::GameState>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        game.reset();
        match_state.set(crate::game::MatchState::Playing);
        next.set(AppState::Menu);
    }
}

/// Clears the previous scoreline as the camera starts its dive, so the board
/// has already rolled over to 0-0 by the time it is legible.
fn start_fresh_match(
    mut game: ResMut<crate::game::GameState>,
    mut match_state: ResMut<NextState<crate::game::MatchState>>,
) {
    game.reset();
    match_state.set(crate::game::MatchState::Playing);
}

/// Set CANOPY_STATE to title, menu or play to skip the states before it.
///
/// Iterating on the menu otherwise means sitting through the whole cold open on
/// every run, and the screenshot capture in `main` has no way to press a key.
fn starting_state() -> AppState {
    match std::env::var("CANOPY_STATE").unwrap_or_default().as_str() {
        "title" => AppState::Title,
        "menu" => AppState::Menu,
        "play" => AppState::Playing,
        _ => AppState::ColdOpen,
    }
}

pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.insert_state(starting_state())
            .init_resource::<director::Director>()
            .init_resource::<ui::Menu>()
            .init_resource::<atmosphere::CurrentMood>()
            .add_systems(Startup, atmosphere::setup_overlays)
            // Every state change restarts the shot clock, so each state can
            // time its own animation from zero.
            .add_systems(OnEnter(AppState::ColdOpen), director::reset_shot_clock)
            .add_systems(
                OnEnter(AppState::Title),
                (director::reset_shot_clock, ui::spawn_title),
            )
            .add_systems(OnExit(AppState::Title), ui::despawn_front_end_ui)
            .add_systems(
                OnEnter(AppState::Menu),
                (director::reset_shot_clock, ui::spawn_menu),
            )
            .add_systems(OnExit(AppState::Menu), ui::despawn_front_end_ui)
            .add_systems(
                OnEnter(AppState::DropIn),
                (
                    director::reset_shot_clock,
                    director::begin_drop_in,
                    start_fresh_match,
                ),
            )
            .add_systems(OnEnter(AppState::Playing), director::hand_over_to_match)
            .add_systems(
                Update,
                (
                    note_activity,
                    advance_cold_open.run_if(in_state(AppState::ColdOpen)),
                    advance_title.run_if(in_state(AppState::Title)),
                    (drive_menu, ui::highlight_menu_row, ui::update_blurb)
                        .chain()
                        .run_if(in_state(AppState::Menu)),
                    finish_drop_in.run_if(in_state(AppState::DropIn)),
                    leave_match.run_if(in_state(AppState::Playing)),
                    (ui::animate_title, ui::pulse_prompt).run_if(in_state(AppState::Title)),
                    ui::fade_out_ui,
                    atmosphere::blend_atmosphere,
                ),
            )
            // Shares a slot with the match camera, which is scheduled for
            // AppState::Playing only. Runs before transform propagation for the
            // same reason: a camera written after propagation is a frame late.
            .add_systems(
                PostUpdate,
                director::drive_camera
                    .run_if(in_front_end)
                    .before(bevy::transform::TransformSystem::TransformPropagate),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn modifier_keys_do_not_skip_the_opening() {
        // Alt-tabbing back into the window delivers a modifier press; treating
        // it as "press any key" would skip the cold open on every refocus.
        let mut keys = ButtonInput::<KeyCode>::default();
        let mouse = ButtonInput::<MouseButton>::default();
        keys.press(KeyCode::AltLeft);
        assert!(!any_key(&keys, &mouse));
        keys.press(KeyCode::KeyJ);
        assert!(any_key(&keys, &mouse));
    }

    #[test]
    fn the_front_end_owns_the_camera_in_every_state_but_play() {
        // in_front_end and playing gate two systems that write the same
        // component; if they ever overlap the camera stutters between them.
        for state in [
            AppState::ColdOpen,
            AppState::Title,
            AppState::Menu,
            AppState::DropIn,
            AppState::Playing,
        ] {
            let mut app = App::new();
            app.insert_state(state);
            let front = app.world.run_system_once(in_front_end);
            let play = app.world.run_system_once(playing);
            assert_ne!(front, play, "{state:?} is owned by both or neither");
        }
    }
}
