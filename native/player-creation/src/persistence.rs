//! Local storage for player-creation artifacts.
//!
//! The [`CreationStore`] trait is the seam a later agent replaces to hand
//! drawings to a model. Implementing it does not require touching the drawing
//! UI or the data model. No API credentials live in this process: a future
//! remote store should post to the local service that already holds them.

use crate::state::{DrawingSlot, PlayerCreationSession, PlayerId, Team};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST_SCHEMA_VERSION: u8 = 1;
pub const MANIFEST_FILE_NAME: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasMetadata {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeCounts {
    pub appearance: usize,
    pub superpower: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerManifestEntry {
    pub player_id: u8,
    pub team: Team,
    /// Paths are relative to the manifest, so the directory can be moved or
    /// uploaded as a unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superpower_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance_saved_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superpower_saved_at: Option<String>,
    pub stroke_counts: StrokeCounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationManifest {
    pub schema_version: u8,
    pub session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub canvas: CanvasMetadata,
    pub players: Vec<PlayerManifestEntry>,
}

impl CreationManifest {
    /// Builds the manifest from the live session. Entries are keyed by player
    /// id and rebuilt from current state, so repeated saves update in place and
    /// can never append a duplicate entry for the same player.
    pub fn from_session(session: &PlayerCreationSession) -> Self {
        let players = PlayerId::all()
            .map(|player| {
                let entry = session.entry(player);
                PlayerManifestEntry {
                    player_id: player.get(),
                    team: player.team(),
                    appearance_path: entry
                        .appearance_saved_at
                        .is_some()
                        .then(|| drawing_file_name(player, DrawingSlot::Appearance)),
                    superpower_path: entry
                        .superpower_saved_at
                        .is_some()
                        .then(|| drawing_file_name(player, DrawingSlot::Superpower)),
                    appearance_saved_at: entry.appearance_saved_at.clone(),
                    superpower_saved_at: entry.superpower_saved_at.clone(),
                    stroke_counts: StrokeCounts {
                        appearance: entry.appearance.stroke_count(),
                        superpower: entry.superpower.stroke_count(),
                    },
                }
            })
            .collect();

        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            session_id: session.id.clone(),
            created_at: session.created_at.clone(),
            updated_at: crate::state::iso_timestamp(),
            canvas: CanvasMetadata {
                width: crate::state::CANVAS_WIDTH,
                height: crate::state::CANVAS_HEIGHT,
            },
            players,
        }
    }

    pub fn entry(&self, player: PlayerId) -> Option<&PlayerManifestEntry> {
        self.players
            .iter()
            .find(|entry| entry.player_id == player.get())
    }
}

/// `player-03-appearance.png`. Zero-padded so a directory listing sorts in
/// roster order rather than 1, 10, 2.
pub fn drawing_file_name(player: PlayerId, slot: DrawingSlot) -> String {
    format!("player-{:02}-{}.png", player.get(), slot.slug())
}

/// Where a session's artifacts live, relative to the store root.
pub fn session_directory(root: &Path, session_id: &str) -> PathBuf {
    root.join("player-creations").join(session_id)
}

pub fn drawing_path(root: &Path, session_id: &str, player: PlayerId, slot: DrawingSlot) -> PathBuf {
    session_directory(root, session_id).join(drawing_file_name(player, slot))
}

/// Storage backend for creation artifacts. Swap the implementation to add a
/// model handoff; the UI only ever sees this trait.
pub trait CreationStore: Send + Sync {
    /// Persists one rendered drawing and returns where it landed.
    fn save_drawing(
        &self,
        session_id: &str,
        player: PlayerId,
        slot: DrawingSlot,
        png: &[u8],
    ) -> Result<PathBuf>;

    /// Writes the manifest, replacing any previous copy.
    fn write_manifest(&self, manifest: &CreationManifest) -> Result<PathBuf>;
}

/// Writes to an ignored local output directory.
#[derive(Debug, Clone)]
pub struct LocalFileStore {
    root: PathBuf,
}

impl Default for LocalFileStore {
    fn default() -> Self {
        Self::new(PathBuf::from("output"))
    }
}

impl LocalFileStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl CreationStore for LocalFileStore {
    fn save_drawing(
        &self,
        session_id: &str,
        player: PlayerId,
        slot: DrawingSlot,
        png: &[u8],
    ) -> Result<PathBuf> {
        let path = drawing_path(&self.root, session_id, player, slot);
        atomic_write(&path, png)?;
        Ok(path)
    }

    fn write_manifest(&self, manifest: &CreationManifest) -> Result<PathBuf> {
        let path = session_directory(&self.root, &manifest.session_id).join(MANIFEST_FILE_NAME);
        let bytes = serde_json::to_vec_pretty(manifest).context("serializing manifest")?;
        atomic_write(&path, &bytes)?;
        Ok(path)
    }
}

/// Writes through a temporary file and renames into place, so a crash or a full
/// disk leaves the previous file intact rather than a half-written one.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("output path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact"),
        std::process::id()
    ));
    fs::write(&temporary, bytes).with_context(|| format!("writing {}", temporary.display()))?;

    // Windows refuses to rename onto an existing file, so step through a backup
    // and restore it if the replacement fails.
    #[cfg(windows)]
    if path.exists() {
        let backup = path.with_extension("bak");
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).with_context(|| format!("backing up {}", path.display()))?;
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::rename(&backup, path);
            return Err(error).with_context(|| format!("replacing {}", path.display()));
        }
        let _ = fs::remove_file(backup);
        return Ok(());
    }

    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("replacing {}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::BrushTool;
    use crate::state::PlayerCreationSession;

    fn player(id: u8) -> PlayerId {
        PlayerId::new(id).unwrap()
    }

    fn draw_and_save(session: &mut PlayerCreationSession, id: u8, slot: DrawingSlot) {
        session.select(player(id));
        let canvas = session.active_canvas_mut(slot);
        canvas.begin_stroke(BrushTool::Brush, [255, 255, 255, 255], 0.1);
        canvas.extend_stroke([0.4, 0.4]);
        canvas.extend_stroke([0.6, 0.6]);
        canvas.end_stroke();
        session
            .active_entry_mut()
            .mark_saved(slot, crate::state::iso_timestamp());
    }

    #[test]
    fn save_paths_are_stable_and_scoped_to_session_and_player() {
        let root = Path::new("output");
        let path = drawing_path(root, "session-abc", player(3), DrawingSlot::Appearance);
        assert_eq!(
            path,
            Path::new("output/player-creations/session-abc/player-03-appearance.png")
        );

        // Same player, other slot, and same slot, other player, stay distinct.
        assert_ne!(
            path,
            drawing_path(root, "session-abc", player(3), DrawingSlot::Superpower)
        );
        assert_ne!(
            path,
            drawing_path(root, "session-abc", player(4), DrawingSlot::Appearance)
        );
        assert_ne!(
            path,
            drawing_path(root, "session-xyz", player(3), DrawingSlot::Appearance)
        );
    }

    #[test]
    fn manifest_serializes_the_camel_case_contract() {
        let session = PlayerCreationSession::default();
        let json = serde_json::to_value(CreationManifest::from_session(&session)).unwrap();

        assert_eq!(json["schemaVersion"], MANIFEST_SCHEMA_VERSION);
        assert!(json["sessionId"].is_string());
        assert!(json["createdAt"].is_string());
        assert!(json["updatedAt"].is_string());
        assert_eq!(json["canvas"]["width"], crate::state::CANVAS_WIDTH);
        assert_eq!(json["players"].as_array().unwrap().len(), 10);
        assert_eq!(json["players"][0]["playerId"], 1);
        assert_eq!(json["players"][0]["team"], "red");
        assert_eq!(json["players"][9]["team"], "yellow");
        // Nothing saved yet, so no drawing paths are advertised.
        assert!(json["players"][0].get("appearancePath").is_none());
    }

    #[test]
    fn manifest_round_trips() {
        let session = PlayerCreationSession::default();
        let manifest = CreationManifest::from_session(&session);
        let encoded = serde_json::to_string(&manifest).unwrap();
        let decoded: CreationManifest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, manifest);
    }

    #[test]
    fn repeated_saves_update_entries_in_place() {
        let mut session = PlayerCreationSession::default();
        draw_and_save(&mut session, 3, DrawingSlot::Appearance);

        let first = CreationManifest::from_session(&session);
        assert_eq!(first.players.len(), 10);
        assert!(first.entry(player(3)).unwrap().appearance_path.is_some());
        assert!(first.entry(player(3)).unwrap().superpower_path.is_none());

        // Saving the same player again, then the other slot, must not append.
        draw_and_save(&mut session, 3, DrawingSlot::Appearance);
        draw_and_save(&mut session, 3, DrawingSlot::Superpower);

        let second = CreationManifest::from_session(&session);
        assert_eq!(
            second.players.len(),
            10,
            "manifest must stay one entry per player"
        );
        assert_eq!(
            second.players.iter().filter(|e| e.player_id == 3).count(),
            1,
            "player 3 must appear exactly once"
        );
        let entry = second.entry(player(3)).unwrap();
        assert!(entry.appearance_path.is_some() && entry.superpower_path.is_some());
        assert_eq!(
            entry.stroke_counts.appearance, 2,
            "both appearance saves kept their strokes"
        );
    }

    #[test]
    fn local_store_writes_drawings_and_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let store = LocalFileStore::new(directory.path().to_path_buf());
        let mut session = PlayerCreationSession::default();
        draw_and_save(&mut session, 7, DrawingSlot::Appearance);

        let png = session
            .entry_mut(player(7))
            .canvas_mut(DrawingSlot::Appearance)
            .to_png()
            .unwrap();
        let drawing_path = store
            .save_drawing(&session.id, player(7), DrawingSlot::Appearance, &png)
            .unwrap();
        let manifest_path = store
            .write_manifest(&CreationManifest::from_session(&session))
            .unwrap();

        assert!(drawing_path.exists());
        assert!(manifest_path.exists());
        assert_eq!(&fs::read(&drawing_path).unwrap()[1..4], b"PNG");

        let decoded: CreationManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(decoded.session_id, session.id);
        assert_eq!(
            decoded.entry(player(7)).unwrap().appearance_path.as_deref(),
            Some("player-07-appearance.png")
        );

        // Rewriting leaves exactly one manifest and no temporary files behind.
        store
            .write_manifest(&CreationManifest::from_session(&session))
            .unwrap();
        let stray = fs::read_dir(manifest_path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .count();
        assert_eq!(stray, 0, "atomic writes must not leave temporary files");
    }

    #[test]
    fn a_failed_write_reports_an_error_instead_of_panicking() {
        let directory = tempfile::tempdir().unwrap();
        // A file where the session directory needs to be makes create_dir_all fail.
        let blocker = directory.path().join("player-creations");
        fs::write(&blocker, b"not a directory").unwrap();

        let store = LocalFileStore::new(directory.path().to_path_buf());
        let result = store.save_drawing("session-abc", player(1), DrawingSlot::Appearance, b"x");
        assert!(
            result.is_err(),
            "the caller must be able to keep the user on screen"
        );
    }
}
