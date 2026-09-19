import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SessionTimeline } from "../components/SessionTimeline";
import { TacticsBoard } from "../components/TacticsBoard";
import { TopBar } from "../components/TopBar";
import { TranscriptPanel } from "../components/TranscriptPanel";
import { requestInterpretation } from "../domain/api";
import { interpretSession, type TacticalOutput } from "../domain/interpret";
import { clearSavedSession, loadSession, saveSession } from "../domain/persistence";
import { findUndoTarget, replaySession } from "../domain/replay";
import { createId, createSession } from "../domain/session";
import { formatTime } from "../domain/time";
import type {
  Annotation,
  AnnotationAddedEvent,
  AnnotationRemovedEvent,
  EntityRef,
  MoveEvent,
  Point,
  RawSessionEvent,
  Session,
  Tool,
  UndoEvent,
} from "../domain/types";
import { useSpeechRecognition } from "../hooks/useSpeechRecognition";

function restoreSession(): Session {
  const restored = loadSession();
  if (!restored) return createSession();
  if (restored.status === "recording" || restored.status === "interpreted") {
    return { ...restored, status: "review" };
  }
  return restored;
}

export function App() {
  const [session, setSession] = useState<Session>(restoreSession);
  const [tool, setTool] = useState<Tool>("arrow");
  const [playheadMs, setPlayheadMs] = useState(session.elapsedMs);
  const [isPlaying, setIsPlaying] = useState(false);
  const [result, setResult] = useState<TacticalOutput | null>(null);
  const [isInterpreting, setIsInterpreting] = useState(false);
  const [interpretationNotice, setInterpretationNotice] = useState<string | null>(null);
  const recordingOriginRef = useRef<number | null>(null);
  const sessionRef = useRef(session);

  sessionRef.current = session;

  const elapsedNow = useCallback(() => {
    if (recordingOriginRef.current === null) return sessionRef.current.elapsedMs;
    return Math.max(0, Math.round(performance.now() - recordingOriginRef.current));
  }, []);

  useEffect(() => saveSession(session), [session]);

  useEffect(() => {
    if (session.status !== "recording") return;
    if (recordingOriginRef.current === null) {
      recordingOriginRef.current = performance.now() - session.elapsedMs;
    }
    const timer = window.setInterval(() => {
      const elapsedMs = elapsedNow();
      setSession((current) => ({ ...current, elapsedMs }));
      setPlayheadMs(elapsedMs);
    }, 100);
    return () => window.clearInterval(timer);
  }, [elapsedNow, session.elapsedMs, session.status]);

  useEffect(() => {
    if (!isPlaying) return;
    let previous = performance.now();
    const timer = window.setInterval(() => {
      const now = performance.now();
      const delta = now - previous;
      previous = now;
      setPlayheadMs((current) => {
        const next = current + delta;
        if (next >= sessionRef.current.elapsedMs) {
          setIsPlaying(false);
          return sessionRef.current.elapsedMs;
        }
        return next;
      });
    }, 32);
    return () => window.clearInterval(timer);
  }, [isPlaying]);

  const append = useCallback((event: RawSessionEvent) => {
    setSession((current) => ({ ...current, events: [...current.events, event] }));
  }, []);

  const addTranscript = useCallback(
    (text: string, source: "speech" | "manual") => {
      if (sessionRef.current.status !== "recording") return;
      const timestampMs = elapsedNow();
      append({
        id: createId("event"),
        type: "transcript_added",
        timestampMs,
        segment: {
          id: createId("transcript"),
          startMs: Math.max(0, timestampMs - 2400),
          endMs: timestampMs,
          text,
          source,
        },
      });
    },
    [append, elapsedNow],
  );

  const speech = useSpeechRecognition(session.status === "recording", (text) =>
    addTranscript(text, "speech"),
  );

  const visibleUntil =
    session.status === "review" || session.status === "interpreted" ? playheadMs : Infinity;
  const replay = useMemo(
    () => replaySession(session.events, visibleUntil),
    [session.events, visibleUntil],
  );
  const fullReplay = useMemo(() => replaySession(session.events), [session.events]);
  const canUndo = Boolean(findUndoTarget(session.events));

  const startOrStop = () => {
    if (session.status === "recording") {
      const timestampMs = elapsedNow();
      recordingOriginRef.current = null;
      setSession((current) => ({
        ...current,
        status: "review",
        elapsedMs: timestampMs,
        events: [
          ...current.events,
          { id: createId("event"), type: "recording_stopped", timestampMs },
        ],
      }));
      setPlayheadMs(timestampMs);
      setIsPlaying(false);
      return;
    }

    const timestampMs = session.elapsedMs;
    const type = session.status === "ready" ? "recording_started" : "recording_resumed";
    recordingOriginRef.current = performance.now() - timestampMs;
    setResult(null);
    setInterpretationNotice(null);
    setSession((current) => ({
      ...current,
      status: "recording",
      events: [...current.events, { id: createId("event"), type, timestampMs }],
    }));
    setPlayheadMs(timestampMs);
  };

  const reset = () => {
    if (!window.confirm("Reset this session and clear its recorded events?")) return;
    const next = createSession();
    recordingOriginRef.current = null;
    clearSavedSession();
    setSession(next);
    setResult(null);
    setPlayheadMs(0);
    setIsPlaying(false);
    setTool("arrow");
    setInterpretationNotice(null);
  };

  type TimestampedBoardEvent =
    | Omit<MoveEvent, "id" | "timestampMs">
    | Omit<AnnotationAddedEvent, "id" | "timestampMs">
    | Omit<AnnotationRemovedEvent, "id" | "timestampMs">
    | Omit<UndoEvent, "id" | "timestampMs">;

  const timestampedAppend = (event: TimestampedBoardEvent) => {
    if (sessionRef.current.status !== "recording") return;
    append({
      ...event,
      id: createId("event"),
      timestampMs: elapsedNow(),
    } as RawSessionEvent);
  };

  const generateJson = async () => {
    if (isInterpreting) return;
    setIsInterpreting(true);
    setInterpretationNotice(null);
    const currentSession = sessionRef.current;
    try {
      setResult(await requestInterpretation(currentSession));
    } catch (error) {
      console.error("Model-backed interpretation failed; using deterministic fallback.", error);
      setResult(interpretSession(currentSession, "deterministic-fallback"));
      setInterpretationNotice(
        "The model service was unavailable, so this result uses the offline deterministic fallback.",
      );
    } finally {
      setSession((current) => ({ ...current, status: "interpreted" }));
      setPlayheadMs(currentSession.elapsedMs);
      setIsInterpreting(false);
    }
  };

  const activeTranscriptId = replay.transcripts.find(
    (segment) => playheadMs >= segment.startMs && playheadMs <= segment.endMs,
  )?.id;

  return (
    <main className="app-shell">
      <TopBar
        title={session.title}
        status={session.status}
        elapsedLabel={formatTime(session.elapsedMs)}
        onTitleChange={(title) => setSession((current) => ({ ...current, title }))}
        onPrimaryAction={startOrStop}
        onReset={reset}
      />

      <div className="workspace">
        <TacticsBoard
          board={replay.board}
          tool={tool}
          interactive={session.status === "recording"}
          canUndo={canUndo}
          elapsedMs={session.elapsedMs}
          onToolChange={setTool}
          onMove={(entity: EntityRef, from: Point, to: Point, path: Point[], startedAtMs) =>
            timestampedAppend({ type: "entity_moved", entity, from, to, path, startedAtMs })
          }
          onAnnotationAdd={(annotation: Annotation, startedAtMs) =>
            timestampedAppend({ type: "annotation_added", annotation, startedAtMs })
          }
          onAnnotationRemove={(annotationId) =>
            timestampedAppend({ type: "annotation_removed", annotationId })
          }
          onUndo={() => {
            const target = findUndoTarget(sessionRef.current.events);
            if (target) timestampedAppend({ type: "undo", targetEventId: target.id });
          }}
        />

        <TranscriptPanel
          transcripts={fullReplay.transcripts}
          activeTranscriptId={activeTranscriptId}
          status={session.status}
          speechSupported={speech.supported}
          listening={speech.listening}
          result={result}
          isInterpreting={isInterpreting}
          interpretationNotice={interpretationNotice}
          onManualAdd={(text) => addTranscript(text, "manual")}
          onEdit={(segmentId, text) =>
            append({
              id: createId("event"),
              type: "transcript_edited",
              segmentId,
              text,
              timestampMs: elapsedNow(),
            })
          }
          onGenerate={generateJson}
          onBackToTranscript={() => {
            setResult(null);
            setInterpretationNotice(null);
            setSession((current) => ({ ...current, status: "review" }));
          }}
        />
      </div>

      <SessionTimeline
        events={session.events}
        transcripts={fullReplay.transcripts}
        elapsedMs={session.elapsedMs}
        playheadMs={playheadMs}
        status={session.status}
        isPlaying={isPlaying}
        onPlayToggle={() => {
          if (playheadMs >= session.elapsedMs) setPlayheadMs(0);
          setIsPlaying((value) => !value);
        }}
        onSeek={(milliseconds) => {
          setIsPlaying(false);
          setPlayheadMs(Math.min(milliseconds, session.elapsedMs));
        }}
      />
    </main>
  );
}
