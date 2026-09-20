export type Point = { x: number; y: number };

export type TeamId = "red" | "yellow";
export type EntityRef = { kind: "player"; id: number } | { kind: "ball" };
export type SessionStatus = "ready" | "recording" | "review" | "interpreted";
export type Tool = "select" | "arrow" | "draw" | "erase";

export interface PlayerState {
  id: number;
  team: TeamId;
  position: Point;
}

export interface BoardState {
  players: PlayerState[];
  ball: Point;
  annotations: Annotation[];
}

export interface Annotation {
  id: string;
  kind: "arrow" | "freehand";
  points: Point[];
}

interface RawEventBase {
  id: string;
  timestampMs: number;
}

export interface RecordingEvent extends RawEventBase {
  type: "recording_started" | "recording_resumed" | "recording_stopped";
}

export interface MoveEvent extends RawEventBase {
  type: "entity_moved";
  entity: EntityRef;
  startedAtMs: number;
  from: Point;
  to: Point;
  path: Point[];
}

export interface AnnotationAddedEvent extends RawEventBase {
  type: "annotation_added";
  annotation: Annotation;
  startedAtMs: number;
}

export interface AnnotationRemovedEvent extends RawEventBase {
  type: "annotation_removed";
  annotationId: string;
}

export interface UndoEvent extends RawEventBase {
  type: "undo";
  targetEventId: string;
}

export interface TranscriptSegment {
  id: string;
  startMs: number;
  endMs: number;
  text: string;
  source: "speech" | "manual";
}

export interface TranscriptAddedEvent extends RawEventBase {
  type: "transcript_added";
  segment: TranscriptSegment;
}

export interface TranscriptEditedEvent extends RawEventBase {
  type: "transcript_edited";
  segmentId: string;
  text: string;
}

export type RawSessionEvent =
  | RecordingEvent
  | MoveEvent
  | AnnotationAddedEvent
  | AnnotationRemovedEvent
  | UndoEvent
  | TranscriptAddedEvent
  | TranscriptEditedEvent;

export interface Session {
  schemaVersion: 1;
  id: string;
  title: string;
  status: SessionStatus;
  elapsedMs: number;
  events: RawSessionEvent[];
  createdAt: string;
}

export interface ReplayState {
  board: BoardState;
  transcripts: TranscriptSegment[];
  effectiveEvents: RawSessionEvent[];
}
