//! End-to-end coverage of the save flow against the real local store: drawings
//! are created, written, read back, and stay attached to the right player.

use tactic_lab_player_creation::drawing::BrushTool;
use tactic_lab_player_creation::persistence::{
    CreationManifest, CreationStore, LocalFileStore, MANIFEST_FILE_NAME,
};
use tactic_lab_player_creation::state::{
    iso_timestamp, DrawingSlot, PlayerCreationSession, PlayerId,
};

fn player(id: u8) -> PlayerId {
    PlayerId::new(id).unwrap()
}

/// Draws a slot-specific shape so the two drawings are distinguishable on disk.
fn draw(session: &mut PlayerCreationSession, slot: DrawingSlot) {
    let color = match slot {
        DrawingSlot::Appearance => [239, 71, 73, 255],
        DrawingSlot::Superpower => [46, 145, 255, 255],
    };
    let canvas = session.active_canvas_mut(slot);
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
    let active = session.active;
    let session_id = session.id.clone();
    let png = session
        .active_canvas_mut(slot)
        .to_png()
        .expect("canvas renders to PNG");
    let path = store
        .save_drawing(&session_id, active, slot, &png)
        .expect("drawing is written");
    session.entry_mut(active).mark_saved(slot, iso_timestamp());
    store
        .write_manifest(&CreationManifest::from_session(session))
        .expect("manifest is written");
    path
}

#[test]
fn drawings_are_saved_reopened_and_stay_with_the_right_player() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalFileStore::new(directory.path().to_path_buf());
    let mut session = PlayerCreationSession::default();

    // Player 2 (red) gets both drawings; player 8 (yellow) only an appearance.
    session.select(player(2));
    draw(&mut session, DrawingSlot::Appearance);
    let appearance_path = save(&mut session, &store, DrawingSlot::Appearance);
    draw(&mut session, DrawingSlot::Superpower);
    let superpower_path = save(&mut session, &store, DrawingSlot::Superpower);

    session.select(player(8));
    draw(&mut session, DrawingSlot::Appearance);
    save(&mut session, &store, DrawingSlot::Appearance);

    // Every artifact is on disk under the session directory.
    let session_directory = directory.path().join("player-creations").join(&session.id);
    for name in [
        "player-02-appearance.png",
        "player-02-superpower.png",
        "player-08-appearance.png",
        MANIFEST_FILE_NAME,
    ] {
        assert!(
            session_directory.join(name).exists(),
            "expected {name} in the session directory"
        );
    }
    assert!(
        !session_directory.join("player-08-superpower.png").exists(),
        "an undrawn slot must not produce a file"
    );

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

    // The manifest ties each file to the correct player and team.
    let manifest: CreationManifest =
        serde_json::from_slice(&std::fs::read(session_directory.join(MANIFEST_FILE_NAME)).unwrap())
            .unwrap();
    assert_eq!(manifest.players.len(), 10);
    assert_eq!(manifest.session_id, session.id);

    let second = manifest.entry(player(2)).unwrap();
    assert_eq!(second.team, tactic_lab_player_creation::state::Team::Red);
    assert_eq!(
        second.appearance_path.as_deref(),
        Some("player-02-appearance.png")
    );
    assert_eq!(
        second.superpower_path.as_deref(),
        Some("player-02-superpower.png")
    );

    let eighth = manifest.entry(player(8)).unwrap();
    assert_eq!(eighth.team, tactic_lab_player_creation::state::Team::Yellow);
    assert_eq!(
        eighth.appearance_path.as_deref(),
        Some("player-08-appearance.png")
    );
    assert!(eighth.superpower_path.is_none());

    // Untouched players carry no artifacts at all.
    let third = manifest.entry(player(3)).unwrap();
    assert!(third.appearance_path.is_none() && third.superpower_path.is_none());
    assert_eq!(third.stroke_counts.appearance, 0);
}

#[test]
fn saving_the_same_slot_twice_overwrites_rather_than_duplicating() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalFileStore::new(directory.path().to_path_buf());
    let mut session = PlayerCreationSession::default();

    session.select(player(5));
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
    assert_eq!(
        manifest.players.iter().filter(|e| e.player_id == 5).count(),
        1,
        "repeated saves must not duplicate the manifest entry"
    );
    assert_eq!(
        manifest.entry(player(5)).unwrap().stroke_counts.appearance,
        2
    );
}

#[test]
fn resetting_one_player_leaves_other_players_artifacts_on_disk() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalFileStore::new(directory.path().to_path_buf());
    let mut session = PlayerCreationSession::default();

    for id in [1u8, 6] {
        session.select(player(id));
        draw(&mut session, DrawingSlot::Appearance);
        save(&mut session, &store, DrawingSlot::Appearance);
    }

    // Reset player 1 and rewrite the manifest, as the confirm dialog does.
    session.entry_mut(player(1)).reset();
    store
        .write_manifest(&CreationManifest::from_session(&session))
        .unwrap();

    let session_directory = directory.path().join("player-creations").join(&session.id);
    let manifest: CreationManifest =
        serde_json::from_slice(&std::fs::read(session_directory.join(MANIFEST_FILE_NAME)).unwrap())
            .unwrap();

    assert!(manifest.entry(player(1)).unwrap().appearance_path.is_none());
    assert_eq!(
        manifest
            .entry(player(6))
            .unwrap()
            .appearance_path
            .as_deref(),
        Some("player-06-appearance.png"),
        "another player's work must survive a reset"
    );
    assert!(
        session_directory.join("player-06-appearance.png").exists(),
        "resetting one player must not delete the session"
    );
    assert_eq!(manifest.players.len(), 10);
}
