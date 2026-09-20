import "dotenv/config";
import { createServer } from "node:http";
import { once } from "node:events";
import { WebSocket } from "ws";
import { expect, it } from "vitest";
import { createApp } from "../../server/app";
import { attachTranscriptionWebSocket } from "../../server/transcription";

it("transcribes an early-release audio fixture and returns a voiced private reply", async () => {
  if (!process.env.OPENAI_API_KEY || !process.env.DEEPGRAM_API_KEY || !(process.env.OPENAI_SHOUT_MODEL || process.env.OPENAI_MODEL)) {
    throw new Error("Configure OpenAI and Deepgram in .env before running live shout tests.");
  }
  // Synthetic speech makes the live provider regression reproducible; it does not test a physical microphone.
  const fixture = await fetch("https://api.deepgram.com/v1/speak?model=aura-2-apollo-en&encoding=linear16&sample_rate=24000&container=none", {
    method: "POST", headers: { Authorization: `Token ${process.env.DEEPGRAM_API_KEY}`, "Content-Type": "application/json" },
    body: JSON.stringify({ text: "Player four, get back on defense!" }), signal: AbortSignal.timeout(12_000),
  });
  expect(fixture.ok).toBe(true);
  const pcm = Buffer.from(await fixture.arrayBuffer());
  const server = createServer(createApp()); attachTranscriptionWebSocket(server);
  server.listen(0, "127.0.0.1"); await once(server, "listening");
  const address = server.address(); if (!address || typeof address === "string") throw new Error("No test server");
  const socket = new WebSocket(`ws://127.0.0.1:${address.port}/api/transcribe`);
  try {
    const transcript = await new Promise<string>((resolve, reject) => {
      const finals = new Map<string, string>();
      const timer = setTimeout(() => reject(new Error("Transcription deadline exceeded")), 15_000);
      socket.on("error", (error) => { clearTimeout(timer); reject(error); });
      socket.on("open", () => {
        socket.send(JSON.stringify({ type: "start", generation: 1, sessionId: "shout-live", sessionOffsetMs: 0, sampleRate: 24000 }));
        socket.send(JSON.stringify({ type: "audio", generation: 1, pcm16: pcm.toString("base64") }));
        socket.send(JSON.stringify({ type: "stop", generation: 1 }));
      });
      socket.on("message", (raw) => {
        const message = JSON.parse(raw.toString());
        if (message.type === "final") finals.set(message.itemId, message.text);
        if (message.type === "error") { clearTimeout(timer); reject(new Error(message.message)); }
        if (message.type === "done") { clearTimeout(timer); resolve([...finals.values()].join(" ")); }
      });
    });
    expect(transcript.toLowerCase()).toMatch(/player (four|4)/);
    const response = await fetch(`http://127.0.0.1:${address.port}/api/shout`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ requestId: "live-shout", teamId: "yellow", transcript }), signal: AbortSignal.timeout(38_000),
    });
    const result = await response.json();
    expect(response.ok, JSON.stringify({ transcript, status: response.status, result })).toBe(true);
    expect(result).toMatchObject({ requestId: "live-shout", playerNumber: 4, audio: { encoding: "linear16", sampleRate: 24000, channels: 1 } });
    expect(result.replyText.length).toBeGreaterThan(0);
    expect(Buffer.from(result.audio.pcm16, "base64").length).toBeGreaterThan(0);
  } finally { socket.terminate(); server.closeAllConnections(); await new Promise<void>((resolve) => server.close(() => resolve())); }
});
