import type { Server } from "node:http";
import type { Request, Response, Router } from "express";
import { Router as createRouter } from "express";
import { WebSocket, WebSocketServer } from "ws";
import { z } from "zod";
import { tacticalOutputSchema } from "../src/domain/interpret";

const joinRequestSchema = z.object({
  role: z.enum(["host", "joiner"]),
});

const readyRequestSchema = z.object({
  role: z.enum(["host", "joiner"]),
  teamId: z.enum(["red", "yellow"]),
  tacticalOutput: tacticalOutputSchema,
});

interface LobbySlot {
  teamId: "red" | "yellow";
  tacticalOutput: unknown;
}

interface LobbyState {
  hostConnected: boolean;
  joinerConnected: boolean;
  host: LobbySlot | null;
  joiner: LobbySlot | null;
  started: boolean;
}

/**
 * One in-memory match slot: this is a LAN 1v1 hackathon app, one match at a
 * time per server process. `reset` clears it between rounds.
 */
let state: LobbyState = {
  hostConnected: false,
  joinerConnected: false,
  host: null,
  joiner: null,
  started: false,
};
const sockets = new Set<WebSocket>();

function statusPayload() {
  return {
    type: "status" as const,
    hostConnected: state.hostConnected,
    joinerConnected: state.joinerConnected,
    hostReady: state.host !== null,
    joinerReady: state.joiner !== null,
    started: state.started,
  };
}

function broadcast(payload: Record<string, unknown>): void {
  const body = JSON.stringify(payload);
  for (const socket of sockets) {
    if (socket.readyState === WebSocket.OPEN) socket.send(body);
  }
}

function broadcastStatus(): void {
  broadcast(statusPayload());
}

function startPayload() {
  if (!state.host || !state.joiner) return null;
  return {
    type: "start" as const,
    red: state.host.teamId === "red" ? state.host.tacticalOutput : state.joiner.tacticalOutput,
    yellow: state.host.teamId === "yellow" ? state.host.tacticalOutput : state.joiner.tacticalOutput,
  };
}

export function lobbyRouter(): Router {
  const router = createRouter();

  router.get("/api/lobby/status", (_request: Request, response: Response) => {
    response.json(statusPayload());
  });

  router.post("/api/lobby/join", (request: Request, response: Response) => {
    const parsed = joinRequestSchema.safeParse(request.body);
    if (!parsed.success) {
      response.status(400).json({ code: "INVALID_LOBBY_JOIN", message: "role must be host or joiner" });
      return;
    }
    if (parsed.data.role === "host") state.hostConnected = true;
    else state.joinerConnected = true;
    broadcastStatus();
    response.json(statusPayload());
  });

  router.post("/api/lobby/ready", (request: Request, response: Response) => {
    const parsed = readyRequestSchema.safeParse(request.body);
    if (!parsed.success) {
      response.status(400).json({
        code: "INVALID_LOBBY_READY",
        message: parsed.error.issues.map((issue) => issue.message).join("; "),
      });
      return;
    }
    const { role, teamId, tacticalOutput } = parsed.data;
    const slot: LobbySlot = { teamId, tacticalOutput };
    if (role === "host") state.host = slot;
    else state.joiner = slot;
    const start = startPayload();
    if (start && !state.started) {
      state.started = true;
      broadcast(start);
    } else {
      broadcastStatus();
    }
    response.json(statusPayload());
  });

  router.post("/api/lobby/start", (_request: Request, response: Response) => {
    const start = startPayload();
    if (!start) {
      response.status(409).json({ code: "LOBBY_NOT_READY", message: "Both sides must be ready before starting." });
      return;
    }
    state.started = true;
    broadcast(start);
    response.json({ ok: true });
  });

  router.post("/api/lobby/reset", (_request: Request, response: Response) => {
    state = { hostConnected: false, joinerConnected: false, host: null, joiner: null, started: false };
    broadcastStatus();
    response.json({ ok: true });
  });

  return router;
}

export function attachLobbyWebSocket(server: Server): void {
  const socketServer = new WebSocketServer({ noServer: true });

  server.on("upgrade", (request, socket, head) => {
    const url = new URL(request.url ?? "/", "http://127.0.0.1");
    if (url.pathname !== "/api/lobby") return;
    socketServer.handleUpgrade(request, socket, head, (client) => {
      socketServer.emit("connection", client, request);
    });
  });

  socketServer.on("connection", (client: WebSocket) => {
    sockets.add(client);
    client.send(JSON.stringify(statusPayload()));
    client.on("close", () => sockets.delete(client));
  });
}

/** Test-only: reset module state between test cases. */
export function resetLobbyStateForTests(): void {
  state = { hostConnected: false, joinerConnected: false, host: null, joiner: null, started: false };
  sockets.clear();
}
