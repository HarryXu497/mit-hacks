//! The in-world tactics table, on this crate's session model.
//!
//! `cube_soccer::tactics` is Jeremy Liu's stone table: an upright board on the clearing's island,
//! with ten tokens, a ball, arrows and a timber sign. It records everything the coach does onto
//! its own flat, timestamped log — deliberately mirroring the 2D screen's, down to the normalized
//! 0..1 point convention — and it deliberately owns neither speech nor interpretation, because
//! those need a microphone and a network service. Its doc comment says as much: both arrive
//! through `Transcript` and `Interpretation`, "which a host fills in".
//!
//! This module is that host. It translates the table's log into [`crate::model::Session`], which
//! is what everything downstream already consumes: `replay_session`, `validate_contract`,
//! `/api/interpret`, `game_handoff`, and the artifact bundle the host collects before a match.
//! Nothing else had to learn about the table.
//!
//! Two fields the table does not record, and what they become here:
//!
//! * `EntityMoved::path` — the table logs a drag's endpoints, not its intermediate samples, so
//!   the path is the straight segment between them. Never misleading: two points *is* the path,
//!   as far as the log knows. (Nothing downstream reads it today; only a replay fixture does.)
//! * `TranscriptSegment::{start_ms, end_ms}` — a sentence lands on the session clock at one
//!   instant, so both are that instant.

use crate::model::{
    Annotation, AnnotationKind, EntityRef, Point, RawSessionEvent, Session, SessionStatus,
    TranscriptSegment, TranscriptSource,
};
use cube_soccer::tactics::{self, EntityRef as TableEntity, RawEvent};

/// The table's normalized board point as a session point.
///
/// Both are already 0..1 over the same board, so this is a rename — but it clamps anyway, because
/// `Session::validate_contract` rejects an out-of-range coordinate outright and a drag released
/// a pixel off the board's edge should not cost the coach their whole session.
fn point(p: tactics::Point) -> Point {
    Point { x: p.x, y: p.y }.clamped()
}

/// One of the ten fixed players, or the ball.
fn entity(e: TableEntity) -> EntityRef {
    match e {
        TableEntity::Player(id) => EntityRef::Player { id },
        TableEntity::Ball => EntityRef::Ball,
    }
}

/// Translate the table's log, in order, into raw session events.
///
/// Event ids are derived from position in the log rather than freshly generated, so translating
/// the same log twice gives the same ids. That matters: the server grounds an interpretation
/// against the ids it was sent and rejects any it did not see, so an id that changed between the
/// interpret call and the upload would fail validation for no reason.
pub fn events_from_table(table: &tactics::Session) -> Vec<RawSessionEvent> {
    table
        .events
        .iter()
        .enumerate()
        .map(|(index, event)| {
            let id = format!("ev-{index}");
            match event {
                RawEvent::RecordingStarted { at_ms } => RawSessionEvent::RecordingStarted {
                    id,
                    timestamp_ms: *at_ms,
                },
                RawEvent::RecordingStopped { at_ms } => RawSessionEvent::RecordingStopped {
                    id,
                    timestamp_ms: *at_ms,
                },
                RawEvent::EntityMoved {
                    entity: who,
                    from,
                    to,
                    started_at_ms,
                    at_ms,
                } => {
                    let (from, to) = (point(*from), point(*to));
                    RawSessionEvent::EntityMoved {
                        id,
                        timestamp_ms: *at_ms,
                        entity: entity(*who),
                        started_at_ms: *started_at_ms,
                        from,
                        to,
                        path: vec![from, to],
                    }
                }
                RawEvent::AnnotationAdded {
                    id: mark,
                    points,
                    arrow,
                    at_ms,
                } => RawSessionEvent::AnnotationAdded {
                    id,
                    timestamp_ms: *at_ms,
                    started_at_ms: *at_ms,
                    annotation: Annotation {
                        id: format!("ann-{mark}"),
                        kind: if *arrow {
                            AnnotationKind::Arrow
                        } else {
                            AnnotationKind::Freehand
                        },
                        points: points.iter().copied().map(point).collect(),
                    },
                },
                RawEvent::AnnotationRemoved { id: mark, at_ms } => {
                    RawSessionEvent::AnnotationRemoved {
                        id,
                        timestamp_ms: *at_ms,
                        annotation_id: format!("ann-{mark}"),
                    }
                }
                RawEvent::TranscriptAdded {
                    id: segment,
                    text,
                    at_ms,
                } => RawSessionEvent::TranscriptAdded {
                    id,
                    timestamp_ms: *at_ms,
                    segment: TranscriptSegment {
                        id: format!("seg-{segment}"),
                        start_ms: *at_ms,
                        end_ms: *at_ms,
                        text: text.clone(),
                        // The table has no microphone; every sentence on its log was pushed in by
                        // this crate's `speech.rs` from the transcription service.
                        source: TranscriptSource::Speech,
                    },
                },
            }
        })
        .collect()
}

/// What the table's log says the session's status is.
///
/// Not applied by [`sync_into`]: the coaching panel's own Record/Stop button owns `status`, and
/// the interpretation flow moves it on to `Interpreted`, which `handle_enter_game` requires
/// before it will start a match. Overwriting it from the table would undo both. Kept public
/// because it is the honest answer to "what does the log say happened".
pub fn status_of(table: &tactics::Session) -> SessionStatus {
    let started = table
        .events
        .iter()
        .any(|e| matches!(e, RawEvent::RecordingStarted { .. }));
    let stopped = table
        .events
        .iter()
        .any(|e| matches!(e, RawEvent::RecordingStopped { .. }));
    match (started, stopped) {
        (_, true) => SessionStatus::Review,
        (true, false) => SessionStatus::Recording,
        (false, false) => SessionStatus::Ready,
    }
}

/// Fold the table's current log into an existing session, leaving its identity alone.
///
/// The session's `id`, `title` and `created_at` are deliberately untouched: `rlSelection.sessionId`
/// has to equal `session.id` for the handoff to ground, and the lobby correlates a machine's
/// upload by that same id. `status` is left alone too — see [`status_of`]. Only what the coach did
/// on the board is replaced.
pub fn sync_into(table: &tactics::Session, session: &mut Session) {
    // What the panel wrote is not on the table's log and never will be, so a wholesale
    // replacement destroyed it. While the clock runs the table changes every frame, which is
    // every frame this mirror runs -- so a character typed into a transcript line reverted a
    // frame later, and a manual note (only addable while recording) never survived at all.
    // Kept in their existing order and with their existing ids, appended after the table's:
    // `replay_session` applies an edit to a segment an earlier event introduced, and the
    // server grounds an interpretation against the ids it was sent.
    let panel_authored: Vec<RawSessionEvent> = session
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                RawSessionEvent::TranscriptEdited { .. }
                    | RawSessionEvent::Undo { .. }
                    | RawSessionEvent::TranscriptAdded {
                        segment: TranscriptSegment { source: TranscriptSource::Manual, .. },
                        ..
                    }
            )
        })
        .cloned()
        .collect();
    session.events = events_from_table(table);
    session.events.extend(panel_authored);
    session.elapsed_ms = table.elapsed_ms;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    fn table_with(events: Vec<RawEvent>, elapsed_ms: u64) -> tactics::Session {
        let mut table = tactics::Session::default();
        for event in events {
            table.append(event);
        }
        table.elapsed_ms = elapsed_ms;
        table
    }

    #[test]
    fn an_empty_table_is_a_ready_session_that_still_validates() {
        let table = table_with(vec![], 0);
        let mut session = Session::default();
        sync_into(&table, &mut session);
        assert!(session.events.is_empty());
        assert_eq!(status_of(&table), SessionStatus::Ready);
        session.validate_contract().expect("an empty session is legal");
    }

    #[test]
    fn a_translated_session_satisfies_the_contract_the_server_enforces() {
        let table = table_with(
            vec![
                RawEvent::RecordingStarted { at_ms: 0 },
                RawEvent::EntityMoved {
                    entity: TableEntity::Player(7),
                    from: Vec2::new(0.3, 0.2),
                    to: Vec2::new(0.55, 0.6),
                    started_at_ms: 100,
                    at_ms: 900,
                },
                RawEvent::AnnotationAdded {
                    id: 1,
                    points: vec![Vec2::new(0.1, 0.1), Vec2::new(0.4, 0.5)],
                    arrow: true,
                    at_ms: 1200,
                },
                RawEvent::TranscriptAdded {
                    id: 2,
                    text: "push up on the left".to_owned(),
                    at_ms: 1500,
                },
                RawEvent::RecordingStopped { at_ms: 2000 },
            ],
            2000,
        );
        let mut session = Session::default();
        sync_into(&table, &mut session);

        session
            .validate_contract()
            .expect("the table's own convention is already the session's");
        assert_eq!(session.events.len(), 5);
        assert_eq!(session.elapsed_ms, 2000);
        assert_eq!(status_of(&table), SessionStatus::Review);
    }

    #[test]
    fn a_sync_never_touches_the_status_the_panel_owns() {
        let mut session = Session {
            status: SessionStatus::Interpreted,
            ..Default::default()
        };
        // A stopped table would report `Review`; the session must stay `Interpreted`, because
        // that is what `handle_enter_game` requires before it will start a match.
        sync_into(
            &table_with(
                vec![
                    RawEvent::RecordingStarted { at_ms: 0 },
                    RawEvent::RecordingStopped { at_ms: 5 },
                ],
                5,
            ),
            &mut session,
        );
        assert_eq!(session.status, SessionStatus::Interpreted);
    }

    #[test]
    fn the_session_keeps_its_identity_across_a_sync() {
        let mut session = Session::default();
        let (id, created_at, title) = (
            session.id.clone(),
            session.created_at.clone(),
            session.title.clone(),
        );
        sync_into(&table_with(vec![RawEvent::RecordingStarted { at_ms: 0 }], 5), &mut session);
        // rlSelection.sessionId must keep matching session.id, and the lobby correlates the
        // upload by it -- so a sync must never re-mint the session.
        assert_eq!(session.id, id);
        assert_eq!(session.created_at, created_at);
        assert_eq!(session.title, title);
    }

    /// What the coach typed into the panel must survive the next mirror.
    ///
    /// The table changes every frame while the clock runs, so the mirror runs every frame.
    /// While `sync_into` replaced the event list wholesale, a character typed into a
    /// transcript row reverted a frame later and a manual note -- which can only be added
    /// while recording, i.e. only while the mirror is running -- never survived at all.
    #[test]
    fn what_the_panel_wrote_survives_a_sync_with_its_ids_intact() {
        let table = table_with(
            vec![RawEvent::TranscriptAdded {
                id: 3,
                text: "push up on the left".to_owned(),
                at_ms: 1_000,
            }],
            2_000,
        );
        let mut session = Session::default();
        sync_into(&table, &mut session);

        session.events.push(RawSessionEvent::TranscriptAdded {
            id: "event-manual".to_owned(),
            timestamp_ms: 1_500,
            segment: TranscriptSegment {
                id: "transcript-manual".to_owned(),
                start_ms: 1_500,
                end_ms: 1_500,
                text: "watch the far post".to_owned(),
                source: TranscriptSource::Manual,
            },
        });
        session.events.push(RawSessionEvent::TranscriptEdited {
            id: "event-edit".to_owned(),
            timestamp_ms: 1_800,
            segment_id: "seg-3".to_owned(),
            text: "push up on the RIGHT".to_owned(),
        });

        // Twice, because the mirror runs on every frame of a recording.
        sync_into(&table, &mut session);
        sync_into(&table, &mut session);

        let ids: Vec<&str> = session.events.iter().map(|e| e.id()).collect();
        assert_eq!(
            ids,
            vec!["ev-0", "event-manual", "event-edit"],
            "the panel's events survive exactly once, after the table's, with their ids"
        );

        // And the edit still lands: it is replayed after the segment it names.
        let replayed = crate::replay::replay_session(&session.events, None);
        let said: Vec<&str> = replayed.transcripts.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(said, vec!["push up on the RIGHT", "watch the far post"]);
    }

    #[test]
    fn translating_twice_gives_identical_ids() {
        let table = table_with(
            vec![
                RawEvent::RecordingStarted { at_ms: 0 },
                RawEvent::AnnotationAdded {
                    id: 4,
                    points: vec![Vec2::new(0.2, 0.2), Vec2::new(0.3, 0.3)],
                    arrow: false,
                    at_ms: 10,
                },
            ],
            10,
        );
        let first: Vec<String> = events_from_table(&table)
            .iter()
            .map(|e| e.id().to_owned())
            .collect();
        let second: Vec<String> = events_from_table(&table)
            .iter()
            .map(|e| e.id().to_owned())
            .collect();
        assert_eq!(first, second, "ids the server grounds against must be stable");
    }

    #[test]
    fn the_pen_and_the_arrow_stay_distinguishable() {
        let table = table_with(
            vec![
                RawEvent::AnnotationAdded {
                    id: 1,
                    points: vec![Vec2::new(0.1, 0.1), Vec2::new(0.2, 0.2)],
                    arrow: true,
                    at_ms: 0,
                },
                RawEvent::AnnotationAdded {
                    id: 2,
                    points: vec![Vec2::new(0.3, 0.3), Vec2::new(0.4, 0.4)],
                    arrow: false,
                    at_ms: 1,
                },
                RawEvent::AnnotationRemoved { id: 1, at_ms: 2 },
            ],
            2,
        );
        let events = events_from_table(&table);
        let kinds: Vec<AnnotationKind> = events
            .iter()
            .filter_map(|e| match e {
                RawSessionEvent::AnnotationAdded { annotation, .. } => Some(annotation.kind),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, vec![AnnotationKind::Arrow, AnnotationKind::Freehand]);

        // A removal has to name the same annotation id the addition used, or the replay cannot
        // take the mark back off the board.
        let removed = events.iter().find_map(|e| match e {
            RawSessionEvent::AnnotationRemoved { annotation_id, .. } => Some(annotation_id.clone()),
            _ => None,
        });
        assert_eq!(removed.as_deref(), Some("ann-1"));
    }

    #[test]
    fn a_point_dragged_off_the_board_is_clamped_rather_than_rejected() {
        let table = table_with(
            vec![RawEvent::EntityMoved {
                entity: TableEntity::Player(1),
                from: Vec2::new(0.5, 0.5),
                to: Vec2::new(1.4, -0.3),
                started_at_ms: 0,
                at_ms: 10,
            }],
            10,
        );
        let mut session = Session::default();
        sync_into(&table, &mut session);
        session
            .validate_contract()
            .expect("a drag past the edge should not invalidate the session");
    }

    #[test]
    fn every_player_the_table_can_move_is_inside_the_fixed_roster() {
        // The board has exactly ten tokens, ids 1-10, and the contract rejects anything else.
        for id in 1..=10u8 {
            let table = table_with(
                vec![RawEvent::EntityMoved {
                    entity: TableEntity::Player(id),
                    from: Vec2::new(0.5, 0.5),
                    to: Vec2::new(0.6, 0.6),
                    started_at_ms: 0,
                    at_ms: 1,
                }],
                1,
            );
            let mut session = Session::default();
            sync_into(&table, &mut session);
            session
                .validate_contract()
                .unwrap_or_else(|e| panic!("player {id} should be legal: {e}"));
        }
    }
}
