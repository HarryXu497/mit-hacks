//! End-to-end coverage of the save flow against the real local store: drawings
//! are created, written, and read back with the appearance/superpower naming
//! convention intact.

use tactic_lab_player_creation::drawing::BrushTool;
use tactic_lab_player_creation::persistence::{
    CreationManifest, CreationStore, LocalFileStore, MANIFEST_FILE_NAME,
};
use tactic_lab_player_creation::state::{iso_timestamp, DrawingSlot, PlayerCreationSession};

/// Draws a slot-specific shape so the two drawings are distinguishable on disk.
fn draw(session: &mut PlayerCreationSession, slot: DrawingSlot) {
    let color = match slot {
        DrawingSlot::Appearance => [239, 71, 73, 255],
        DrawingSlot::Superpower => [46, 145, 255, 255],
    };
    let canvas = session.canvas_mut(slot);
    canvas.begin_stroke(BrushTool::Brush, color, 0.08);
    match slot {
        DrawingSlot::Appearance => {
            canvas.extend_stroke([0.2, 0.2]);
            canvas.extend_stroke([0.8, 0.8]);
        }
        DrawingSlot::Superpower => {
            canvas.extend_stroke([0.8, 0.2]);
            canvas.extend_stroke([0.2, 0.8]);
        }
    }
    canvas.end_stroke();
}

/// Mirrors what the Save button does: write the PNG, flag the slot, rewrite the
/// manifest.
fn save(
    session: &mut PlayerCreationSession,
    store: &LocalFileStore,
    slot: DrawingSlot,
) -> std::path::PathBuf {
    let session_id = session.id.clone();
    let png = session
        .canvas_mut(slot)
        .to_png()
        .expect("canvas renders to PNG");
    let path = store
        .save_drawing(&session_id, slot, &png)
        .expect("drawing is written");
    session.player.mark_saved(slot, iso_timestamp());
    store
        .write_manifest(&CreationManifest::from_session(session))
        .expect("manifest is written");
    path
}

#[test]
fn drawings_are_saved_and_reopened_with_distinct_filenames() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalFileStore::new(directory.path().to_path_buf());
    let mut session = PlayerCreationSession::default();

    draw(&mut session, DrawingSlot::Appearance);
    let appearance_path = save(&mut session, &store, DrawingSlot::Appearance);
    draw(&mut session, DrawingSlot::Superpower);
    let superpower_path = save(&mut session, &store, DrawingSlot::Superpower);

    // Every artifact is on disk under the session directory, named by slot.
    let session_directory = directory.path().join("player-creations").join(&session.id);
    for name in ["appearance.png", "superpower.png", MANIFEST_FILE_NAME] {
        assert!(
            session_directory.join(name).exists(),
            "expected {name} in the session directory"
        );
    }

    // Reopening: the PNGs decode at canvas resolution and differ per slot.
    let appearance = image::open(&appearance_path).expect("appearance reopens");
    let superpower = image::open(&superpower_path).expect("superpower reopens");
    assert_eq!(appearance.width(), 1024);
    assert_eq!(appearance.height(), 1024);
    assert_ne!(
        appearance.into_rgba8().into_raw(),
        superpower.into_rgba8().into_raw(),
        "the two slots must not export the same image"
    );

    let manifest: CreationManifest =
        serde_json::from_slice(&std::fs::read(session_directory.join(MANIFEST_FILE_NAME)).unwrap())
            .unwrap();
    assert_eq!(manifest.session_id, session.id);
    assert_eq!(manifest.appearance_path.as_deref(), Some("appearance.png"));
    assert_eq!(manifest.superpower_path.as_deref(), Some("superpower.png"));
}

#[test]
fn saving_the_same_slot_twice_overwrites_rather_than_duplicating() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalFileStore::new(directory.path().to_path_buf());
    let mut session = PlayerCreationSession::default();

    draw(&mut session, DrawingSlot::Appearance);
    let first = save(&mut session, &store, DrawingSlot::Appearance);
    let first_bytes = std::fs::read(&first).unwrap();

    // Draw more, then save again to the same slot.
    draw(&mut session, DrawingSlot::Appearance);
    let second = save(&mut session, &store, DrawingSlot::Appearance);
    assert_eq!(first, second, "the save path must be stable across saves");

    let session_directory = directory.path().join("player-creations").join(&session.id);
    let png_count = std::fs::read_dir(&session_directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .count();
    assert_eq!(
        png_count, 1,
        "re-saving must overwrite, not accumulate files"
    );
    assert_ne!(
        first_bytes,
        std::fs::read(&second).unwrap(),
        "the newer drawing must replace the older pixels"
    );

    let manifest: CreationManifest =
        serde_json::from_slice(&std::fs::read(session_directory.join(MANIFEST_FILE_NAME)).unwrap())
            .unwrap();
    assert_eq!(manifest.stroke_counts.appearance, 2);
}

#[test]
fn resetting_clears_both_slots_from_the_manifest_but_keeps_the_session_directory() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalFileStore::new(directory.path().to_path_buf());
    let mut session = PlayerCreationSession::default();

    draw(&mut session, DrawingSlot::Appearance);
    save(&mut session, &store, DrawingSlot::Appearance);
    draw(&mut session, DrawingSlot::Superpower);
    save(&mut session, &store, DrawingSlot::Superpower);

    // Reset, as the confirm dialog does, and rewrite the manifest.
    session.player.reset();
    store
        .write_manifest(&CreationManifest::from_session(&session))
        .unwrap();

    let session_directory = directory.path().join("player-creations").join(&session.id);
    let manifest: CreationManifest =
        serde_json::from_slice(&std::fs::read(session_directory.join(MANIFEST_FILE_NAME)).unwrap())
            .unwrap();

    assert!(manifest.appearance_path.is_none());
    assert!(manifest.superpower_path.is_none());
    assert!(
        session_directory.join("appearance.png").exists(),
        "resetting must not delete previously written files, only the manifest's record of them"
    );
}
