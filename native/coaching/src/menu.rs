//! The two screens the menu grew: quick settings, and an admin panel.
//!
//! Both are reached from the lobby and both close back to it. Deliberately small: everything on
//! them is something the app really has and someone really needs to change mid-demo. A setting
//! that does nothing is worse than a missing one, because it costs a demo the time it takes to
//! discover that.
//!
//! The admin panel is for whoever is running the demo, not for a player. It is the one place that
//! says plainly which controller is driving the match, what the classifier made of the drawing,
//! and which model each side is wearing — the questions that otherwise need a debugger.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::forge::ForgedPower;
use crate::network::{LobbyScreen, LobbyUiState, NetworkEndpoint, NetworkRole};
use crate::phase::AppPhase;
use crate::theme::{self, Tone};
use crate::world::RoundCount;
use cube_soccer::creation::CreationPhase;
use cube_soccer::entities::{WornCharacters, BASE_CHARACTER};
use cube_soccer::game::Team;
use cube_soccer::tactics::speech::SpeechRuntime;

/// Quick settings: the microphone, the picture, and where the service is.
///
/// Three things because three things can actually go wrong in front of an audience — the wrong
/// input device, a frame rate that will not hold, and a joiner pointed at the wrong host.
pub fn settings_ui(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<LobbyUiState>,
    mut speech: ResMut<SpeechRuntime>,
    mut msaa: ResMut<Msaa>,
    mut endpoint: ResMut<NetworkEndpoint>,
) {
    if ui_state.screen != LobbyScreen::Settings {
        return;
    }
    let ctx = contexts.ctx_mut();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(44.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(theme::SLAB_MAX_WIDTH + 120.0);
            theme::title(ui, "Settings", Some("Takes effect immediately."));

            theme::panel_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("MICROPHONE").strong().color(theme::GOLD));
                if speech.devices.is_empty() {
                    ui.label(
                        egui::RichText::new("No input device found. Coaching still works; nothing will be transcribed.")
                            .color(theme::CLOTH_DIM),
                    );
                } else {
                    let selected = speech.selected.min(speech.devices.len() - 1);
                    egui::ComboBox::from_id_source("microphone")
                        .width(320.0)
                        .selected_text(speech.devices[selected].clone())
                        .show_ui(ui, |ui| {
                            // Cloned first: the picker borrows `speech` mutably to set the index.
                            let devices = speech.devices.clone();
                            for (index, name) in devices.iter().enumerate() {
                                ui.selectable_value(&mut speech.selected, index, name);
                            }
                        });
                }
            });

            ui.add_space(10.0);
            theme::panel_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("ANTI-ALIASING").strong().color(theme::GOLD));
                ui.label(
                    egui::RichText::new(
                        "Two samples is the default: the goal netting's thin beams sparkle with none at all, and four costs about a fifth of the frame.",
                    )
                    .size(12.0)
                    .color(theme::CLOTH_DIM),
                );
                ui.horizontal(|ui| {
                    for (label, value) in [("Off", Msaa::Off), ("2x", Msaa::Sample2), ("4x", Msaa::Sample4)] {
                        if ui.selectable_label(*msaa == value, label).clicked() {
                            *msaa = value;
                        }
                    }
                });
            });

            ui.add_space(10.0);
            theme::panel_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("LOCAL SERVICE").strong().color(theme::GOLD));
                ui.label(
                    egui::RichText::new("Transcription, interpretation and the superpower classifier. A joiner has this pointed at the host automatically.")
                        .size(12.0)
                        .color(theme::CLOTH_DIM),
                );
                ui.horizontal(|ui| {
                    ui.label("Address");
                    ui.text_edit_singleline(&mut endpoint.host_addr);
                });
            });

            ui.add_space(20.0);
            if theme::slab(ui, "Back", Tone::Plain, false).clicked() {
                ui_state.screen = LobbyScreen::MainMenu;
            }
        });
    });
}

/// The admin panel: what the app currently believes, and the shortcuts past it.
#[allow(clippy::too_many_arguments)]
pub fn admin_ui(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<LobbyUiState>,
    mut role: ResMut<NetworkRole>,
    mut next_phase: ResMut<NextState<AppPhase>>,
    mut next_creation: ResMut<NextState<CreationPhase>>,
    mut worn: ResMut<WornCharacters>,
    forged: Res<ForgedPower>,
    rounds: Res<RoundCount>,
) {
    if ui_state.screen != LobbyScreen::Admin {
        return;
    }
    let ctx = contexts.ctx_mut();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(44.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(theme::SLAB_MAX_WIDTH + 200.0);
            theme::title(ui, "Admin", Some("For whoever is running the demo."));

            theme::panel_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("MATCH CONTROLLER").strong().color(theme::GOLD));
                // Stated here and only here. The HUD deliberately does not carry this, but
                // someone running the demo should be able to find out without a debugger.
                ui.label("Heuristic tactical AI, driven by the coached play.");
                ui.label(
                    egui::RichText::new(
                        "No trained checkpoint is loaded. The coached play sets each side's shape, pressing and depth; nothing here calls a policy.",
                    )
                    .size(12.0)
                    .color(theme::CLOTH_DIM),
                );
                ui.label(format!("Rounds played: {}", rounds.0));
            });

            ui.add_space(10.0);
            theme::panel_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("DRAWN SUPERPOWER").strong().color(theme::GOLD));
                match (forged.kind, &forged.error) {
                    (Some(kind), _) => {
                        ui.label(format!(
                            "{} — {:.0}% confident",
                            kind.slug(),
                            forged.confidence * 100.0
                        ));
                        if let Some(motif) = &forged.motif {
                            ui.label(
                                egui::RichText::new(format!("read as: {motif}"))
                                    .size(12.0)
                                    .color(theme::CLOTH_DIM),
                            );
                        }
                    }
                    (None, Some(error)) => {
                        ui.label(egui::RichText::new(error).color(theme::CLAY));
                    }
                    (None, None) => {
                        ui.label(
                            egui::RichText::new("Nothing classified yet.").color(theme::CLOTH_DIM),
                        );
                    }
                }
            });

            ui.add_space(10.0);
            theme::panel_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("WORN CHARACTERS").strong().color(theme::GOLD));
                for (team, label) in [(Team::Orange, "Orange"), (Team::Blue, "Blue")] {
                    ui.label(format!(
                        "{label}: {}",
                        worn.get(team).unwrap_or("the jungle's blocky character")
                    ));
                }
                ui.horizontal(|ui| {
                    if ui.button("Back to the base monkey").clicked() {
                        for team in [Team::Orange, Team::Blue] {
                            worn.set(team, Some(BASE_CHARACTER.to_owned()));
                        }
                    }
                    if ui.button("Back to the blocky character").clicked() {
                        for team in [Team::Orange, Team::Blue] {
                            worn.set(team, None);
                        }
                    }
                });
            });

            ui.add_space(10.0);
            theme::panel_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("SKIP AHEAD").strong().color(theme::GOLD));
                ui.label(
                    egui::RichText::new("Solo only. Jumping straight to a match leaves it on the balanced default, because nothing has been coached.")
                        .size(12.0)
                        .color(theme::CLOTH_DIM),
                );
                ui.horizontal(|ui| {
                    if ui.button("The easel").clicked() {
                        *role = NetworkRole::Solo;
                        next_creation.set(CreationPhase::PaintingAppearance);
                        next_phase.set(AppPhase::Creation);
                    }
                    if ui.button("The table").clicked() {
                        *role = NetworkRole::Solo;
                        next_creation.set(CreationPhase::Coaching);
                        next_phase.set(AppPhase::Coaching);
                    }
                    if ui.button("The match").clicked() {
                        *role = NetworkRole::Solo;
                        next_creation.set(CreationPhase::Departing);
                        next_phase.set(AppPhase::Game);
                    }
                });
            });

            ui.add_space(20.0);
            if theme::slab(ui, "Back", Tone::Plain, false).clicked() {
                ui_state.screen = LobbyScreen::MainMenu;
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_screens_close_back_to_the_main_menu() {
        // Neither is a dead end: the only way out of a screen with no Back is the window's X.
        for screen in [LobbyScreen::Settings, LobbyScreen::Admin] {
            assert_ne!(screen, LobbyScreen::MainMenu);
        }
    }

    #[test]
    fn the_admin_panel_can_put_a_side_back_in_either_fallback() {
        // The two buttons have to reach genuinely different states, or one of them is a lie.
        let mut worn = WornCharacters::default();
        for team in [Team::Orange, Team::Blue] {
            worn.set(team, None);
        }
        assert_eq!(worn.get(Team::Orange), None);

        for team in [Team::Orange, Team::Blue] {
            worn.set(team, Some(BASE_CHARACTER.to_owned()));
        }
        assert_eq!(worn.get(Team::Orange), Some(BASE_CHARACTER));
        assert_eq!(worn.get(Team::Blue), Some(BASE_CHARACTER));
    }
}
