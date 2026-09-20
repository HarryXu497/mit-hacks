import { afterEach, describe, expect, it } from "vitest";
import request from "supertest";
import { createApp } from "./app";
import { resetLobbyStateForTests } from "./lobby";
import type { TacticalOutput } from "../src/domain/interpret";

function fixtureOutput(teamId: "red" | "yellow"): TacticalOutput {
  return {
    schemaVersion: "2.0",
    taxonomyVersion: "tactics-v2",
    interpretationMode: "deterministic-preview",
    session: { id: "s", title: "t", durationMs: 0 },
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
      sessionId: "s",
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
  afterEach(() => resetLobbyStateForTests());

  it("reports not-ready until both sides post", async () => {
    const app = createApp();
    const initial = await request(app).get("/api/lobby/status");
    expect(initial.body).toMatchObject({ hostReady: false, joinerReady: false });

    await request(app)
      .post("/api/lobby/ready")
      .send({ role: "host", teamId: "red", tacticalOutput: fixtureOutput("red") })
      .expect(200);

    const afterHost = await request(app).get("/api/lobby/status");
    expect(afterHost.body).toMatchObject({ hostReady: true, joinerReady: false });

    await request(app)
      .post("/api/lobby/ready")
      .send({ role: "joiner", teamId: "yellow", tacticalOutput: fixtureOutput("yellow") })
      .expect(200);

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

  it("auto-starts once both sides post a ready tactical output", async () => {
    const app = createApp();
    await request(app)
      .post("/api/lobby/ready")
      .send({ role: "host", teamId: "red", tacticalOutput: fixtureOutput("red") })
      .expect(200);
    const afterOne = await request(app).get("/api/lobby/status");
    expect(afterOne.body.started).toBe(false);

    const afterTwo = await request(app)
      .post("/api/lobby/ready")
      .send({ role: "joiner", teamId: "yellow", tacticalOutput: fixtureOutput("yellow") });
    expect(afterTwo.body.started).toBe(true);
  });

  it("rejects a malformed ready payload", async () => {
    const app = createApp();
    await request(app)
      .post("/api/lobby/ready")
      .send({ role: "host", teamId: "purple", tacticalOutput: {} })
      .expect(400);
  });
});
