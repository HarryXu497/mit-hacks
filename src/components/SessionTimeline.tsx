import { Pause, Play } from "lucide-react";
import type { RawSessionEvent, SessionStatus, TranscriptSegment } from "../domain/types";
import { formatTime } from "../domain/time";

interface SessionTimelineProps {
  events: RawSessionEvent[];
  transcripts: TranscriptSegment[];
  elapsedMs: number;
  playheadMs: number;
  status: SessionStatus;
  isPlaying: boolean;
  onPlayToggle: () => void;
  onSeek: (milliseconds: number) => void;
}

const isBoardEvent = (event: RawSessionEvent) =>
  event.type === "entity_moved" ||
  event.type === "annotation_added" ||
  event.type === "annotation_removed" ||
  event.type === "undo";

export function SessionTimeline({
  events,
  transcripts,
  elapsedMs,
  playheadMs,
  status,
  isPlaying,
  onPlayToggle,
  onSeek,
}: SessionTimelineProps) {
  const duration = Math.max(40_000, Math.ceil(elapsedMs / 10_000) * 10_000 || 40_000);
  const boardEvents = events.filter(isBoardEvent);
  const reviewable = status === "review" || status === "interpreted";
  const percent = Math.min(100, (playheadMs / duration) * 100);
  const ticks = Array.from({ length: duration / 5000 + 1 }, (_, index) => index * 5000);

  return (
    <section className="timeline" aria-label="Session timeline">
      <div className="timeline-heading">
        <div className="timeline-title-wrap">
          <button
            className="timeline-play"
            onClick={onPlayToggle}
            disabled={!reviewable || elapsedMs === 0}
            aria-label={isPlaying ? "Pause replay" : "Play replay"}
          >
            {isPlaying ? <Pause size={14} fill="currentColor" /> : <Play size={14} fill="currentColor" />}
          </button>
          <h2>Session timeline</h2>
        </div>
        <span>{boardEvents.length} board events · {transcripts.length} transcript segments</span>
      </div>
      <div className="timeline-grid">
        <div className="timeline-label-spacer" />
        <div className="timeline-ruler">
          {ticks.map((tick) => (
            <span key={tick} style={{ left: `${(tick / duration) * 100}%` }}>
              <i />
              {tick % 10_000 === 0 ? <time>{formatTime(tick)}</time> : null}
            </span>
          ))}
        </div>
        <div className="timeline-label">Board events</div>
        <div className="timeline-track board-track">
          {boardEvents.map((event) => (
            <span
              key={event.id}
              className="event-tick"
              style={{ left: `${Math.min(100, (event.timestampMs / duration) * 100)}%` }}
              title={`${event.type} at ${formatTime(event.timestampMs)}`}
            />
          ))}
        </div>
        <div className="timeline-label">Transcript</div>
        <div className="timeline-track transcript-track">
          {transcripts.map((segment) => (
            <span
              key={segment.id}
              className="transcript-segment"
              style={{
                left: `${(segment.startMs / duration) * 100}%`,
                width: `${Math.max(1.5, ((segment.endMs - segment.startMs) / duration) * 100)}%`,
              }}
              title={segment.text}
            />
          ))}
        </div>
        <div className="playhead" style={{ left: `calc(112px + (100% - 112px) * ${percent / 100})` }}>
          <span>{formatTime(playheadMs)}</span>
          <i />
        </div>
        <input
          className="timeline-scrubber"
          type="range"
          min="0"
          max={duration}
          step="50"
          value={Math.min(playheadMs, duration)}
          disabled={!reviewable}
          onChange={(event) => onSeek(Number(event.target.value))}
          aria-label="Session playhead"
        />
      </div>
    </section>
  );
}
