import { EventEmitter } from "node:events";
import { WebSocket } from "ws";
import { afterEach, expect, it, vi } from "vitest";
import { proxyTranscription } from "./transcription";
class Socket extends EventEmitter {
  readyState: number = WebSocket.OPEN;
  sent: Array<string | Buffer> = [];
  send(data: string | Buffer) { this.sent.push(data); }
  close() { this.readyState = WebSocket.CLOSED; }
  terminate() { this.close(); }
  events() { return this.sent.filter((s): s is string => typeof s === "string").map((s) => JSON.parse(s)); }
}
function fixture() {
  vi.stubEnv("DEEPGRAM_API_KEY", "test");
  const client = new Socket(); const provider = new Socket();
  let handlers!: Parameters<NonNullable<Parameters<typeof proxyTranscription>[1]>>[0];
  proxyTranscription(client as unknown as WebSocket, (h) => { handlers = h; return provider as unknown as WebSocket; });
  const send = (message: object) => client.emit("message", Buffer.from(JSON.stringify({ generation: 1, ...message })));
  send({ type: "start", sampleRate: 24000, sessionOffsetMs: 0 });
  const result = (text: string, start: number, speechFinal = true) => handlers.onEvent({ type: "Results", start, duration: 1, is_final: true, speech_final: speechFinal, channel: { alternatives: [{ transcript: text }] } });
  return { client, provider, handlers, send, result };
}
afterEach(() => { vi.useRealTimers(); vi.unstubAllEnvs(); });
it("flushes buffered audio before CloseStream on release before connect", () => {
  const { send, handlers, provider, client, result } = fixture();
  send({ type: "audio", pcm16: "AAAAAA==" }); send({ type: "stop" }); handlers.onOpen();
  expect(Buffer.isBuffer(provider.sent[0])).toBe(true);
  expect(JSON.parse(provider.sent[1] as string)).toEqual({ type: "CloseStream" });
  result("Player four.", 0); result("Get back!", 2, false);
  expect(client.events().some((e) => e.type === "done")).toBe(false);
  handlers.onEvent({ type: "Metadata" }); handlers.onClose();
  expect(client.events().filter((e) => e.type === "final").map((e) => e.text)).toEqual(["Player four.", "Get back!"]);
  expect(client.events().at(-1).type).toBe("done");
});
it("does not finish on a pause while held, and ignores duplicate final segments", () => {
  const { handlers, result, client, send } = fixture(); handlers.onOpen();
  send({ type: "audio", pcm16: "AAAAAA==" });
  result("Player four.", 0); result("Player four.", 0); result("Get back. Please!", 2);
  expect(client.events().filter((e) => e.type === "final").map((e) => e.text)).toEqual(["Player four.", "Get back.", "Please!"]);
  expect(client.events().some((e) => e.type === "done")).toBe(false);
  send({ type: "stop" }); handlers.onEvent({ type: "Metadata" }); handlers.onClose();
});
it("finishes silence only after provider completion", () => {
  const { handlers, send, client } = fixture(); handlers.onOpen();
  send({ type: "audio", pcm16: "AAAAAA==" }); send({ type: "stop" });
  handlers.onEvent({ type: "Metadata" }); handlers.onClose();
  expect(client.events().filter((e) => e.type === "final")).toEqual([]);
  expect(client.events().at(-1).type).toBe("done");
});
it("reports incomplete finalization explicitly even with no pending words", async () => {
  vi.useFakeTimers(); const { handlers, send, client } = fixture(); handlers.onOpen();
  send({ type: "audio", pcm16: "AAAAAA==" }); send({ type: "stop" });
  await vi.advanceTimersByTimeAsync(8001);
  expect(client.events().at(-1)).toMatchObject({ type: "error", message: expect.stringContaining("finalizing") });
  expect(client.events().some((e) => e.type === "done")).toBe(false);
});
it("does not report success if the provider disconnects before metadata", () => {
  const { handlers, send, client } = fixture(); handlers.onOpen();
  send({ type: "audio", pcm16: "AAAAAA==" }); send({ type: "stop" }); handlers.onClose();
  expect(client.events().at(-1).type).toBe("error");
});

it("isolates a canceled client's socket reset without crashing the API", async () => {
  vi.useFakeTimers();
  const { handlers, send, client, provider } = fixture(); handlers.onOpen();
  send({ type: "audio", pcm16: "AAAAAA==" }); send({ type: "stop" });
  expect(() => client.emit("error", new Error("read ECONNRESET"))).not.toThrow();
  expect(provider.readyState).toBe(WebSocket.CLOSED);
  expect(client.readyState).toBe(WebSocket.CLOSED);
  await vi.advanceTimersByTimeAsync(9000);
  expect(client.events().some((e) => e.type === "done")).toBe(false);
});
