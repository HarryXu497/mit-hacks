import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import request from "supertest";
import type { Express } from "express";
import { createApp } from "./app";
import { currentMatchIdForTests, resetLobbyStateForTests } from "./lobby";
import type { TacticalOutput } from "../src/domain/interpret";

/** A real 1x1 PNG so the server's magic-byte validation is exercised. */
const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

function postBundle(app: Express, role: "host" | "joiner") {
  const teamId = role === "host" ? "red" : "yellow";
  return request(app)
    .post("/api/lobby/artifacts")
    .send({
      matchId: currentMatchIdForTests(),
      role,
      teamId,
      creationSessionId: `creation-${role}`,
      coachingSessionId: `coaching-${role}`,
      session: { schemaVersion: 1, id: `coaching-${role}`, events: [] },
      tacticalOutput: fixtureOutput(teamId, `coaching-${role}`),
      manifest: { schemaVersion: 1, sessionId: `creation-${role}` },
      drawings: {
        appearance: TINY_PNG.toString("base64"),
        superpower: TINY_PNG.toString("base64"),
      },
    });
}

function postReady(app: Express, role: "host" | "joiner") {
  const teamId = role === "host" ? "red" : "yellow";
  return request(app)
    .post("/api/lobby/ready")
    .send({ role, teamId, tacticalOutput: fixtureOutput(teamId, `coaching-${role}`) });
}

function fixtureOutput(teamId: "red" | "yellow", sessionId = "s"): TacticalOutput {
  return {
    schemaVersion: "2.0",
    taxonomyVersion: "tactics-v2",
    interpretationMode: "deterministic-preview",
    session: { id: sessionId, title: "t", durationMs: 0 },
    teams: [
      { id: "red", playerIds: [1, 2, 3, 4, 5] },
      { id: "yellow", playerIds: [6, 7, 8, 9, 10] },
    ],
    classification: {
      primaryTactic: "balanced",
      secondaryTraits: [],
      alternativeTactics: [],
      selectionReason: "system_fallback",
      evidenceStrength: "weak",
      explanation: "test fixture",
    },
    summary: { name: "t", objective: "test" },
    steps: [],
    finalState: { players: [], ball: { x: 0.5, y: 0.5 }, annotations: [] },
    rlSelection: {
      schemaVersion: "2.0",
      taxonomyVersion: "tactics-v2",
      sessionId,
      primaryTactic: "balanced",
      downstreamValue: "balanced",
      teamId,
      playerOverrides: [],
      selectionReason: "system_fallback",
      evidenceStrength: "weak",
    },
  };
}

describe("lobby endpoints", () => {
  let outputDir: string;

  beforeEach(() => {
    outputDir = mkdtempSync(path.join(tmpdir(), "tactic-lab-lobby-"));
    process.env.TACTIC_LAB_OUTPUT_DIR = outputDir;
  });

  afterEach(() => {
    resetLobbyStateForTests();
    delete process.env.TACTIC_LAB_OUTPUT_DIR;
    rmSync(outputDir, { recursive: true, force: true });
  });

  it("reports not-ready until both sides post", async () => {
    const app = createApp();
    const initial = await request(app).get("/api/lobby/status");
    expect(initial.body).toMatchObject({ hostReady: false, joinerReady: false });

    await postBundle(app, "host").expect(200);
    await postReady(app, "host").expect(200);

    const afterHost = await request(app).get("/api/lobby/status");
    expect(afterHost.body).toMatchObject({ hostReady: true, joinerReady: false });

    await postBundle(app, "joiner").expect(200);
    await postReady(app, "joiner").expect(200);

    const afterBoth = await request(app).get("/api/lobby/status");
    expect(afterBoth.body).toMatchObject({ hostReady: true, joinerReady: true });
  });

  it("refuses to start until both sides are ready", async () => {
    const app = createApp();
    await request(app).post("/api/lobby/start").expect(409);
  });

  it("tracks connection before either side has a tactical output", async () => {
    const app = createApp();
    await request(app).post("/api/lobby/join").send({ role: "host" }).expect(200);
    const status = await request(app).get("/api/lobby/status");
    expect(status.body).toMatchObject({ hostConnected: true, joinerConnected: false });
  });

  it("auto-starts once both sides have uploaded and posted ready", async () => {
    const app = createApp();
    await postBundle(app, "host").expect(200);
    await postReady(app, "host").expect(200);
    const afterOne = await request(app).get("/api/lobby/status");
    expect(afterOne.body.started).toBe(false);

    await postBundle(app, "joiner").expect(200);
    const afterTwo = await postReady(app, "joiner");
    expect(afterTwo.body.started).toBe(true);
  });

  it("rejects a malformed ready payload", async () => {
    const app = createApp();
    await request(app)
      .post("/api/lobby/ready")
      .send({ role: "host", teamId: "purple", tacticalOutput: {} })
      .expect(400);
  });

  it("refuses ready until that side's artifacts are stored", async () => {
    const app = createApp();
    const rejected = await postReady(app, "host").expect(409);
    expect(rejected.body.code).toBe("ARTIFACTS_REQUIRED");

    await postBundle(app, "host").expect(200);
    await postReady(app, "host").expect(200);
  });

  it("writes both players' artifacts into one match folder", async () => {
    const app = createApp();
    const matchId = currentMatchIdForTests();
    await postBundle(app, "host").expect(200);
    await postBundle(app, "joiner").expect(200);

    const matchDir = path.join(outputDir, "matches", matchId);
    for (const role of ["host", "joiner"] as const) {
      for (const file of [
        "session.json",
        "tactical-output.json",
        "manifest.json",
        "appearance.png",
        "superpower.png",
      ]) {
        expect(readFileSync(path.join(matchDir, role, file)).length).toBeGreaterThan(0);
      }
      // Byte-for-byte: catches base64 corruption in the upload path.
      expect(readFileSync(path.join(matchDir, role, "appearance.png"))).toEqual(TINY_PNG);
    }

    const index = JSON.parse(readFileSync(path.join(matchDir, "match.json"), "utf8"));
    expect(index.players.host.coachingSessionId).toBe("coaching-host");
    expect(index.players.joiner.coachingSessionId).toBe("coaching-joiner");
  });

  it("rejects a bundle whose drawing is not a PNG", async () => {
    const app = createApp();
    const response = await request(app)
      .post("/api/lobby/artifacts")
      .send({
        matchId: currentMatchIdForTests(),
        role: "host",
        teamId: "red",
        coachingSessionId: "coaching-host",
        session: { schemaVersion: 1 },
        tacticalOutput: fixtureOutput("red", "coaching-host"),
        drawings: { appearance: Buffer.from("not a png").toString("base64") },
      });
    expect(response.status).toBe(500);
    const status = await request(app).get("/api/lobby/status");
    expect(status.body.hostArtifacts).toBe(false);
  });

  it("rejects a bundle for a stale match id", async () => {
    const app = createApp();
    const response = await request(app)
      .post("/api/lobby/artifacts")
      .send({
        matchId: "match-from-a-previous-game",
        role: "host",
        teamId: "red",
        coachingSessionId: "coaching-host",
        session: { schemaVersion: 1 },
        tacticalOutput: fixtureOutput("red", "coaching-host"),
      });
    expect(response.status).toBe(409);
    expect(response.body.code).toBe("STALE_MATCH_ID");
  });
});
