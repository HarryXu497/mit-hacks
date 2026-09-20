import { afterEach, expect, it, vi } from "vitest";
const { create, options } = vi.hoisted(() => ({ create: vi.fn(), options: vi.fn() }));
vi.mock("openai", () => ({ default: class {
  constructor(config: unknown) { options(config); }
  responses = { create };
} }));
import { createShout } from "./shout";
const input = { requestId: "test", teamId: "red" as const, transcript: "Player four, get back!" };
const signal = () => new AbortController().signal;
function setup() {
  vi.stubEnv("OPENAI_API_KEY", "test"); vi.stubEnv("DEEPGRAM_API_KEY", "test");
  vi.stubEnv("OPENAI_MODEL", "fallback-model"); vi.stubEnv("OPENAI_SHOUT_MODEL", "shout-model");
  create.mockResolvedValue({ status: "completed", output_text: "I was building suspense, coach, but the defense awaits my brilliance!" });
  const fetch = vi.fn(async () => new Response(new Uint8Array([0, 0, 1, 0]), { headers: { "Content-Type": "audio/l16;rate=24000" } }));
  vi.stubGlobal("fetch", fetch); return fetch;
}
afterEach(() => { vi.unstubAllGlobals(); vi.unstubAllEnvs(); vi.clearAllMocks(); });
it("uses stateless Responses, configured model, bounded retries, and the player's PCM voice", async () => {
  const fetch = setup(); await createShout(input, signal());
  expect(options).toHaveBeenCalledWith({ apiKey: "test", timeout: 20000, maxRetries: 0 });
  expect(create).toHaveBeenCalledWith(expect.objectContaining({ model: "shout-model", store: false, input: JSON.stringify({ teamId: "red", playerNumber: 4, feedback: input.transcript }), instructions: expect.stringContaining("Dramatic excuse-maker") }), { signal: expect.any(AbortSignal) });
  const request = create.mock.calls[0][0];
  expect(request).not.toHaveProperty("tools"); expect(request).not.toHaveProperty("previous_response_id");
  expect(fetch).toHaveBeenCalledWith(expect.stringContaining("model=aura-perseus-en&encoding=linear16&sample_rate=24000&container=none"), expect.objectContaining({ method: "POST", signal: expect.any(AbortSignal) }));
});
it("falls back to OPENAI_MODEL", async () => {
  setup(); vi.stubEnv("OPENAI_SHOUT_MODEL", ""); await createShout(input, signal());
  expect(create.mock.calls[0][0].model).toBe("fallback-model");
});
it("rejects an unfinished model response without synthesizing", async () => {
  const fetch = setup();
  create.mockResolvedValue({ status: "incomplete", output_text: "A partial reply" });
  await expect(createShout(input, signal())).rejects.toThrow();
  expect(fetch).not.toHaveBeenCalled();
});
it("still answers when the model returns nothing usable", async () => {
  const fetch = setup();
  create.mockResolvedValue({ status: "completed", output_text: "" });
  expect(await createShout(input, signal())).toMatchObject({ replyText: "On it, coach!" });
  expect(fetch).toHaveBeenCalled();
});
it("preserves text when Deepgram returns invalid PCM or a provider error", async () => {
  setup();
  for (const response of [new Response("error", { status: 500 }), new Response(new Uint8Array([1]), { headers: { "Content-Type": "audio/l16" } }), new Response("{}", { headers: { "Content-Type": "application/json" } })]) {
    vi.stubGlobal("fetch", vi.fn(async () => response));
    expect(await createShout(input, signal())).toMatchObject({ audio: null, audioError: "Voice unavailable", replyText: expect.stringContaining("suspense") });
  }
});
