import { afterEach, describe, expect, it, vi } from "vitest";
import request from "supertest";
import express from "express";
import { createShout, PERSONALITIES, resolveShout, shoutRouter, type ShoutProviders } from "./shout";

const backend = (): ShoutProviders => ({
  generate: vi.fn(async () => "I was adding dramatic tension, but fine, defense it is!"),
  synthesize: vi.fn(async () => ({ pcm16: "AAAAAA==", encoding: "linear16", sampleRate: 24000, channels: 1 } as const)),
});
const input = { requestId: "shout-1", teamId: "red" as const, transcript: "Player four, get back on defense!" };
afterEach(() => { vi.useRealTimers(); vi.unstubAllEnvs(); });
describe("shouts", () => {
  it.each(["Player 4, get back!", "Player four, move!", "Player number four move", "player #4 move", "player 4, player four, move", "Player four get back in two seconds"])("resolves %s", (text) => {
    expect(resolveShout("yellow", text)).toEqual({ teamId: "yellow", playerNumber: 4, feedback: text });
  });
  it.each(["", "get back", "player 0 move", "player six move", "player 14 move", "player -1 move", "player 1.5 move", "run at them"])("still answers %s", async (transcript) => {
    const provider = backend();
    const response = await createShout({ ...input, transcript }, new AbortController().signal, provider);
    expect(provider.generate).toHaveBeenCalled();
    expect(response.playerNumber).toBeGreaterThanOrEqual(1);
    expect(response.playerNumber).toBeLessThanOrEqual(5);
    expect(response.replyText.length).toBeGreaterThan(0);
  });
  it.each([["player 4 and player 2 move", 4], ["players three and four move", 3], ["players 1, 2, and 3 move", 1]])("answers as the first player named in %s", (transcript, expected) => {
    expect(resolveShout("red", transcript as string).playerNumber).toBe(expected);
  });
  it("spreads unaddressed shouts across the bench rather than always picking one", () => {
    const picked = new Set(Array.from({ length: 80 }, () => resolveShout("red", "get back").playerNumber));
    expect(picked.size).toBeGreaterThan(1);
    for (const n of picked) expect(n).toBeGreaterThanOrEqual(1), expect(n).toBeLessThanOrEqual(5);
  });
  it.each(PERSONALITIES.map((_, i) => i + 1))("uses player %i's fixed personality and voice on either team", async (n) => {
    for (const teamId of ["red", "yellow"] as const) {
      const provider = backend();
      const response = await createShout({ ...input, teamId, transcript: `Player ${n}, move!` }, new AbortController().signal, provider);
      expect(provider.generate).toHaveBeenCalledWith({ teamId, playerNumber: n, feedback: `Player ${n}, move!` }, PERSONALITIES[n - 1].personality, expect.any(AbortSignal));
      expect(provider.synthesize).toHaveBeenCalledWith(response.replyText, PERSONALITIES[n - 1].voice, expect.any(AbortSignal));
      expect(response.playerNumber).toBe(n);
    }
  });
  it.each(["", " ", "bad\nreply", "x".repeat(241)])("falls back rather than failing on invalid generated text", async (text) => {
    const provider = backend(); provider.generate = vi.fn(async () => text);
    const result = await createShout(input, new AbortController().signal, provider);
    expect(result.replyText).toBe("On it, coach!");
    expect(provider.synthesize).toHaveBeenCalled();
  });
  it("retains text on synthesis failure", async () => {
    const provider = backend(); provider.synthesize = vi.fn(async () => { throw new Error("failed"); });
    const result = await createShout(input, new AbortController().signal, provider);
    expect(result).toMatchObject({ requestId: input.requestId, playerNumber: 4, audio: null, audioError: "Voice unavailable" });
    expect(result.replyText).toContain("dramatic");
  });
  it("bounds generation and synthesis, including uncooperative providers", async () => {
    vi.useFakeTimers();
    const provider = backend(); provider.generate = vi.fn(() => new Promise<never>(() => {}));
    const result = createShout(input, new AbortController().signal, provider, 50);
    const rejected = expect(result).rejects.toThrow("timed out");
    await vi.advanceTimersByTimeAsync(51); await rejected;
    const voice = backend(); voice.synthesize = vi.fn(() => new Promise<never>(() => {}));
    const textOnly = createShout(input, new AbortController().signal, voice, 50);
    await vi.advanceTimersByTimeAsync(51);
    expect(await textOnly).toMatchObject({ audioError: "Voice unavailable", audio: null });
  });
  it("returns simultaneous coaches' responses only to their own requests", async () => {
    const app = express().use(express.json()).use(shoutRouter(backend()));
    const results = await Promise.all([request(app).post("/api/shout").send(input), request(app).post("/api/shout").send({ ...input, teamId: "yellow", requestId: "joiner" })]);
    expect(results.map((r) => r.status)).toEqual([200, 200]);
    expect(results.map((r) => r.body.requestId)).toEqual(["shout-1", "joiner"]);
    expect(results.map((r) => r.body.playerNumber)).toEqual([4, 4]);
    expect((await request(app).post("/api/shout").send({ ...input, teamId: "blue" })).status).toBe(400);
    // An out-of-range number is no longer a refusal: someone on the roster still answers.
    const spare = await request(app).post("/api/shout").send({ ...input, transcript: "player 9 move" });
    expect(spare.status).toBe(200);
    expect(spare.body.playerNumber).toBeGreaterThanOrEqual(1);
    expect(spare.body.playerNumber).toBeLessThanOrEqual(5);
  });
});
