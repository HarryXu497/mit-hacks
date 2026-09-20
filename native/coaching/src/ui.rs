use crate::board::{append_undo, BoardInteraction, BoardViewport, BOARD_ASPECT};
use crate::interpretation::{InterpretationState, RequestInterpretation, TacticalResult};
use crate::game_handoff::TeamSide;
use crate::network::{MatchReady, NetworkRole};
use crate::model::{create_id, RawSessionEvent, SessionStatus, Tool, TranscriptSource};
use crate::persistence::{export_tactical_json, PersistenceStatus};
use crate::replay::{find_undo_target, replay_session};
use crate::session::CoachingSession;
use cube_soccer::tactics::Transcript as TableTranscript;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

// The panels are the jungle's own timber, not a cold slab floating in front of
// it: face, bars, outline, and text all come from `theme`'s palette so the flat
// screens read as carved wood standing in the same world as the easel behind them.
const PANEL: egui::Color32 = crate::theme::PLANK;
const BAR: egui::Color32 = crate::theme::PLANK_DARK;
const BORDER: egui::Color32 = crate::theme::INK;
const TEXT: egui::Color32 = crate::theme::CLOTH;
const MUTED: egui::Color32 = crate::theme::CLOTH_DIM;
const GOLD: egui::Color32 = crate::theme::GOLD;
const RED: egui::Color32 = crate::theme::CLAY;
const WARN: egui::Color32 = egui::Color32::from_rgb(235, 170, 120);

#[derive(Resource, Default)]
pub struct CoachingUiState {
    manual_transcript: String,
    confirm_reset: bool,
}

/// Dress every flat screen in the jungle's colours, once at startup.
///
/// The blue-grey palette this used to set belonged to a 2D board floating over a blank window.
/// There is a jungle behind these panels now, so they take their colours from it -- see `theme`,
/// which also owns the slab the menus are built from.
pub fn configure_egui(mut contexts: EguiContexts) {
    crate::theme::apply(contexts.ctx_mut());
}

#[allow(clippy::too_many_arguments)]
pub fn coaching_ui(
    mut contexts: EguiContexts,
    mut session: ResMut<CoachingSession>,
    mut interaction: ResMut<BoardInteraction>,
    mut viewport: ResMut<BoardViewport>,
    table_speech: Res<TableTranscript>,
    mut result: ResMut<TacticalResult>,
    persistence: Res<PersistenceStatus>,
    mut ui_state: ResMut<CoachingUiState>,
    mut interpretation_requests: EventWriter<RequestInterpretation>,
    mut match_ready: EventWriter<MatchReady>,
    role: Res<NetworkRole>,
    mut table_keyboard: ResMut<cube_soccer::tactics::KeyboardCaptured>,
) {
    let context = contexts.ctx_mut();

    // Silence the table's own keyboard shortcuts whenever a panel field has
    // focus, so typing a note (Enter especially) does not also fire them.
    table_keyboard.0 = context.wants_keyboard_input();

    top_bar(context, &mut session, &mut ui_state);
    timeline_panel(context, &mut session);
    transcript_panel(
        context,
        &mut session,
        &table_speech,
        &mut result,
        &persistence,
        &mut ui_state,
        &mut interpretation_requests,
        &mut match_ready,
        role.coached_side(),
    );
    board_panel(context, &mut session, &mut interaction, &mut viewport);

    if ui_state.confirm_reset {
        reset_dialog(context, &mut session, &mut ui_state);
    }
}

fn top_bar(
    context: &egui::Context,
    session: &mut CoachingSession,
    ui_state: &mut CoachingUiState,
) {
    egui::TopBottomPanel::top("top-bar")
        .exact_height(58.0)
        .frame(panel_frame(BAR))
        .show(context, |ui| {
            crate::theme::wood_grain(ui.painter(), ui.max_rect().expand2(egui::vec2(12.0, 7.0)));
            ui.horizontal_centered(|ui| {
                ui.add_space(12.0);
                crate::theme::panel_title(ui, "Tactic Lab", 24.0);
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
                            session.stop();
                        } else {
                            session.start_or_resume();
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
                // A wooden tool tray, not a floating grey pill: same timber and ink
                // as the panels so the board's controls belong to the same app.
                egui::Frame::none()
                    .fill(PANEL)
                    .stroke(egui::Stroke::new(2.0_f32, BORDER))
                    .rounding(3.0)
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .show(ui, |ui| {
                        crate::theme::wood_grain(
                            ui.painter(),
                            ui.max_rect().expand2(egui::vec2(8.0, 6.0)),
                        );
                        ui.visuals_mut().selection.bg_fill = GOLD;
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
    // Ink on gold when chosen — the leading-edge look the app uses for a
    // selection — and cloth on timber otherwise.
    let text = if selected {
        egui::RichText::new(label).color(crate::theme::INK).strong()
    } else {
        egui::RichText::new(label)
    };
    let button = egui::Button::new(text)
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
    table_speech: &TableTranscript,
    result: &mut TacticalResult,
    persistence: &PersistenceStatus,
    ui_state: &mut CoachingUiState,
    requests: &mut EventWriter<RequestInterpretation>,
    match_ready: &mut EventWriter<MatchReady>,
    coached_side: TeamSide,
) {
    egui::SidePanel::right("transcript-panel")
        .resizable(true)
        .default_width(390.0)
        .width_range(330.0..=500.0)
        .frame(panel_frame(PANEL))
        .show(context, |ui| {
            crate::theme::wood_grain(ui.painter(), ui.max_rect().expand2(egui::vec2(12.0, 7.0)));
            ui.horizontal(|ui| {
                crate::theme::panel_title(
                    ui,
                    if result.output.is_some() {
                        "Tactical JSON"
                    } else {
                        "Live transcript"
                    },
                    22.0,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The table's microphone reports itself, in its own words — and some of
                    // those words are long device errors that used to overrun the heading.
                    // The header only badges the state; the full text shows in the body.
                    let (label, healthy) = mic_status(&table_speech.status, table_speech.live);
                    let color = if healthy { MUTED } else { WARN };
                    ui.label(egui::RichText::new(label).small().color(color));
                });
            });
            ui.separator();

            if result.output.is_some() {
                result_view(ui, session, result, match_ready, coached_side);
            } else {
                transcript_view(ui, session, table_speech, persistence, ui_state, requests, result);
            }
        });
}

fn transcript_view(
    ui: &mut egui::Ui,
    session: &mut CoachingSession,
    table_speech: &TableTranscript,
    persistence: &PersistenceStatus,
    ui_state: &mut CoachingUiState,
    requests: &mut EventWriter<RequestInterpretation>,
    result: &TacticalResult,
) {
    // A microphone or device problem, in full: the header only had room to badge
    // it. Strip the "Speech error: " prefix when present so it reads as plain prose.
    if !mic_status(&table_speech.status, table_speech.live).1 {
        let detail = table_speech
            .status
            .strip_prefix("Speech error: ")
            .unwrap_or(&table_speech.status);
        ui.colored_label(WARN, detail.trim());
    }
    if let Some(error) = &persistence.error {
        ui.colored_label(WARN, error);
    }

    let replay = replay_session(&session.session.events, None);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((ui.available_height() - if result.state == InterpretationState::Failed { 230.0 } else { 185.0 }).max(80.0))
        .show(ui, |ui| {
            // DEMO TODO: simplify these diagnostics after integration is stable;
            // retain an explicit failure state and retry, never silently use Balanced.
            if result.state == InterpretationState::Failed {
                ui.colored_label(egui::Color32::from_rgb(245, 130, 120), "Interpretation failed — no tactic was applied.");
                if let Some(notice) = &result.notice {
                    ui.label(notice);
                }
                ui.label("Your recording is preserved. Retry, or explicitly continue with Balanced.");
                ui.separator();
            }
            if replay.transcripts.is_empty() && table_speech.partial.is_empty() {
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
            if !table_speech.partial.is_empty() {
                ui.label(
                    egui::RichText::new(&table_speech.partial)
                        .italics()
                        .color(egui::Color32::from_rgb(170, 190, 215)),
                );
            }
        });

    ui.separator();
    crate::theme::section_label(ui, "Manual transcript");
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
        if submit && !ui_state.manual_transcript.trim().is_empty() {
            let text = std::mem::take(&mut ui_state.manual_transcript);
            session.add_transcript(text, TranscriptSource::Manual, None, None);
        }
    });
    ui.add_space(8.0);
    // Anything recorded or noted can be interpreted — you no longer have to
    // stop recording first. If a recording is still running, tapping Generate
    // finalises it (below) before the request goes out.
    let has_content = !session.session.events.is_empty();
    let can_generate = has_content && result.state != InterpretationState::Generating;
    let label = if result.state == InterpretationState::Generating {
        "Generating…"
    } else if result.state == InterpretationState::Failed {
        "Retry interpretation"
    } else {
        "Generate JSON"
    };
    let generate_size = egui::vec2(ui.available_width(), 42.0);
    if ui
        .add_enabled_ui(can_generate, |ui| {
            ui.add_sized(
                generate_size,
                egui::Button::new(egui::RichText::new(label).strong().color(crate::theme::INK))
                    .fill(GOLD),
            )
        })
        .inner
        .clicked()
    {
        // Finalise a live recording so the interpreter, which ignores requests
        // while recording, accepts this one on the next tick.
        if session.session.status == SessionStatus::Recording {
            session.stop();
        }
        requests.send(RequestInterpretation::Generate);
    }
    if result.state == InterpretationState::Failed {
        ui.add_space(4.0);
        if ui
            .add_sized(
                egui::vec2(ui.available_width(), 34.0),
                egui::Button::new("Continue anyway (Balanced)"),
            )
            .clicked()
        {
            requests.send(RequestInterpretation::ContinueBalanced);
        }
    }

}

fn result_view(
    ui: &mut egui::Ui,
    session: &mut CoachingSession,
    result: &mut TacticalResult,
    match_ready: &mut EventWriter<MatchReady>,
    coached_side: TeamSide,
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
    // Must use the team THIS machine coached: hardcoding red left the
    // joiner's own results screen with no summary at all.
    if let Ok(team) =
        crate::game_handoff::CoachedTeam::from_output_for_team(&output, coached_side)
    {
        ui.label(team.summary_line());
    }
    let pretty = serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".into());
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
        match_ready.send(MatchReady);
    }
    ui.add_space(8.0);
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
}

fn timeline_panel(context: &egui::Context, session: &mut CoachingSession) {
    egui::TopBottomPanel::bottom("timeline")
        .exact_height(150.0)
        .frame(panel_frame(BAR))
        .show(context, |ui| {
            crate::theme::wood_grain(ui.painter(), ui.max_rect().expand2(egui::vec2(12.0, 7.0)));
            ui.horizontal(|ui| {
                crate::theme::panel_title(ui, "Session timeline", 17.0);
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
                    .color(GOLD),
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
                    ui_state.manual_transcript.clear();
                    ui_state.confirm_reset = false;
                }
            });
        });
}

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(2.0_f32, BORDER))
        .inner_margin(egui::Margin::symmetric(14.0, 9.0))
}

/// A short badge for the microphone's self-reported status, plus whether it is
/// healthy. Device errors are long sentences; in the heading they overran the
/// title, so anything that is not a known idle/listening state is collapsed to
/// "Mic unavailable" and shown in full in the transcript body instead.
fn mic_status(status: &str, live: bool) -> (String, bool) {
    let line = status.lines().next().unwrap_or("").trim();
    let healthy = matches!(
        line.to_ascii_lowercase().as_str(),
        "" | "listening" | "idle" | "ready" | "recording" | "stopped"
    );
    if line.is_empty() {
        (if live { "Listening" } else { "Idle" }.to_owned(), true)
    } else if healthy {
        (line.to_owned(), true)
    } else {
        ("Mic unavailable".to_owned(), false)
    }
}

fn format_time(milliseconds: u64) -> String {
    let total_seconds = milliseconds / 1_000;
    format!("{:02}:{:02}", total_seconds / 60, total_seconds % 60)
}
