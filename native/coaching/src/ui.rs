use crate::board::{append_undo, BoardInteraction, BoardViewport, BOARD_ASPECT};
use crate::interpretation::{InterpretationState, RequestInterpretation, TacticalResult};
use crate::EnterGame;
use crate::model::{create_id, RawSessionEvent, SessionStatus, Tool, TranscriptSource};
use crate::persistence::{export_tactical_json, PersistenceStatus};
use crate::replay::{find_undo_target, replay_session};
use crate::session::CoachingSession;
use crate::speech::{SpeechRuntime, SpeechStatus};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

const PANEL: egui::Color32 = egui::Color32::from_rgb(18, 27, 38);
const BORDER: egui::Color32 = egui::Color32::from_rgb(43, 57, 73);
const TEXT: egui::Color32 = egui::Color32::from_rgb(237, 243, 251);
const MUTED: egui::Color32 = egui::Color32::from_rgb(157, 171, 188);
const BLUE: egui::Color32 = egui::Color32::from_rgb(46, 145, 255);
const RED: egui::Color32 = egui::Color32::from_rgb(239, 71, 73);

#[derive(Resource, Default)]
pub struct CoachingUiState {
    manual_transcript: String,
    confirm_reset: bool,
}

pub fn configure_egui(mut contexts: EguiContexts) {
    let context = contexts.ctx_mut();
    let mut style = (*context.style()).clone();
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(21, 31, 43);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(27, 39, 53);
    style.visuals.selection.bg_fill = BLUE.linear_multiply(0.35);
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, BLUE);
    style.spacing.item_spacing = egui::vec2(9.0, 8.0);
    context.set_style(style);
}

#[allow(clippy::too_many_arguments)]
pub fn coaching_ui(
    mut contexts: EguiContexts,
    mut session: ResMut<CoachingSession>,
    mut interaction: ResMut<BoardInteraction>,
    mut viewport: ResMut<BoardViewport>,
    mut speech: ResMut<SpeechRuntime>,
    mut result: ResMut<TacticalResult>,
    persistence: Res<PersistenceStatus>,
    mut ui_state: ResMut<CoachingUiState>,
    mut interpretation_requests: EventWriter<RequestInterpretation>,
    mut enter_game: EventWriter<EnterGame>,
) {
    let context = contexts.ctx_mut();

    top_bar(context, &mut session, &mut speech, &mut ui_state);
    timeline_panel(context, &mut session);
    transcript_panel(
        context,
        &mut session,
        &mut speech,
        &mut result,
        &persistence,
        &mut ui_state,
        &mut interpretation_requests,
        &mut enter_game,
    );
    board_panel(context, &mut session, &mut interaction, &mut viewport);

    if ui_state.confirm_reset {
        reset_dialog(context, &mut session, &mut speech, &mut ui_state);
    }
}

fn top_bar(
    context: &egui::Context,
    session: &mut CoachingSession,
    speech: &mut SpeechRuntime,
    ui_state: &mut CoachingUiState,
) {
    egui::TopBottomPanel::top("top-bar")
        .exact_height(58.0)
        .frame(panel_frame(egui::Color32::from_rgb(17, 26, 37)))
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(12.0);
                ui.heading(egui::RichText::new("Tactic Lab").size(22.0).strong());
                ui.separator();
                let title_response = ui.add(
                    egui::TextEdit::singleline(&mut session.session.title)
                        .desired_width(235.0)
                        .frame(false),
                );
                if title_response.changed() {
                    session.mark_edited();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    if ui.button("Reset").clicked() {
                        ui_state.confirm_reset = true;
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format_time(session.elapsed_now()))
                            .monospace()
                            .color(TEXT),
                    );
                    let (label, color) = if session.session.status == SessionStatus::Recording {
                        ("Stop recording", RED)
                    } else if session.session.status == SessionStatus::Ready {
                        ("Start recording", egui::Color32::from_rgb(8, 123, 73))
                    } else {
                        ("Resume", egui::Color32::from_rgb(8, 123, 73))
                    };
                    let button = egui::Button::new(egui::RichText::new(label).strong())
                        .fill(color)
                        .min_size(egui::vec2(142.0, 40.0));
                    if ui.add(button).clicked() {
                        if session.session.status == SessionStatus::Recording {
                            speech.stop();
                            session.stop();
                        } else {
                            session.start_or_resume();
                            speech.start(session);
                        }
                    }
                });
            });
        });
}

fn board_panel(
    context: &egui::Context,
    session: &mut CoachingSession,
    interaction: &mut BoardInteraction,
    viewport: &mut BoardViewport,
) {
    egui::CentralPanel::default()
        // The board is drawn by the Bevy camera underneath egui, so this panel
        // must stay transparent for the pitch to show through.
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(egui::Margin::same(16.0)),
        )
        .show(context, |ui| {
            let available = ui.available_rect_before_wrap();
            let toolbar_height = 58.0;
            let board_height = (available.height() - toolbar_height - 8.0).max(120.0);
            let board_width = (board_height * BOARD_ASPECT).min(available.width().max(120.0));
            let board_height = (board_width / BOARD_ASPECT).min(board_height);
            let center = egui::pos2(available.center().x, available.min.y + board_height * 0.5);
            let field = egui::Rect::from_center_size(center, egui::vec2(board_width, board_height))
                .intersect(ui.max_rect());
            viewport.rect = field;
            viewport.visible = field.width() > 10.0 && field.height() > 10.0;

            let toolbar_rect = egui::Rect::from_min_size(
                egui::pos2(
                    field.center().x - (board_width.min(500.0) * 0.5),
                    field.max.y + 5.0,
                ),
                egui::vec2(board_width.min(500.0), toolbar_height),
            );
            ui.allocate_ui_at_rect(toolbar_rect, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(207, 214, 217))
                    .rounding(7.0)
                    .inner_margin(egui::Margin::symmetric(7.0, 6.0))
                    .show(ui, |ui| {
                        let visuals = ui.visuals_mut();
                        visuals.override_text_color = Some(egui::Color32::from_rgb(39, 49, 61));
                        let idle = egui::Color32::from_rgb(232, 236, 238);
                        let hovered = egui::Color32::from_rgb(216, 223, 227);
                        visuals.widgets.inactive.weak_bg_fill = idle;
                        visuals.widgets.inactive.bg_fill = idle;
                        visuals.widgets.hovered.weak_bg_fill = hovered;
                        visuals.widgets.hovered.bg_fill = hovered;
                        visuals.widgets.active.weak_bg_fill = hovered;
                        visuals.widgets.active.bg_fill = hovered;
                        visuals.selection.bg_fill = BLUE.linear_multiply(0.55);
                        ui.horizontal_centered(|ui| {
                            tool_button(ui, interaction, Tool::Select, "Select");
                            tool_button(ui, interaction, Tool::Arrow, "Arrow");
                            tool_button(ui, interaction, Tool::Draw, "Draw");
                            tool_button(ui, interaction, Tool::Erase, "Erase");
                            ui.separator();
                            let enabled = session.session.status == SessionStatus::Recording
                                && find_undo_target(&session.session.events).is_some();
                            if ui.add_enabled(enabled, egui::Button::new("Undo")).clicked() {
                                append_undo(session);
                            }
                        });
                    });
            });
        });
}

fn tool_button(ui: &mut egui::Ui, interaction: &mut BoardInteraction, tool: Tool, label: &str) {
    let selected = interaction.tool == tool;
    let button = egui::Button::new(label)
        .selected(selected)
        .min_size(egui::vec2(74.0, 38.0));
    if ui.add(button).clicked() {
        interaction.tool = tool;
    }
}

#[allow(clippy::too_many_arguments)]
fn transcript_panel(
    context: &egui::Context,
    session: &mut CoachingSession,
    speech: &mut SpeechRuntime,
    result: &mut TacticalResult,
    persistence: &PersistenceStatus,
    ui_state: &mut CoachingUiState,
    requests: &mut EventWriter<RequestInterpretation>,
    enter_game: &mut EventWriter<EnterGame>,
) {
    egui::SidePanel::right("transcript-panel")
        .resizable(true)
        .default_width(390.0)
        .width_range(330.0..=500.0)
        .frame(panel_frame(PANEL))
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading(if result.output.is_some() {
                    "Tactical JSON"
                } else {
                    "Live transcript"
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let status = match speech.status {
                        SpeechStatus::Listening => "Listening…",
                        SpeechStatus::Connecting => "Connecting…",
                        SpeechStatus::Finalizing => "Finalizing…",
                        SpeechStatus::Error => "Speech unavailable",
                        SpeechStatus::Idle => "Idle",
                    };
                    ui.label(egui::RichText::new(status).small().color(MUTED));
                });
            });
            ui.separator();

            if result.output.is_some() {
                result_view(ui, session, result, enter_game);
            } else {
                transcript_view(ui, session, speech, persistence, ui_state, requests, result);
            }
        });
}

fn transcript_view(
    ui: &mut egui::Ui,
    session: &mut CoachingSession,
    speech: &mut SpeechRuntime,
    persistence: &PersistenceStatus,
    ui_state: &mut CoachingUiState,
    requests: &mut EventWriter<RequestInterpretation>,
    result: &TacticalResult,
) {
    if !speech.devices.is_empty() {
        egui::ComboBox::from_label("Microphone")
            .selected_text(
                speech
                    .devices
                    .get(speech.selected_device)
                    .map(String::as_str)
                    .unwrap_or("Default"),
            )
            .show_ui(ui, |ui| {
                for (index, device) in speech.devices.iter().enumerate() {
                    ui.selectable_value(&mut speech.selected_device, index, device);
                }
            });
    } else {
        ui.colored_label(
            egui::Color32::from_rgb(220, 170, 90),
            "No microphone detected",
        );
    }
    if let Some(error) = &speech.error {
        ui.colored_label(egui::Color32::from_rgb(235, 170, 120), error);
    }
    if let Some(error) = &persistence.error {
        ui.colored_label(egui::Color32::from_rgb(235, 170, 120), error);
    }

    let replay = replay_session(&session.session.events, None);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(ui.available_height() - 185.0)
        .show(ui, |ui| {
            if replay.transcripts.is_empty() && speech.partial_text.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(70.0);
                    ui.label(
                        egui::RichText::new("Speech and manual notes will appear here.")
                            .color(MUTED),
                    );
                });
            }
            for segment in replay.transcripts {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format_time(segment.start_ms))
                            .monospace()
                            .small()
                            .color(MUTED),
                    );
                    let mut text = segment.text.clone();
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut text)
                            .desired_width(f32::INFINITY)
                            .frame(false),
                    );
                    if response.changed() {
                        session.append(RawSessionEvent::TranscriptEdited {
                            id: create_id("event"),
                            timestamp_ms: session.elapsed_now(),
                            segment_id: segment.id,
                            text,
                        });
                    }
                });
                ui.separator();
            }
            if !speech.partial_text.is_empty() {
                ui.label(
                    egui::RichText::new(&speech.partial_text)
                        .italics()
                        .color(egui::Color32::from_rgb(170, 190, 215)),
                );
            }
        });

    ui.separator();
    ui.label(
        egui::RichText::new("MANUAL TRANSCRIPT")
            .small()
            .color(MUTED),
    );
    ui.horizontal(|ui| {
        let add_button_width = 58.0;
        let field_width =
            (ui.available_width() - add_button_width - ui.spacing().item_spacing.x).max(80.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut ui_state.manual_transcript)
                .desired_width(field_width)
                .hint_text("Add coaching note"),
        );
        let submit = ui.button("Add").clicked()
            || (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
        if submit && session.session.status == SessionStatus::Recording {
            let text = std::mem::take(&mut ui_state.manual_transcript);
            session.add_transcript(text, TranscriptSource::Manual, None, None);
        }
    });
    ui.add_space(8.0);
    let can_generate = matches!(
        session.session.status,
        SessionStatus::Review | SessionStatus::Interpreted
    ) && !speech.is_finalizing()
        && result.state != InterpretationState::Generating;
    let label = if result.state == InterpretationState::Generating {
        "Generating…"
    } else if speech.is_finalizing() {
        "Finalizing transcript…"
    } else {
        "Generate JSON"
    };
    let generate_size = egui::vec2(ui.available_width(), 42.0);
    if ui
        .add_enabled_ui(can_generate, |ui| {
            ui.add_sized(
                generate_size,
                egui::Button::new(egui::RichText::new(label).strong()).fill(BLUE),
            )
        })
        .inner
        .clicked()
    {
        requests.send(RequestInterpretation);
    }
}

fn result_view(
    ui: &mut egui::Ui,
    session: &mut CoachingSession,
    result: &mut TacticalResult,
    enter_game: &mut EventWriter<EnterGame>,
) {
    let Some(output) = result.output.clone() else {
        return;
    };
    if ui.button("Back to transcript").clicked() {
        session.session.status = SessionStatus::Review;
        *result = TacticalResult::default();
        return;
    }
    if let Some(notice) = &result.notice {
        ui.colored_label(egui::Color32::from_rgb(220, 201, 141), notice);
    }
    let pretty = serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".into());
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut pretty.clone())
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .interactive(false),
            );
        });
    ui.horizontal(|ui| {
        if ui.button("Copy JSON").clicked() {
            ui.output_mut(|output| output.copied_text = pretty.clone());
        }
        if ui.button("Export").clicked() {
            let _ = export_tactical_json(&session.session, &output);
        }
    });
    if let Some(path) = &result.tactics_path {
        ui.label(
            egui::RichText::new(format!("Saved: {}", path.display()))
                .small()
                .color(MUTED),
        );
    }
    ui.add_space(8.0);
    if ui
        .add_sized(
            egui::vec2(ui.available_width(), 42.0),
            egui::Button::new(egui::RichText::new("Next").strong())
                .fill(egui::Color32::from_rgb(8, 123, 73)),
        )
        .clicked()
    {
        enter_game.send(EnterGame);
    }
}

fn timeline_panel(context: &egui::Context, session: &mut CoachingSession) {
    egui::TopBottomPanel::bottom("timeline")
        .exact_height(150.0)
        .frame(panel_frame(egui::Color32::from_rgb(19, 29, 40)))
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("Session timeline").size(15.0));
                if ui
                    .add_enabled(
                        session.session.status != SessionStatus::Recording
                            && session.session.elapsed_ms > 0,
                        egui::Button::new(if session.playing { "Pause" } else { "Play" }),
                    )
                    .clicked()
                {
                    session.toggle_playback();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format_time(session.playhead_ms))
                            .monospace()
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(8.0);
            let mut playhead = session.playhead_ms as f64;
            let max = session.session.elapsed_ms.max(1) as f64;
            let response = ui.add_enabled(
                session.session.status != SessionStatus::Recording,
                egui::Slider::new(&mut playhead, 0.0..=max)
                    .show_value(false)
                    .trailing_fill(true),
            );
            if response.changed() {
                session.seek(playhead.round() as u64);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Board events").small().color(MUTED));
                ui.label(
                    egui::RichText::new(format!(
                        "{}",
                        session
                            .session
                            .events
                            .iter()
                            .filter(|event| event.is_undoable_action())
                            .count()
                    ))
                    .small()
                    .color(BLUE),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new("Transcript segments")
                        .small()
                        .color(MUTED),
                );
                ui.label(
                    egui::RichText::new(
                        replay_session(&session.session.events, None)
                            .transcripts
                            .len()
                            .to_string(),
                    )
                    .small()
                    .color(RED),
                );
            });
        });
}

fn reset_dialog(
    context: &egui::Context,
    session: &mut CoachingSession,
    speech: &mut SpeechRuntime,
    ui_state: &mut CoachingUiState,
) {
    egui::Window::new("Reset session?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(context, |ui| {
            ui.label("This clears all recorded board and transcript events.");
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui_state.confirm_reset = false;
                }
                if ui.add(egui::Button::new("Reset").fill(RED)).clicked() {
                    session.reset();
                    speech.reset(session.generation);
                    ui_state.manual_transcript.clear();
                    ui_state.confirm_reset = false;
                }
            });
        });
}

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .inner_margin(egui::Margin::symmetric(14.0, 9.0))
}

fn format_time(milliseconds: u64) -> String {
    let total_seconds = milliseconds / 1_000;
    format!("{:02}:{:02}", total_seconds / 60, total_seconds % 60)
}
