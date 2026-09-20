use crate::model::{
    create_id, RawSessionEvent, Session, SessionStatus, TranscriptSegment, TranscriptSource,
};
use bevy::prelude::*;
use std::time::Instant;

#[derive(Resource, Debug)]
pub struct CoachingSession {
    pub session: Session,
    pub playhead_ms: u64,
    pub playing: bool,
    pub generation: u64,
    pub revision: u64,
    recording_origin: Option<Instant>,
    playback_origin: Option<(Instant, u64)>,
}

impl Default for CoachingSession {
    fn default() -> Self {
        Self::from_session(Session::default())
    }
}

impl CoachingSession {
    pub fn from_session(mut session: Session) -> Self {
        if matches!(
            session.status,
            SessionStatus::Recording | SessionStatus::Interpreted
        ) {
            session.status = SessionStatus::Review;
        }
        Self {
            playhead_ms: session.elapsed_ms,
            session,
            playing: false,
            generation: 0,
            revision: 0,
            recording_origin: None,
            playback_origin: None,
        }
    }

    pub fn elapsed_now(&self) -> u64 {
        self.recording_origin
            .map(|origin| {
                self.session
                    .elapsed_ms
                    .saturating_add(origin.elapsed().as_millis() as u64)
            })
            .unwrap_or(self.session.elapsed_ms)
    }

    pub fn start_or_resume(&mut self) {
        if self.session.status == SessionStatus::Recording {
            return;
        }
        let timestamp_ms = self.session.elapsed_ms;
        let event = if self.session.status == SessionStatus::Ready {
            RawSessionEvent::RecordingStarted {
                id: create_id("event"),
                timestamp_ms,
            }
        } else {
            RawSessionEvent::RecordingResumed {
                id: create_id("event"),
                timestamp_ms,
            }
        };
        self.session.events.push(event);
        self.revision = self.revision.wrapping_add(1);
        self.session.status = SessionStatus::Recording;
        self.recording_origin = Some(Instant::now());
        self.playhead_ms = timestamp_ms;
        self.playing = false;
        self.playback_origin = None;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn stop(&mut self) {
        if self.session.status != SessionStatus::Recording {
            return;
        }
        let timestamp_ms = self.elapsed_now();
        self.session.elapsed_ms = timestamp_ms;
        self.session.events.push(RawSessionEvent::RecordingStopped {
            id: create_id("event"),
            timestamp_ms,
        });
        self.revision = self.revision.wrapping_add(1);
        self.session.status = SessionStatus::Review;
        self.recording_origin = None;
        self.playhead_ms = timestamp_ms;
        self.playing = false;
    }

    pub fn reset(&mut self) {
        let next_generation = self.generation.wrapping_add(1);
        let next_revision = self.revision.wrapping_add(1);
        *self = Self::default();
        self.generation = next_generation;
        self.revision = next_revision;
    }

    pub fn invalidate_result(&mut self) {
        if self.session.status == SessionStatus::Interpreted {
            self.session.status = SessionStatus::Review;
        }
    }

    pub fn mark_edited(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.invalidate_result();
    }

    pub fn append(&mut self, event: RawSessionEvent) {
        self.session.events.push(event);
        self.mark_edited();
    }

    pub fn add_transcript(
        &mut self,
        text: String,
        source: TranscriptSource,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
    ) {
        // Manual notes may be jotted before recording or in review; only speech
        // capture is bound to the recording clock, and that path builds its own
        // segments. An empty note is still dropped.
        if text.trim().is_empty() {
            return;
        }
        let timestamp_ms = end_ms.unwrap_or_else(|| self.elapsed_now());
        let segment = TranscriptSegment {
            id: create_id("transcript"),
            start_ms: start_ms.unwrap_or(timestamp_ms.saturating_sub(2_400)),
            end_ms: timestamp_ms,
            text: text.trim().to_owned(),
            source,
        };
        self.append(RawSessionEvent::TranscriptAdded {
            id: create_id("event"),
            timestamp_ms,
            segment,
        });
    }

    pub fn toggle_playback(&mut self) {
        if self.session.status == SessionStatus::Recording || self.session.elapsed_ms == 0 {
            return;
        }
        if self.playhead_ms >= self.session.elapsed_ms {
            self.playhead_ms = 0;
        }
        self.playing = !self.playing;
        self.playback_origin = self.playing.then(|| (Instant::now(), self.playhead_ms));
    }

    pub fn seek(&mut self, milliseconds: u64) {
        self.playhead_ms = milliseconds.min(self.session.elapsed_ms);
        self.playing = false;
        self.playback_origin = None;
    }

    pub fn tick(&mut self) {
        if self.session.status == SessionStatus::Recording {
            self.playhead_ms = self.elapsed_now();
        } else if let Some((origin, start)) = self.playback_origin {
            self.playhead_ms = start.saturating_add(origin.elapsed().as_millis() as u64);
            if self.playhead_ms >= self.session.elapsed_ms {
                self.playhead_ms = self.session.elapsed_ms;
                self.playing = false;
                self.playback_origin = None;
            }
        }
    }
}

pub fn tick_session(mut session: ResMut<CoachingSession>) {
    session.tick();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_recording_enters_review() {
        let session = Session {
            status: SessionStatus::Recording,
            ..default()
        };
        assert_eq!(
            CoachingSession::from_session(session).session.status,
            SessionStatus::Review
        );
    }

    #[test]
    fn paused_time_is_not_added_when_resuming() {
        let mut state = CoachingSession::default();
        state.start_or_resume();
        state.stop();
        let stopped_at = state.session.elapsed_ms;
        state.start_or_resume();
        assert!(state.elapsed_now() >= stopped_at);
        assert!(state.elapsed_now() < stopped_at + 100);
    }
}
