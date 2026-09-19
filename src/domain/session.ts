import type { BoardState, Point, RawSessionEvent, Session } from "./types";

const uid = (prefix: string) =>
  `${prefix}-${typeof crypto !== "undefined" && crypto.randomUUID ? crypto.randomUUID() : Math.random().toString(36).slice(2)}`;

export const createId = uid;

export const clampPoint = (point: Point): Point => ({
  x: Math.min(1, Math.max(0, point.x)),
  y: Math.min(1, Math.max(0, point.y)),
});

export const initialBoard: BoardState = {
  players: [
    { id: 1, team: "red", position: { x: 0.5, y: 0.08 } },
    { id: 2, team: "red", position: { x: 0.3, y: 0.23 } },
    { id: 3, team: "red", position: { x: 0.7, y: 0.23 } },
    { id: 4, team: "red", position: { x: 0.28, y: 0.4 } },
    { id: 5, team: "red", position: { x: 0.72, y: 0.4 } },
    { id: 6, team: "yellow", position: { x: 0.28, y: 0.6 } },
    { id: 7, team: "yellow", position: { x: 0.72, y: 0.6 } },
    { id: 8, team: "yellow", position: { x: 0.3, y: 0.77 } },
    { id: 9, team: "yellow", position: { x: 0.7, y: 0.77 } },
    { id: 10, team: "yellow", position: { x: 0.5, y: 0.92 } },
  ],
  ball: { x: 0.5, y: 0.51 },
  annotations: [],
};

export const createSession = (): Session => ({
  schemaVersion: 1,
  id: uid("session"),
  title: "Build-up pattern",
  status: "ready",
  elapsedMs: 0,
  events: [],
  createdAt: new Date().toISOString(),
});

export const appendEvent = (session: Session, event: RawSessionEvent): Session => ({
  ...session,
  events: [...session.events, event],
});
