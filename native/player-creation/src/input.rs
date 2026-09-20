//! Input handling: pointer and keyboard translated into drawing intent.
//!
//! This layer decides *what the user asked for*; `ui.rs` decides where things
//! sit on screen and `drawing.rs` decides what a stroke looks like. Keeping the
//! translation here is what lets the canvas be driven by tests and, later, by
//! something other than a mouse.

use crate::drawing::{BrushTool, Canvas, CanvasPoint};
use bevy::prelude::*;
use bevy_egui::egui;

/// Brush colours offered in the tool strip.
pub const PALETTE: [(&str, [u8; 4]); 8] = [
    ("Chalk", [244, 248, 255, 255]),
    ("Coral", [239, 71, 73, 255]),
    ("Amber", [247, 196, 31, 255]),
    ("Grass", [46, 184, 114, 255]),
    ("Electric", [46, 145, 255, 255]),
    ("Violet", [154, 110, 245, 255]),
    ("Sand", [214, 170, 122, 255]),
    ("Ink", [22, 30, 42, 255]),
];

pub const MIN_BRUSH_WIDTH: f32 = 0.004;
pub const MAX_BRUSH_WIDTH: f32 = 0.16;

/// Currently selected tool settings. Shared by both drawing phases so the user
/// does not have to re-pick a brush when moving to the superpower canvas.
#[derive(Resource, Debug, Clone)]
pub struct ToolSettings {
    pub tool: BrushTool,
    pub color: [u8; 4],
    pub width: f32,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            tool: BrushTool::Brush,
            color: PALETTE[0].1,
            width: 0.022,
        }
    }
}

impl ToolSettings {
    pub fn adjust_width(&mut self, factor: f32) {
        self.width = (self.width * factor).clamp(MIN_BRUSH_WIDTH, MAX_BRUSH_WIDTH);
    }
}

/// A user intent that the UI applies once it knows which canvas is active.
/// Returning commands rather than mutating directly keeps keyboard handling
/// independent of which player and slot happen to be selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasCommand {
    Undo,
    Clear,
    Save,
    SelectBrush,
    SelectEraser,
    Grow,
    Shrink,
}

/// Maps a pointer position inside `rect` to normalized canvas coordinates.
/// Returns `None` outside the canvas so strokes cannot start off-surface.
pub fn canvas_point(position: egui::Pos2, rect: egui::Rect) -> Option<CanvasPoint> {
    if !rect.contains(position) || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    Some([
        (position.x - rect.min.x) / rect.width(),
        (position.y - rect.min.y) / rect.height(),
    ])
}

/// Clamped variant used while dragging, so leaving the canvas mid-stroke
/// pins the stroke to the edge instead of dropping points.
pub fn clamped_canvas_point(position: egui::Pos2, rect: egui::Rect) -> CanvasPoint {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return [0.0, 0.0];
    }
    [
        ((position.x - rect.min.x) / rect.width()).clamp(0.0, 1.0),
        ((position.y - rect.min.y) / rect.height()).clamp(0.0, 1.0),
    ]
}

/// Drives one canvas from the egui response for the canvas rectangle.
pub fn apply_pointer(
    canvas: &mut Canvas,
    response: &egui::Response,
    rect: egui::Rect,
    tools: &ToolSettings,
) {
    if response.drag_started() {
        if let Some(position) = response.interact_pointer_pos() {
            if let Some(point) = canvas_point(position, rect) {
                canvas.begin_stroke(tools.tool, tools.color, tools.width);
                canvas.extend_stroke(point);
            }
        }
    }

    if response.dragged() {
        if let Some(position) = response.interact_pointer_pos() {
            canvas.extend_stroke(clamped_canvas_point(position, rect));
        }
    }

    if response.drag_released() {
        canvas.end_stroke();
    }

    // A click without a drag should still leave a dot.
    if response.clicked() {
        if let Some(position) = response.interact_pointer_pos() {
            if let Some(point) = canvas_point(position, rect) {
                canvas.begin_stroke(tools.tool, tools.color, tools.width);
                canvas.extend_stroke(point);
                canvas.end_stroke();
            }
        }
    }
}

/// Reads keyboard shortcuts. Ignored while a text field has focus so typing a
/// name cannot erase a drawing.
pub fn keyboard_commands(context: &egui::Context) -> Vec<CanvasCommand> {
    if context.wants_keyboard_input() {
        return Vec::new();
    }
    context.input(|input| {
        let mut commands = Vec::new();
        if input.key_pressed(egui::Key::Z) && input.modifiers.command {
            commands.push(CanvasCommand::Undo);
        }
        if input.key_pressed(egui::Key::B) {
            commands.push(CanvasCommand::SelectBrush);
        }
        if input.key_pressed(egui::Key::E) {
            commands.push(CanvasCommand::SelectEraser);
        }
        if input.key_pressed(egui::Key::OpenBracket) {
            commands.push(CanvasCommand::Shrink);
        }
        if input.key_pressed(egui::Key::CloseBracket) {
            commands.push(CanvasCommand::Grow);
        }
        if input.key_pressed(egui::Key::Enter) {
            commands.push(CanvasCommand::Save);
        }
        commands
    })
}

/// Applies the tool-affecting commands. Canvas- and flow-affecting ones are
/// returned to the caller, which knows the active player and slot.
pub fn apply_tool_command(tools: &mut ToolSettings, command: CanvasCommand) -> bool {
    match command {
        CanvasCommand::SelectBrush => tools.tool = BrushTool::Brush,
        CanvasCommand::SelectEraser => tools.tool = BrushTool::Eraser,
        CanvasCommand::Grow => tools.adjust_width(1.25),
        CanvasCommand::Shrink => tools.adjust_width(0.8),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 200.0))
    }

    #[test]
    fn pointer_maps_into_normalized_canvas_space() {
        assert_eq!(
            canvas_point(egui::pos2(100.0, 50.0), rect()),
            Some([0.0, 0.0])
        );
        assert_eq!(
            canvas_point(egui::pos2(200.0, 150.0), rect()),
            Some([0.5, 0.5])
        );
    }

    #[test]
    fn pointer_outside_the_canvas_does_not_start_a_stroke() {
        assert_eq!(canvas_point(egui::pos2(10.0, 10.0), rect()), None);
        assert_eq!(canvas_point(egui::pos2(400.0, 150.0), rect()), None);
    }

    #[test]
    fn dragging_off_canvas_pins_to_the_edge() {
        assert_eq!(
            clamped_canvas_point(egui::pos2(-500.0, 900.0), rect()),
            [0.0, 1.0]
        );
    }

    #[test]
    fn a_degenerate_rect_is_handled_rather_than_dividing_by_zero() {
        let empty = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(0.0, 0.0));
        assert_eq!(canvas_point(egui::pos2(0.0, 0.0), empty), None);
        assert_eq!(
            clamped_canvas_point(egui::pos2(5.0, 5.0), empty),
            [0.0, 0.0]
        );
    }

    #[test]
    fn tool_commands_change_only_tool_settings() {
        let mut tools = ToolSettings::default();
        assert!(apply_tool_command(&mut tools, CanvasCommand::SelectEraser));
        assert_eq!(tools.tool, BrushTool::Eraser);
        assert!(apply_tool_command(&mut tools, CanvasCommand::SelectBrush));
        assert_eq!(tools.tool, BrushTool::Brush);

        // Undo is not a tool command; the caller handles it.
        assert!(!apply_tool_command(&mut tools, CanvasCommand::Undo));
    }

    #[test]
    fn brush_width_stays_within_usable_bounds() {
        let mut tools = ToolSettings::default();
        for _ in 0..50 {
            tools.adjust_width(2.0);
        }
        assert_eq!(tools.width, MAX_BRUSH_WIDTH);
        for _ in 0..100 {
            tools.adjust_width(0.5);
        }
        assert_eq!(tools.width, MIN_BRUSH_WIDTH);
    }
}
