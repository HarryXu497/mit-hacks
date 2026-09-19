use crate::drawing::Canvas;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

/// Export resolution for every drawing. Independent of window size so that
/// resizing never changes what gets written to disk.
pub const CANVAS_WIDTH: u32 = 1024;
pub const CANVAS_HEIGHT: u32 = 1024;

/// The fixed 5-v-5 setup: red `1..=5`, yellow `6..=10`.
pub const PLAYER_COUNT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Team {
    Red,
    Yellow,
}

impl Team {
    pub fn label(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Yellow => "Yellow",
        }
    }
}

/// A validated player identity. Constructing one is the only way to name a
/// player, so an out-of-range id cannot reach the roster or a save path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(u8);

impl PlayerId {
    pub fn new(value: u8) -> Option<Self> {
        (1..=PLAYER_COUNT as u8)
            .contains(&value)
            .then_some(Self(value))
    }

    pub fn get(self) -> u8 {
        self.0
    }

    /// The single source of truth for id-to-team mapping.
    pub fn team(self) -> Team {
        if self.0 <= 5 {
            Team::Red
        } else {
            Team::Yellow
        }
    }

    fn index(self) -> usize {
        self.0 as usize - 1
    }

    pub fn all() -> impl Iterator<Item = Self> {
        (1..=PLAYER_COUNT as u8).map(Self)
    }
}

/// Which of a player's two drawings is being edited.
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

/// Everything belonging to one player. The canvases are owned here and indexed
/// by player id, so there is no free-floating "current drawing" that could be
/// attached to the wrong player when the user navigates.
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

    /// Clears this player only. Never touches another player's work.
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
    pub active: PlayerId,
    entries: Vec<PlayerEntry>,
}

impl Default for PlayerCreationSession {
    fn default() -> Self {
        Self {
            id: create_id("session"),
            created_at: iso_timestamp(),
            active: PlayerId(1),
            entries: (0..PLAYER_COUNT).map(|_| PlayerEntry::default()).collect(),
        }
    }
}

impl PlayerCreationSession {
    pub fn entry(&self, player: PlayerId) -> &PlayerEntry {
        &self.entries[player.index()]
    }

    pub fn entry_mut(&mut self, player: PlayerId) -> &mut PlayerEntry {
        &mut self.entries[player.index()]
    }

    pub fn active_entry(&self) -> &PlayerEntry {
        self.entry(self.active)
    }

    pub fn active_entry_mut(&mut self) -> &mut PlayerEntry {
        let active = self.active;
        self.entry_mut(active)
    }

    /// The one place `(active player, slot)` is resolved into a canvas.
    pub fn active_canvas(&self, slot: DrawingSlot) -> &Canvas {
        self.active_entry().canvas(slot)
    }

    pub fn active_canvas_mut(&mut self, slot: DrawingSlot) -> &mut Canvas {
        self.active_entry_mut().canvas_mut(slot)
    }

    pub fn select(&mut self, player: PlayerId) {
        self.active = player;
    }

    /// Advance to the next player, wrapping at 10.
    pub fn select_next(&mut self) {
        let next = self.active.get() % PLAYER_COUNT as u8 + 1;
        self.active = PlayerId(next);
    }

    /// Step back a player, wrapping at 1.
    pub fn select_previous(&mut self) {
        let previous = match self.active.get() {
            1 => PLAYER_COUNT as u8,
            other => other - 1,
        };
        self.active = PlayerId(previous);
    }

    pub fn completed_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.is_complete())
            .count()
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
    fn player_id_maps_to_team() {
        for value in 1..=5 {
            assert_eq!(PlayerId::new(value).unwrap().team(), Team::Red);
        }
        for value in 6..=10 {
            assert_eq!(PlayerId::new(value).unwrap().team(), Team::Yellow);
        }
    }

    #[test]
    fn player_id_rejects_ids_outside_the_fixed_setup() {
        assert!(PlayerId::new(0).is_none());
        assert!(PlayerId::new(11).is_none());
        assert_eq!(PlayerId::all().count(), PLAYER_COUNT);
    }

    #[test]
    fn appearance_and_superpower_are_independent() {
        let mut session = PlayerCreationSession::default();
        let canvas = session.active_canvas_mut(DrawingSlot::Appearance);
        canvas.begin_stroke(crate::drawing::BrushTool::Brush, [255, 0, 0, 255], 0.05);
        canvas.extend_stroke([0.2, 0.2]);
        canvas.end_stroke();

        assert_eq!(
            session
                .active_canvas(DrawingSlot::Appearance)
                .stroke_count(),
            1
        );
        assert!(session.active_canvas(DrawingSlot::Superpower).is_empty());

        // Clearing one slot leaves the other alone.
        session.active_canvas_mut(DrawingSlot::Superpower).clear();
        assert_eq!(
            session
                .active_canvas(DrawingSlot::Appearance)
                .stroke_count(),
            1
        );
    }

    #[test]
    fn switching_players_does_not_move_strokes() {
        let mut session = PlayerCreationSession::default();
        session.select(PlayerId::new(3).unwrap());
        let canvas = session.active_canvas_mut(DrawingSlot::Appearance);
        canvas.begin_stroke(crate::drawing::BrushTool::Brush, [255, 255, 255, 255], 0.05);
        canvas.extend_stroke([0.4, 0.4]);
        canvas.end_stroke();

        session.select(PlayerId::new(4).unwrap());
        assert!(
            session.active_canvas(DrawingSlot::Appearance).is_empty(),
            "player 4 must not inherit player 3's drawing"
        );

        session.select(PlayerId::new(3).unwrap());
        assert_eq!(
            session
                .active_canvas(DrawingSlot::Appearance)
                .stroke_count(),
            1
        );
    }

    #[test]
    fn navigation_wraps_across_the_fixed_roster() {
        let mut session = PlayerCreationSession::default();
        assert_eq!(session.active.get(), 1);
        session.select_previous();
        assert_eq!(session.active.get(), 10);
        session.select_next();
        assert_eq!(session.active.get(), 1);

        session.select(PlayerId::new(5).unwrap());
        session.select_next();
        assert_eq!(session.active.get(), 6);
        assert_eq!(session.active.team(), Team::Yellow);
    }

    #[test]
    fn resetting_a_player_leaves_the_rest_of_the_session_intact() {
        let mut session = PlayerCreationSession::default();
        for id in [2u8, 7] {
            session.select(PlayerId::new(id).unwrap());
            for slot in [DrawingSlot::Appearance, DrawingSlot::Superpower] {
                let canvas = session.active_canvas_mut(slot);
                canvas.begin_stroke(crate::drawing::BrushTool::Brush, [1, 2, 3, 255], 0.05);
                canvas.extend_stroke([0.5, 0.5]);
                canvas.end_stroke();
            }
            session
                .active_entry_mut()
                .mark_saved(DrawingSlot::Appearance, "t".into());
            session
                .active_entry_mut()
                .mark_saved(DrawingSlot::Superpower, "t".into());
        }
        assert_eq!(session.completed_count(), 2);

        session.entry_mut(PlayerId::new(2).unwrap()).reset();

        assert!(session
            .entry(PlayerId::new(2).unwrap())
            .appearance
            .is_empty());
        assert!(!session
            .entry(PlayerId::new(7).unwrap())
            .appearance
            .is_empty());
        assert_eq!(session.completed_count(), 1);
        assert_eq!(
            session.entries.len(),
            PLAYER_COUNT,
            "reset must not drop roster slots"
        );
    }
}
