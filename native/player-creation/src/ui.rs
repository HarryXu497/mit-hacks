//! egui presentation for the player-creation flow.
//!
//! Layout only. Drawing intent comes from `input.rs`, stroke behaviour from
//! `drawing.rs`, and writes from `persistence.rs`. Palette and panel treatment
//! match `native/coaching` so the two screens read as one product.

use crate::drawing::{BrushTool, Canvas};
use crate::input::{
    apply_pointer, apply_tool_command, keyboard_commands, CanvasCommand, ToolSettings,
    MAX_BRUSH_WIDTH, MIN_BRUSH_WIDTH, PALETTE,
};
use crate::persistence::CreationManifest;
use crate::state::{
    ContinueToCoaching, CreationFlow, DrawingSlot, PlayerCreationSession, PlayerId, Team,
};
use crate::StoreResource;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use std::collections::HashMap;

const APP: egui::Color32 = egui::Color32::from_rgb(16, 24, 34);
const PANEL: egui::Color32 = egui::Color32::from_rgb(18, 27, 38);
const BORDER: egui::Color32 = egui::Color32::from_rgb(43, 57, 73);
const TEXT: egui::Color32 = egui::Color32::from_rgb(237, 243, 251);
const MUTED: egui::Color32 = egui::Color32::from_rgb(157, 171, 188);
const BLUE: egui::Color32 = egui::Color32::from_rgb(46, 145, 255);
const CORAL: egui::Color32 = egui::Color32::from_rgb(239, 71, 73);
const GREEN: egui::Color32 = egui::Color32::from_rgb(46, 184, 114);
const TEAM_RED: egui::Color32 = egui::Color32::from_rgb(230, 46, 51);
const TEAM_YELLOW: egui::Color32 = egui::Color32::from_rgb(247, 196, 31);
/// Canvas ground. Drawings export with transparency; this is only the backdrop.
const CANVAS_GROUND: egui::Color32 = egui::Color32::from_rgb(28, 38, 51);

fn team_color(team: Team) -> egui::Color32 {
    match team {
        Team::Red => TEAM_RED,
        Team::Yellow => TEAM_YELLOW,
    }
}

/// Transient screen state: texture cache, confirmations, and the last
/// save outcome.
#[derive(Resource, Default)]
pub struct CreationUiState {
    /// One cached texture per (player, slot), so switching players cannot show
    /// another player's pixels.
    textures: HashMap<(u8, &'static str), egui::TextureHandle>,
    /// Keyed the same way; bumped whenever the canvas is re-rastered.
    uploaded_revision: HashMap<(u8, &'static str), u64>,
    pub error: Option<String>,
    pub saved_notice: Option<String>,
    pub confirm_reset_player: bool,
}

impl CreationUiState {
    fn invalidate(&mut self, player: PlayerId, slot: DrawingSlot) {
        self.uploaded_revision.remove(&(player.get(), slot.slug()));
    }
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
pub fn creation_ui(
    mut contexts: EguiContexts,
    mut session: ResMut<PlayerCreationSession>,
    mut tools: ResMut<ToolSettings>,
    mut ui_state: ResMut<CreationUiState>,
    flow: Res<State<CreationFlow>>,
    mut next_flow: ResMut<NextState<CreationFlow>>,
    store: Res<StoreResource>,
    mut continue_events: EventWriter<ContinueToCoaching>,
) {
    let context = contexts.ctx_mut();
    let current = *flow.get();

    let mut commands = keyboard_commands(context);
    commands.retain(|command| !apply_tool_command(&mut tools, *command));

    top_bar(context, &session, current);
    roster_panel(context, &mut session, &mut ui_state, current);

    match current {
        CreationFlow::AppearanceDrawing | CreationFlow::SuperpowerDrawing => {
            let slot = current.slot().expect("drawing screens always have a slot");
            drawing_screen(
                context,
                &mut session,
                &mut tools,
                &mut ui_state,
                &mut next_flow,
                &store,
                slot,
                current,
                &mut commands,
            );
        }
        CreationFlow::PlayerReview => review_screen(
            context,
            &mut session,
            &mut ui_state,
            &mut next_flow,
            &store,
            &mut continue_events,
        ),
        CreationFlow::ContinueToCoaching => handoff_screen(context, &session),
    }

    if ui_state.confirm_reset_player {
        reset_player_dialog(context, &mut session, &mut ui_state);
    }
}

fn top_bar(context: &egui::Context, session: &PlayerCreationSession, flow: CreationFlow) {
    egui::TopBottomPanel::top("creation-top-bar")
        .exact_height(58.0)
        .frame(panel_frame(egui::Color32::from_rgb(17, 26, 37)))
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(12.0);
                ui.heading(egui::RichText::new("Tactic Lab").size(22.0).strong());
                ui.separator();
                ui.label(egui::RichText::new("Player creation").color(MUTED));
                ui.separator();
                phase_indicator(ui, flow);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(format!("{} / 10 complete", session.completed_count()))
                            .color(MUTED)
                            .small(),
                    );
                    ui.separator();
                    player_chip(ui, session.active);
                });
            });
        });
}

/// Appearance -> Superpower -> Review, with the current phase highlighted.
fn phase_indicator(ui: &mut egui::Ui, flow: CreationFlow) {
    let phases = [
        ("Appearance", CreationFlow::AppearanceDrawing),
        ("Superpower", CreationFlow::SuperpowerDrawing),
        ("Review", CreationFlow::PlayerReview),
    ];
    let reached = |phase: CreationFlow| match flow {
        CreationFlow::AppearanceDrawing => phase == CreationFlow::AppearanceDrawing,
        CreationFlow::SuperpowerDrawing => phase != CreationFlow::PlayerReview,
        _ => true,
    };
    for (index, (label, phase)) in phases.iter().enumerate() {
        if index > 0 {
            ui.label(egui::RichText::new("→").color(MUTED));
        }
        let active = flow == *phase;
        let color = if active {
            BLUE
        } else if reached(*phase) {
            TEXT
        } else {
            MUTED
        };
        let mut text = egui::RichText::new(*label).color(color);
        if active {
            text = text.strong();
        }
        ui.label(text);
    }
}

fn player_chip(ui: &mut egui::Ui, player: PlayerId) {
    let color = team_color(player.team());
    egui::Frame::none()
        .fill(color.linear_multiply(0.22))
        .stroke(egui::Stroke::new(1.0_f32, color))
        .rounding(14.0)
        .inner_margin(egui::Margin::symmetric(11.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (response, painter) =
                    ui.allocate_painter(egui::vec2(12.0, 12.0), egui::Sense::hover());
                painter.circle_filled(response.rect.center(), 5.0, color);
                ui.label(
                    egui::RichText::new(format!(
                        "Player {} · {}",
                        player.get(),
                        player.team().label()
                    ))
                    .strong(),
                );
            });
        });
}

/// Left rail: explicit navigation across the ten fixed players.
fn roster_panel(
    context: &egui::Context,
    session: &mut PlayerCreationSession,
    ui_state: &mut CreationUiState,
    flow: CreationFlow,
) {
    egui::SidePanel::left("roster")
        .resizable(false)
        .exact_width(186.0)
        .frame(panel_frame(PANEL))
        .show(context, |ui| {
            ui.label(egui::RichText::new("ROSTER").small().color(MUTED));
            ui.add_space(4.0);

            let navigable = flow != CreationFlow::ContinueToCoaching;
            for team in [Team::Red, Team::Yellow] {
                ui.label(
                    egui::RichText::new(team.label().to_uppercase())
                        .small()
                        .color(team_color(team)),
                );
                for player in PlayerId::all().filter(|player| player.team() == team) {
                    roster_row(ui, session, ui_state, player, navigable);
                }
                ui.add_space(6.0);
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(navigable, egui::Button::new("← Prev"))
                    .clicked()
                {
                    session.select_previous();
                    ui_state.saved_notice = None;
                }
                if ui
                    .add_enabled(navigable, egui::Button::new("Next →"))
                    .clicked()
                {
                    session.select_next();
                    ui_state.saved_notice = None;
                }
            });
            ui.add_space(4.0);
            if ui
                .add_enabled(navigable, egui::Button::new("Reset this player"))
                .on_hover_text("Clears both drawings for the selected player only")
                .clicked()
            {
                ui_state.confirm_reset_player = true;
            }
        });
}

fn roster_row(
    ui: &mut egui::Ui,
    session: &mut PlayerCreationSession,
    ui_state: &mut CreationUiState,
    player: PlayerId,
    navigable: bool,
) {
    let entry = session.entry(player);
    let appearance_done = entry.appearance_saved_at.is_some();
    let superpower_done = entry.superpower_saved_at.is_some();
    let selected = session.active == player;

    let response = ui.add_enabled(
        navigable,
        egui::Button::new(
            egui::RichText::new(format!("Player {}", player.get())).color(if selected {
                TEXT
            } else {
                MUTED
            }),
        )
        .selected(selected)
        .min_size(egui::vec2(ui.available_width(), 28.0)),
    );
    // Completion dots sit on the button's own rect so the row stays one line.
    let painter = ui.painter();
    let center_y = response.rect.center().y;
    for (index, done) in [appearance_done, superpower_done].into_iter().enumerate() {
        let x = response.rect.right() - 18.0 + index as f32 * 10.0;
        let color = if done { GREEN } else { BORDER };
        painter.circle_filled(egui::pos2(x, center_y), 3.5, color);
    }
    if response.clicked() {
        session.select(player);
        ui_state.saved_notice = None;
        ui_state.error = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn drawing_screen(
    context: &egui::Context,
    session: &mut PlayerCreationSession,
    tools: &mut ToolSettings,
    ui_state: &mut CreationUiState,
    next_flow: &mut NextState<CreationFlow>,
    store: &StoreResource,
    slot: DrawingSlot,
    flow: CreationFlow,
    commands: &mut Vec<CanvasCommand>,
) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(APP)
                .inner_margin(egui::Margin::same(16.0)),
        )
        .show(context, |ui| {
            prompt_header(ui, slot, session.active);
            if let Some(error) = ui_state.error.clone() {
                banner(ui, &error, CORAL);
            }
            if let Some(notice) = ui_state.saved_notice.clone() {
                banner(ui, &notice, GREEN);
            }

            let toolbar_height = 56.0;
            let available = ui.available_rect_before_wrap();
            // Square canvas, letterboxed into whatever space the window leaves.
            let side = (available.height() - toolbar_height - 10.0)
                .min(available.width())
                .max(80.0);
            let canvas_rect = egui::Rect::from_center_size(
                egui::pos2(available.center().x, available.min.y + side * 0.5),
                egui::vec2(side, side),
            );

            let response = canvas_surface(ui, session, ui_state, slot, canvas_rect);
            apply_pointer(
                session.active_canvas_mut(slot),
                &response,
                canvas_rect,
                tools,
            );
            if response.dragged() || response.drag_released() || response.clicked() {
                ui_state.invalidate(session.active, slot);
                ui_state.saved_notice = None;
            }

            let toolbar_rect = egui::Rect::from_min_size(
                egui::pos2(
                    canvas_rect.center().x - side.min(620.0) * 0.5,
                    canvas_rect.max.y + 8.0,
                ),
                egui::vec2(side.min(620.0), toolbar_height),
            );
            ui.allocate_ui_at_rect(toolbar_rect, |ui| {
                tool_strip(ui, session, tools, ui_state, slot, commands);
            });
        });

    // Keyboard commands the tool strip did not already consume.
    for command in commands.drain(..) {
        match command {
            CanvasCommand::Undo => {
                session.active_canvas_mut(slot).undo();
                ui_state.invalidate(session.active, slot);
            }
            CanvasCommand::Clear => {
                session.active_canvas_mut(slot).clear();
                ui_state.invalidate(session.active, slot);
            }
            CanvasCommand::Save => {
                save_active(session, ui_state, store, slot, next_flow, flow);
            }
            CanvasCommand::NextPlayer => {
                session.select_next();
                ui_state.saved_notice = None;
            }
            CanvasCommand::PreviousPlayer => {
                session.select_previous();
                ui_state.saved_notice = None;
            }
            _ => {}
        }
    }
}

fn prompt_header(ui: &mut egui::Ui, slot: DrawingSlot, player: PlayerId) {
    let (title, hint) = match slot {
        DrawingSlot::Appearance => (
            format!("Draw player {}", player.get()),
            "What does this player look like?",
        ),
        DrawingSlot::Superpower => (
            format!("Draw player {}'s superpower", player.get()),
            "What can this player do that nobody else can?",
        ),
    };
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new(title).size(19.0).strong());
        ui.label(egui::RichText::new(hint).color(MUTED));
    });
    ui.add_space(6.0);
}

/// Paints the canvas: committed strokes come from a cached texture, the
/// in-progress stroke is painted directly so dragging never re-uploads pixels.
fn canvas_surface(
    ui: &mut egui::Ui,
    session: &mut PlayerCreationSession,
    ui_state: &mut CreationUiState,
    slot: DrawingSlot,
    rect: egui::Rect,
) -> egui::Response {
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 10.0, CANVAS_GROUND);
    painter.rect_stroke(rect, 10.0, egui::Stroke::new(1.0_f32, BORDER));

    let player = session.active;
    let key = (player.get(), slot.slug());
    let canvas = session.active_canvas_mut(slot);
    let revision = canvas.stroke_count() as u64;

    let needs_upload = ui_state.uploaded_revision.get(&key) != Some(&revision)
        || !ui_state.textures.contains_key(&key);
    if needs_upload {
        let size = [canvas.width() as usize, canvas.height() as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, canvas.raster());
        match ui_state.textures.get_mut(&key) {
            Some(handle) => handle.set(image, egui::TextureOptions::LINEAR),
            None => {
                let handle = ui.ctx().load_texture(
                    format!("canvas-{}-{}", key.0, key.1),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                ui_state.textures.insert(key, handle);
            }
        }
        ui_state.uploaded_revision.insert(key, revision);
    }

    if let Some(handle) = ui_state.textures.get(&key) {
        painter.image(
            handle.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    if canvas.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            match slot {
                DrawingSlot::Appearance => "Draw your player here",
                DrawingSlot::Superpower => "Draw the superpower here",
            },
            egui::FontId::proportional(15.0),
            MUTED,
        );
    }

    paint_active_stroke(&painter, canvas, rect);
    response
}

fn paint_active_stroke(painter: &egui::Painter, canvas: &Canvas, rect: egui::Rect) {
    let Some(stroke) = canvas.active_stroke() else {
        return;
    };
    let color = match stroke.tool {
        BrushTool::Brush => egui::Color32::from_rgba_unmultiplied(
            stroke.color[0],
            stroke.color[1],
            stroke.color[2],
            stroke.color[3],
        ),
        // The eraser has no colour of its own; show where it is acting.
        BrushTool::Eraser => CANVAS_GROUND,
    };
    let width = stroke.width * rect.width();
    let points: Vec<egui::Pos2> = stroke
        .points
        .iter()
        .map(|point| {
            egui::pos2(
                rect.min.x + point[0] * rect.width(),
                rect.min.y + point[1] * rect.height(),
            )
        })
        .collect();

    if points.len() == 1 {
        painter.circle_filled(points[0], width * 0.5, color);
        return;
    }
    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], egui::Stroke::new(width, color));
        painter.circle_filled(pair[1], width * 0.5, color);
    }
}

fn tool_strip(
    ui: &mut egui::Ui,
    session: &mut PlayerCreationSession,
    tools: &mut ToolSettings,
    ui_state: &mut CreationUiState,
    slot: DrawingSlot,
    commands: &mut Vec<CanvasCommand>,
) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(207, 214, 217))
        .rounding(7.0)
        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            ui.visuals_mut().override_text_color = Some(egui::Color32::from_rgb(39, 49, 61));
            ui.horizontal_centered(|ui| {
                if ui
                    .add(egui::Button::new("Brush").selected(tools.tool == BrushTool::Brush))
                    .clicked()
                {
                    tools.tool = BrushTool::Brush;
                }
                if ui
                    .add(egui::Button::new("Eraser").selected(tools.tool == BrushTool::Eraser))
                    .clicked()
                {
                    tools.tool = BrushTool::Eraser;
                }
                ui.separator();

                for (name, color) in PALETTE {
                    let selected = tools.color == color;
                    let (response, painter) =
                        ui.allocate_painter(egui::vec2(20.0, 20.0), egui::Sense::click());
                    let swatch = egui::Color32::from_rgb(color[0], color[1], color[2]);
                    painter.circle_filled(response.rect.center(), 8.0, swatch);
                    if selected {
                        painter.circle_stroke(
                            response.rect.center(),
                            9.5,
                            egui::Stroke::new(2.0_f32, BLUE),
                        );
                    }
                    if response.on_hover_text(name).clicked() {
                        tools.color = color;
                        tools.tool = BrushTool::Brush;
                    }
                }
                ui.separator();

                ui.add(
                    egui::Slider::new(&mut tools.width, MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .on_hover_text("Brush size");
                ui.separator();

                let can_undo = session.active_canvas(slot).stroke_count() > 0;
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo"))
                    .clicked()
                {
                    commands.push(CanvasCommand::Undo);
                }
                if ui
                    .add_enabled(can_undo, egui::Button::new("Clear"))
                    .clicked()
                {
                    commands.push(CanvasCommand::Clear);
                }

                let has_drawing = !session.active_canvas(slot).is_empty();
                let label = match slot {
                    DrawingSlot::Appearance => "Save & draw superpower",
                    DrawingSlot::Superpower => "Save & review",
                };
                if ui
                    .add_enabled(
                        has_drawing,
                        egui::Button::new(egui::RichText::new(label).strong())
                            .fill(BLUE)
                            .min_size(egui::vec2(170.0, 34.0)),
                    )
                    .on_disabled_hover_text("Draw something first")
                    .clicked()
                {
                    commands.push(CanvasCommand::Save);
                }
                let _ = ui_state;
            });
        });
}

/// Writes the active drawing, then advances only if the write succeeded.
fn save_active(
    session: &mut PlayerCreationSession,
    ui_state: &mut CreationUiState,
    store: &StoreResource,
    slot: DrawingSlot,
    next_flow: &mut NextState<CreationFlow>,
    flow: CreationFlow,
) {
    if session.active_canvas(slot).is_empty() {
        ui_state.error = Some(format!(
            "Draw the {} before saving.",
            slot.label().to_lowercase()
        ));
        return;
    }

    let player = session.active;
    let session_id = session.id.clone();
    let png = match session.active_canvas_mut(slot).to_png() {
        Ok(png) => png,
        Err(error) => {
            ui_state.error = Some(format!("Could not render the drawing: {error:#}"));
            return;
        }
    };

    if let Err(error) = store.0.save_drawing(&session_id, player, slot, &png) {
        ui_state.error = Some(format!(
            "Could not save player {}'s {}: {error:#}. Check that the output folder is writable, then try again.",
            player.get(),
            slot.label().to_lowercase()
        ));
        return;
    }

    // Mark saved before the manifest is built so it records this write.
    let previous = session.entry(player).saved_at(slot).map(str::to_owned);
    session
        .entry_mut(player)
        .mark_saved(slot, crate::state::iso_timestamp());

    if let Err(error) = store
        .0
        .write_manifest(&CreationManifest::from_session(session))
    {
        // Roll the flag back so the manifest and the UI cannot disagree.
        match previous {
            Some(timestamp) => session.entry_mut(player).mark_saved(slot, timestamp),
            None => session.entry_mut(player).clear_saved(slot),
        }
        ui_state.error = Some(format!("Could not update the manifest: {error:#}"));
        return;
    }

    ui_state.error = None;
    ui_state.saved_notice = Some(format!(
        "Saved player {}'s {}.",
        player.get(),
        slot.label().to_lowercase()
    ));
    next_flow.set(match flow {
        CreationFlow::AppearanceDrawing => CreationFlow::SuperpowerDrawing,
        _ => CreationFlow::PlayerReview,
    });
}

fn review_screen(
    context: &egui::Context,
    session: &mut PlayerCreationSession,
    ui_state: &mut CreationUiState,
    next_flow: &mut NextState<CreationFlow>,
    store: &StoreResource,
    continue_events: &mut EventWriter<ContinueToCoaching>,
) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(APP)
                .inner_margin(egui::Margin::same(16.0)),
        )
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new(format!("Player {} review", session.active.get()))
                        .size(19.0)
                        .strong(),
                );
                ui.label(egui::RichText::new("Check both drawings before moving on.").color(MUTED));
            });
            ui.add_space(8.0);
            if let Some(error) = ui_state.error.clone() {
                banner(ui, &error, CORAL);
            }

            let preview_side =
                ((ui.available_width() - 40.0) * 0.5).min(ui.available_height() - 110.0);
            ui.horizontal_top(|ui| {
                for slot in [DrawingSlot::Appearance, DrawingSlot::Superpower] {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(slot.label()).strong());
                        let rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(preview_side.max(80.0), preview_side.max(80.0)),
                        );
                        ui.allocate_rect(rect, egui::Sense::hover());
                        canvas_preview(ui, session, ui_state, slot, rect);
                        let saved = session.entry(session.active).saved_at(slot).is_some();
                        ui.label(
                            egui::RichText::new(if saved { "Saved" } else { "Not saved" })
                                .small()
                                .color(if saved { GREEN } else { MUTED }),
                        );
                        if ui
                            .button(format!("Edit {}", slot.label().to_lowercase()))
                            .clicked()
                        {
                            next_flow.set(match slot {
                                DrawingSlot::Appearance => CreationFlow::AppearanceDrawing,
                                DrawingSlot::Superpower => CreationFlow::SuperpowerDrawing,
                            });
                        }
                    });
                    ui.add_space(16.0);
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Next player →").clicked() {
                    session.select_next();
                    ui_state.saved_notice = None;
                    next_flow.set(CreationFlow::AppearanceDrawing);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("Continue to coaching").strong())
                                .fill(BLUE)
                                .min_size(egui::vec2(190.0, 38.0)),
                        )
                        .clicked()
                    {
                        let manifest = CreationManifest::from_session(session);
                        match store.0.write_manifest(&manifest) {
                            Ok(path) => {
                                continue_events.send(ContinueToCoaching {
                                    session_id: session.id.clone(),
                                    manifest_path: Some(path),
                                });
                                ui_state.error = None;
                                next_flow.set(CreationFlow::ContinueToCoaching);
                            }
                            Err(error) => {
                                ui_state.error =
                                    Some(format!("Could not finalize the session: {error:#}"));
                            }
                        }
                    }
                });
            });
        });
}

fn canvas_preview(
    ui: &mut egui::Ui,
    session: &mut PlayerCreationSession,
    ui_state: &mut CreationUiState,
    slot: DrawingSlot,
    rect: egui::Rect,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, CANVAS_GROUND);
    painter.rect_stroke(rect, 8.0, egui::Stroke::new(1.0_f32, BORDER));

    let key = (session.active.get(), slot.slug());
    let canvas = session.active_canvas_mut(slot);
    let revision = canvas.stroke_count() as u64;
    if ui_state.uploaded_revision.get(&key) != Some(&revision)
        || !ui_state.textures.contains_key(&key)
    {
        let size = [canvas.width() as usize, canvas.height() as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, canvas.raster());
        match ui_state.textures.get_mut(&key) {
            Some(handle) => handle.set(image, egui::TextureOptions::LINEAR),
            None => {
                let handle = ui.ctx().load_texture(
                    format!("preview-{}-{}", key.0, key.1),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                ui_state.textures.insert(key, handle);
            }
        }
        ui_state.uploaded_revision.insert(key, revision);
    }
    if let Some(handle) = ui_state.textures.get(&key) {
        painter.image(
            handle.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

/// Terminal screen. In the combined app the coaching plugin takes over here.
fn handoff_screen(context: &egui::Context, session: &PlayerCreationSession) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(APP)
                .inner_margin(egui::Margin::same(16.0)),
        )
        .show(context, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.35);
                ui.heading(
                    egui::RichText::new("Ready for coaching")
                        .size(22.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} of 10 players created. Session {}.",
                        session.completed_count(),
                        session.id
                    ))
                    .color(MUTED),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Drawings are saved under output/player-creations/.")
                        .small()
                        .color(MUTED),
                );
            });
        });
}

fn reset_player_dialog(
    context: &egui::Context,
    session: &mut PlayerCreationSession,
    ui_state: &mut CreationUiState,
) {
    let player = session.active;
    egui::Window::new("Reset this player?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(context, |ui| {
            ui.label(format!(
                "This clears player {}'s appearance and superpower drawings. Other players are not affected.",
                player.get()
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui_state.confirm_reset_player = false;
                }
                if ui.add(egui::Button::new("Reset player").fill(CORAL)).clicked() {
                    session.entry_mut(player).reset();
                    ui_state.invalidate(player, DrawingSlot::Appearance);
                    ui_state.invalidate(player, DrawingSlot::Superpower);
                    ui_state.saved_notice = None;
                    ui_state.error = None;
                    ui_state.confirm_reset_player = false;
                }
            });
        });
}

fn banner(ui: &mut egui::Ui, message: &str, color: egui::Color32) {
    egui::Frame::none()
        .fill(color.linear_multiply(0.16))
        .stroke(egui::Stroke::new(1.0_f32, color))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(message).color(color));
        });
    ui.add_space(6.0);
}

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .inner_margin(egui::Margin::symmetric(14.0, 9.0))
}
