//! Saving the paintings.
//!
//! The files written here follow the contract the 2D player-creation screen
//! established, byte for byte in layout: two transparent PNGs and a manifest
//! under `output/player-creations/<session_id>/`. Whatever reads those
//! artifacts downstream — the coaching handoff today, a model later — must not
//! be able to tell which screen produced them, so nothing about the shape is
//! invented here. The manifest tests pin that shape.

use super::paint::{Paintings, Slot, CANVAS_PX};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
pub struct CreationManifest {
    pub schema_version: u8,
    pub session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub canvas: CanvasMetadata,
    /// Relative to the manifest, so the directory can be moved as a unit.
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

/// Identity and save state for this sitting. One per run of the clearing.
#[derive(Resource, Debug, Clone)]
pub struct CreationSession {
    pub id: String,
    pub created_at: String,
    pub appearance_saved_at: Option<String>,
    pub superpower_saved_at: Option<String>,
    /// Where artifacts go. `output` beside the working directory, as in 2D.
    pub root: PathBuf,
}

impl Default for CreationSession {
    fn default() -> Self {
        Self {
            id: format!("session-{}", bevy::utils::Uuid::new_v4()),
            created_at: iso_timestamp(),
            appearance_saved_at: None,
            superpower_saved_at: None,
            root: PathBuf::from("output"),
        }
    }
}

impl CreationSession {
    pub fn saved_at(&self, slot: Slot) -> Option<&str> {
        match slot {
            Slot::Appearance => self.appearance_saved_at.as_deref(),
            Slot::Superpower => self.superpower_saved_at.as_deref(),
        }
    }

    fn set_saved(&mut self, slot: Slot, at: Option<String>) {
        match slot {
            Slot::Appearance => self.appearance_saved_at = at,
            Slot::Superpower => self.superpower_saved_at = at,
        }
    }

    pub fn directory(&self) -> PathBuf {
        self.root.join("player-creations").join(&self.id)
    }

    pub fn manifest(&self, paintings: &Paintings) -> CreationManifest {
        CreationManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            session_id: self.id.clone(),
            created_at: self.created_at.clone(),
            updated_at: iso_timestamp(),
            canvas: CanvasMetadata {
                width: CANVAS_PX,
                height: CANVAS_PX,
            },
            appearance_path: self
                .appearance_saved_at
                .is_some()
                .then(|| drawing_file_name(Slot::Appearance)),
            superpower_path: self
                .superpower_saved_at
                .is_some()
                .then(|| drawing_file_name(Slot::Superpower)),
            appearance_saved_at: self.appearance_saved_at.clone(),
            superpower_saved_at: self.superpower_saved_at.clone(),
            stroke_counts: StrokeCounts {
                appearance: paintings.appearance.strokes.len(),
                superpower: paintings.superpower.strokes.len(),
            },
        }
    }

    /// Writes one painting and the manifest. On any failure the session's
    /// saved-state is rolled back, so the manifest and the clearing can never
    /// disagree about what is on disk.
    pub fn save(&mut self, slot: Slot, paintings: &Paintings) -> Result<PathBuf, String> {
        let png = super::paint::encode_png(paintings.sheet(slot))?;
        let path = self.directory().join(drawing_file_name(slot));
        atomic_write(&path, &png)?;

        let previous = self.saved_at(slot).map(str::to_owned);
        self.set_saved(slot, Some(iso_timestamp()));
        if let Err(error) = self.write_manifest(paintings) {
            self.set_saved(slot, previous);
            return Err(error);
        }
        Ok(path)
    }

    pub fn write_manifest(&self, paintings: &Paintings) -> Result<PathBuf, String> {
        let path = self.directory().join(MANIFEST_FILE_NAME);
        let bytes = serde_json::to_vec_pretty(&self.manifest(paintings))
            .map_err(|error| format!("serializing the manifest: {error}"))?;
        atomic_write(&path, &bytes)?;
        Ok(path)
    }
}

/// `appearance.png` / `superpower.png`.
pub fn drawing_file_name(slot: Slot) -> String {
    format!("{}.png", slot.slug())
}

pub fn iso_timestamp() -> String {
    humantime::format_rfc3339(SystemTime::now()).to_string()
}

/// Write to a temporary sibling, then rename into place, so a crash mid-write
/// never leaves a half-written painting where a good one used to be.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("the output path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("artifact");
    let temporary = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&temporary, bytes).map_err(|e| format!("writing {}: {e}", temporary.display()))?;

    // Windows refuses to rename onto an existing file, so step through a backup
    // and restore it if the replacement fails.
    #[cfg(windows)]
    if path.exists() {
        let backup = path.with_extension("bak");
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|e| format!("backing up {}: {e}", path.display()))?;
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::rename(&backup, path);
            return Err(format!("replacing {}: {error}", path.display()));
        }
        let _ = fs::remove_file(&backup);
        return Ok(());
    }
    fs::rename(&temporary, path).map_err(|e| format!("placing {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sitting-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn paintings() -> Paintings {
        Paintings::new(&mut Assets::<Image>::default())
    }

    #[test]
    fn the_manifest_uses_the_same_field_names_as_the_2d_screen() {
        let session = CreationSession::default();
        let json = serde_json::to_value(session.manifest(&paintings())).unwrap();
        let object = json.as_object().unwrap();
        for key in ["schemaVersion", "sessionId", "createdAt", "updatedAt", "canvas", "strokeCounts"] {
            assert!(object.contains_key(key), "missing {key}");
        }
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["canvas"]["width"], 1024);
        assert_eq!(json["strokeCounts"]["appearance"], 0);
        // Nothing saved yet, so no paths are claimed.
        assert!(!object.contains_key("appearancePath"));
        assert!(!object.contains_key("superpowerPath"));
    }

    #[test]
    fn saving_writes_a_transparent_png_and_a_manifest_that_points_at_it() {
        let mut session = CreationSession { root: scratch("save"), ..default() };
        let paintings = paintings();
        let path = session.save(Slot::Appearance, &paintings).expect("save succeeds");

        assert!(path.ends_with("appearance.png"));
        let decoded = image::load_from_memory(&fs::read(&path).unwrap()).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (CANVAS_PX, CANVAS_PX));
        assert!(decoded.pixels().all(|p| p[3] == 0), "an unpainted export is transparent");

        let manifest: CreationManifest =
            serde_json::from_slice(&fs::read(session.directory().join(MANIFEST_FILE_NAME)).unwrap()).unwrap();
        assert_eq!(manifest.appearance_path.as_deref(), Some("appearance.png"));
        assert!(manifest.superpower_path.is_none(), "only what was saved is listed");
        assert_eq!(manifest.session_id, session.id);
        let _ = fs::remove_dir_all(&session.root);
    }

    #[test]
    fn saving_twice_replaces_the_file_in_place() {
        let mut session = CreationSession { root: scratch("twice"), ..default() };
        let paintings = paintings();
        session.save(Slot::Superpower, &paintings).unwrap();
        session.save(Slot::Superpower, &paintings).expect("the second save also succeeds");
        let files: Vec<_> = fs::read_dir(session.directory())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(files.iter().all(|f| !f.ends_with(".tmp") && !f.ends_with(".bak")), "{files:?}");
        let _ = fs::remove_dir_all(&session.root);
    }

    #[test]
    fn session_ids_follow_the_shared_convention() {
        assert!(CreationSession::default().id.starts_with("session-"));
    }
}
