use crate::drawing::Canvas;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

/// Export resolution for every drawing. Independent of window size so that
/// resizing never changes what gets written to disk.
pub const CANVAS_WIDTH: u32 = 1024;
pub const CANVAS_HEIGHT: u32 = 1024;

/// Which of the player's two drawings is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrawingSlot {
    Appearance,
    Superpower,
}

impl DrawingSlot {
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Superpower => "Superpower",
        }
    }

    /// Filename fragment. Kept here so persistence and the UI cannot disagree.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::Superpower => "superpower",
        }
    }
}

/// Screen the flow is currently on.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CreationFlow {
    #[default]
    AppearanceDrawing,
    SuperpowerDrawing,
    PlayerReview,
    ContinueToCoaching,
}

impl CreationFlow {
    /// The drawing slot this screen edits, if it is a drawing screen.
    pub fn slot(self) -> Option<DrawingSlot> {
        match self {
            Self::AppearanceDrawing => Some(DrawingSlot::Appearance),
            Self::SuperpowerDrawing => Some(DrawingSlot::Superpower),
            _ => None,
        }
    }
}

/// Emitted once the user leaves player creation. The coaching app listens for
/// this to take over; see `docs/native-player-creation.md`.
#[derive(Event, Debug, Clone)]
pub struct ContinueToCoaching {
    pub session_id: String,
    pub manifest_path: Option<std::path::PathBuf>,
}

/// The player's two drawings. There is exactly one of these per session — no
/// roster, no team split, just an appearance and a superpower.
#[derive(Debug)]
pub struct PlayerEntry {
    pub appearance: Canvas,
    pub superpower: Canvas,
    pub appearance_saved_at: Option<String>,
    pub superpower_saved_at: Option<String>,
}

impl Default for PlayerEntry {
    fn default() -> Self {
        Self {
            appearance: Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT),
            superpower: Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT),
            appearance_saved_at: None,
            superpower_saved_at: None,
        }
    }
}

impl PlayerEntry {
    pub fn canvas(&self, slot: DrawingSlot) -> &Canvas {
        match slot {
            DrawingSlot::Appearance => &self.appearance,
            DrawingSlot::Superpower => &self.superpower,
        }
    }

    pub fn canvas_mut(&mut self, slot: DrawingSlot) -> &mut Canvas {
        match slot {
            DrawingSlot::Appearance => &mut self.appearance,
            DrawingSlot::Superpower => &mut self.superpower,
        }
    }

    pub fn saved_at(&self, slot: DrawingSlot) -> Option<&str> {
        match slot {
            DrawingSlot::Appearance => self.appearance_saved_at.as_deref(),
            DrawingSlot::Superpower => self.superpower_saved_at.as_deref(),
        }
    }

    pub fn mark_saved(&mut self, slot: DrawingSlot, timestamp: String) {
        match slot {
            DrawingSlot::Appearance => self.appearance_saved_at = Some(timestamp),
            DrawingSlot::Superpower => self.superpower_saved_at = Some(timestamp),
        }
    }

    /// Undoes a `mark_saved`, used to roll back when a later write in the
    /// same save fails.
    pub fn clear_saved(&mut self, slot: DrawingSlot) {
        match slot {
            DrawingSlot::Appearance => self.appearance_saved_at = None,
            DrawingSlot::Superpower => self.superpower_saved_at = None,
        }
    }

    /// Both drawings exist and have been written to disk.
    pub fn is_complete(&self) -> bool {
        self.appearance_saved_at.is_some() && self.superpower_saved_at.is_some()
    }

    pub fn reset(&mut self) {
        self.appearance.clear();
        self.superpower.clear();
        self.appearance_saved_at = None;
        self.superpower_saved_at = None;
    }
}

#[derive(Resource, Debug)]
pub struct PlayerCreationSession {
    /// Stable for the lifetime of the session; names the output directory.
    pub id: String,
    pub created_at: String,
    pub player: PlayerEntry,
}

impl Default for PlayerCreationSession {
    fn default() -> Self {
        Self {
            id: create_id("session"),
            created_at: iso_timestamp(),
            player: PlayerEntry::default(),
        }
    }
}

impl PlayerCreationSession {
    pub fn canvas(&self, slot: DrawingSlot) -> &Canvas {
        self.player.canvas(slot)
    }

    pub fn canvas_mut(&mut self, slot: DrawingSlot) -> &mut Canvas {
        self.player.canvas_mut(slot)
    }
}

pub fn create_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

pub fn iso_timestamp() -> String {
    humantime::format_rfc3339(SystemTime::now()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_and_superpower_are_independent() {
        let mut session = PlayerCreationSession::default();
        let canvas = session.canvas_mut(DrawingSlot::Appearance);
        canvas.begin_stroke(crate::drawing::BrushTool::Brush, [255, 0, 0, 255], 0.05);
        canvas.extend_stroke([0.2, 0.2]);
        canvas.end_stroke();

        assert_eq!(session.canvas(DrawingSlot::Appearance).stroke_count(), 1);
        assert!(session.canvas(DrawingSlot::Superpower).is_empty());

        // Clearing one slot leaves the other alone.
        session.canvas_mut(DrawingSlot::Superpower).clear();
        assert_eq!(session.canvas(DrawingSlot::Appearance).stroke_count(), 1);
    }

    #[test]
    fn resetting_the_player_clears_both_drawings_and_their_saved_state() {
        let mut session = PlayerCreationSession::default();
        for slot in [DrawingSlot::Appearance, DrawingSlot::Superpower] {
            let canvas = session.canvas_mut(slot);
            canvas.begin_stroke(crate::drawing::BrushTool::Brush, [1, 2, 3, 255], 0.05);
            canvas.extend_stroke([0.5, 0.5]);
            canvas.end_stroke();
            session.player.mark_saved(slot, "t".into());
        }
        assert!(session.player.is_complete());

        session.player.reset();

        assert!(session.canvas(DrawingSlot::Appearance).is_empty());
        assert!(session.canvas(DrawingSlot::Superpower).is_empty());
        assert!(!session.player.is_complete());
    }
}
