import { randomUUID } from "node:crypto";
import type { Server } from "node:http";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { Request, Response, Router } from "express";
import { Router as createRouter } from "express";
import { WebSocket, WebSocketServer } from "ws";
import { z } from "zod";
import { tacticalOutputSchema } from "../src/domain/interpret";

const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

const joinRequestSchema = z.object({
  role: z.enum(["host", "joiner"]),
});

const readyRequestSchema = z.object({
  role: z.enum(["host", "joiner"]),
  teamId: z.enum(["red", "yellow"]),
  tacticalOutput: tacticalOutputSchema,
});

/**
 * Everything one player produced in a session. Drawings are optional so a
 * skipped creation flow can't deadlock a match, but the session log and the
 * tactical output are required — those are what downstream models need.
 */
const artifactBundleSchema = z.object({
  matchId: z.string().min(1),
  role: z.enum(["host", "joiner"]),
  teamId: z.enum(["red", "yellow"]),
  creationSessionId: z.string().optional(),
  coachingSessionId: z.string().min(1),
  session: z.object({}).passthrough(),
  tacticalOutput: tacticalOutputSchema,
  manifest: z.object({}).passthrough().optional(),
  drawings: z
    .object({
      appearance: z.string().optional(),
      superpower: z.string().optional(),
    })
    .optional(),
});

export type ArtifactBundle = z.infer<typeof artifactBundleSchema>;

interface LobbySlot {
  teamId: "red" | "yellow";
  tacticalOutput: unknown;
}

interface StoredArtifacts {
  teamId: "red" | "yellow";
  creationSessionId?: string;
  coachingSessionId: string;
  directory: string;
  files: string[];
  storedAt: string;
}

interface LobbyState {
  matchId: string;
  hostConnected: boolean;
  joinerConnected: boolean;
  host: LobbySlot | null;
  joiner: LobbySlot | null;
  artifacts: { host: StoredArtifacts | null; joiner: StoredArtifacts | null };
  started: boolean;
}

function freshState(): LobbyState {
  return {
    matchId: `match-${randomUUID()}`,
    hostConnected: false,
    joinerConnected: false,
    host: null,
    joiner: null,
    artifacts: { host: null, joiner: null },
    started: false,
  };
}

/**
 * One in-memory match slot: this is a LAN 1v1 hackathon app, one match at a
 * time per server process. `reset` clears it between rounds.
 */
let state: LobbyState = freshState();
const sockets = new Set<WebSocket>();

/**
 * Resolved from this file's location, not CWD, so the host always writes to
 * the repo's `output/` no matter where it was launched from.
 * `TACTIC_LAB_OUTPUT_DIR` overrides it (used by tests to write to a tempdir).
 */
function matchesRoot(): string {
  const root =
    process.env.TACTIC_LAB_OUTPUT_DIR ??
    path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "output");
  return path.join(root, "matches");
}

function statusPayload() {
  return {
    type: "status" as const,
    matchId: state.matchId,
    hostConnected: state.hostConnected,
    joinerConnected: state.joinerConnected,
    hostReady: state.host !== null,
    joinerReady: state.joiner !== null,
    hostArtifacts: state.artifacts.host !== null,
    joinerArtifacts: state.artifacts.joiner !== null,
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

/**
 * The match may only start once both sides have posted a tactical output AND
 * both artifact bundles are safely on the host's disk — losing a player's
 * drawings/session to a race would cost training data we can't recover.
 */
function startPayload() {
  if (!state.host || !state.joiner) return null;
  if (!state.artifacts.host || !state.artifacts.joiner) return null;
  return {
    type: "start" as const,
    matchId: state.matchId,
    red: state.host.teamId === "red" ? state.host.tacticalOutput : state.joiner.tacticalOutput,
    yellow: state.host.teamId === "yellow" ? state.host.tacticalOutput : state.joiner.tacticalOutput,
  };
}

function maybeStart(): boolean {
  const start = startPayload();
  if (!start || state.started) return false;
  state.started = true;
  broadcast(start);
  return true;
}

async function writeMatchIndex(): Promise<void> {
  const directory = path.join(matchesRoot(), state.matchId);
  await mkdir(directory, { recursive: true });
  const index = {
    matchId: state.matchId,
    updatedAt: new Date().toISOString(),
    players: {
      host: state.artifacts.host
        ? {
            teamId: state.artifacts.host.teamId,
            creationSessionId: state.artifacts.host.creationSessionId ?? null,
            coachingSessionId: state.artifacts.host.coachingSessionId,
            files: state.artifacts.host.files,
            storedAt: state.artifacts.host.storedAt,
          }
        : null,
      joiner: state.artifacts.joiner
        ? {
            teamId: state.artifacts.joiner.teamId,
            creationSessionId: state.artifacts.joiner.creationSessionId ?? null,
            coachingSessionId: state.artifacts.joiner.coachingSessionId,
            files: state.artifacts.joiner.files,
            storedAt: state.artifacts.joiner.storedAt,
          }
        : null,
    },
  };
  await writeFile(path.join(directory, "match.json"), JSON.stringify(index, null, 2));
}

function decodePng(base64: string, label: string): Buffer {
  const bytes = Buffer.from(base64, "base64");
  if (!bytes.subarray(0, PNG_MAGIC.length).equals(PNG_MAGIC)) {
    throw new Error(`${label} is not a valid PNG (bad magic bytes) — the upload was corrupted.`);
  }
  return bytes;
}

async function storeBundle(bundle: ArtifactBundle): Promise<StoredArtifacts> {
  const directory = path.join(matchesRoot(), bundle.matchId, bundle.role);
  await mkdir(directory, { recursive: true });
  const files: string[] = [];

  const write = async (name: string, contents: string | Buffer) => {
    await writeFile(path.join(directory, name), contents);
    files.push(name);
  };

  await write("session.json", JSON.stringify(bundle.session, null, 2));
  await write("tactical-output.json", JSON.stringify(bundle.tacticalOutput, null, 2));
  if (bundle.manifest) {
    await write("manifest.json", JSON.stringify(bundle.manifest, null, 2));
  }
  if (bundle.drawings?.appearance) {
    await write("appearance.png", decodePng(bundle.drawings.appearance, "appearance.png"));
  }
  if (bundle.drawings?.superpower) {
    await write("superpower.png", decodePng(bundle.drawings.superpower, "superpower.png"));
  }

  return {
    teamId: bundle.teamId,
    creationSessionId: bundle.creationSessionId,
    coachingSessionId: bundle.coachingSessionId,
    directory,
    files,
    storedAt: new Date().toISOString(),
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

  router.post("/api/lobby/artifacts", async (request: Request, response: Response) => {
    const parsed = artifactBundleSchema.safeParse(request.body);
    if (!parsed.success) {
      response.status(400).json({
        code: "INVALID_ARTIFACT_BUNDLE",
        message: parsed.error.issues.map((issue) => `${issue.path.join(".")}: ${issue.message}`).join("; "),
      });
      return;
    }
    const bundle = parsed.data;
    if (bundle.matchId !== state.matchId) {
      response.status(409).json({
        code: "STALE_MATCH_ID",
        message: `Bundle is for ${bundle.matchId}, but the current match is ${state.matchId}.`,
      });
      return;
    }
    try {
      const stored = await storeBundle(bundle);
      state.artifacts[bundle.role] = stored;
      await writeMatchIndex();
    } catch (error) {
      console.error("Failed to store artifact bundle", error);
      response.status(500).json({
        code: "ARTIFACT_WRITE_FAILED",
        message: error instanceof Error ? error.message : "Could not write the artifact bundle.",
      });
      return;
    }
    if (!maybeStart()) broadcastStatus();
    response.json({ ...statusPayload(), storedAt: state.artifacts[bundle.role]?.storedAt });
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
    // The artifact bundle must already be on disk: the match is not allowed to
    // start until both players' data is safely captured for later model runs.
    if (!state.artifacts[role]) {
      response.status(409).json({
        code: "ARTIFACTS_REQUIRED",
        message: `Upload ${role} artifacts via /api/lobby/artifacts before posting ready.`,
      });
      return;
    }
    const slot: LobbySlot = { teamId, tacticalOutput };
    if (role === "host") state.host = slot;
    else state.joiner = slot;
    if (!maybeStart()) broadcastStatus();
    response.json(statusPayload());
  });

  router.post("/api/lobby/start", (_request: Request, response: Response) => {
    const start = startPayload();
    if (!start) {
      response.status(409).json({
        code: "LOBBY_NOT_READY",
        message: "Both sides must post a tactical output and an artifact bundle before starting.",
      });
      return;
    }
    state.started = true;
    broadcast(start);
    response.json({ ok: true });
  });

  router.post("/api/lobby/reset", (_request: Request, response: Response) => {
    state = freshState();
    broadcastStatus();
    response.json(statusPayload());
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
    // `start` is broadcast exactly once. A client whose socket was mid-reconnect
    // at that instant would otherwise wait forever, so replay it on connect.
    if (state.started) {
      const start = startPayload();
      if (start) client.send(JSON.stringify(start));
    }
    client.on("close", () => sockets.delete(client));
  });
}

/** Test-only: reset module state between test cases. */
export function resetLobbyStateForTests(): void {
  state = freshState();
  sockets.clear();
}

/** Test-only: the current match id, for asserting on-disk output paths. */
export function currentMatchIdForTests(): string {
  return state.matchId;
}
