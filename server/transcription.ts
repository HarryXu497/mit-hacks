import type { Server } from "node:http";
import { WebSocket, WebSocketServer, type RawData } from "ws";

interface StartMessage {
  type: "start";
  generation: number;
  sessionId: string;
  sessionOffsetMs: number;
  sampleRate: 24000;
}

interface AudioMessage {
  type: "audio";
  generation: number;
  pcm16: string;
}

interface StopMessage {
  type: "stop";
  generation: number;
}

type ClientMessage = StartMessage | AudioMessage | StopMessage;

const DEEPGRAM_LISTEN_URL = "wss://api.deepgram.com/v1/listen";

export function attachTranscriptionWebSocket(server: Server): void {
  const socketServer = new WebSocketServer({ noServer: true });

  server.on("upgrade", (request, socket, head) => {
    const url = new URL(request.url ?? "/", "http://127.0.0.1");
    if (url.pathname !== "/api/transcribe") return;
    socketServer.handleUpgrade(request, socket, head, (client) => {
      socketServer.emit("connection", client, request);
    });
  });

  socketServer.on("connection", (client) => proxyTranscription(client));
}

export function proxyTranscription(client: WebSocket, connect = connectProvider): void {
  let provider: WebSocket | null = null;
  let generation: number | null = null;
  let sessionOffsetMs = 0;
  let receivedSamples = 0;
  let providerReady = false;
  let stopping = false;
  let completed = false;
  let closeSent = false;
  let providerMetadata = false;
  const seenSegments = new Set<string>();
  let stopTimer: NodeJS.Timeout | null = null;
  const queuedAudio: string[] = [];
  let nextItemId = 0;
  let currentItemId: string | null = null;
  let confirmedSegments: string[] = [];
  let utteranceStartMs: number | null = null;
  let utteranceEndMs = 0;

  const sendClient = (message: Record<string, unknown>) => {
    if (client.readyState === WebSocket.OPEN && generation !== null) {
      client.send(JSON.stringify({ ...message, generation }));
    }
  };

  const finish = () => {
    if (stopTimer) clearTimeout(stopTimer);
    stopTimer = null;
    completed = true;
    sendClient({ type: "done" });
    provider?.close();
    client.close();
  };

  const fail = (message: string) => {
    if (completed) return;
    completed = true;
    if (stopTimer) clearTimeout(stopTimer);
    sendClient({ type: "error", message });
    provider?.close();
    client.close();
  };
  const closeStream = () => {
    if (providerReady && stopping && !closeSent) {
      closeSent = true;
      provider!.send(JSON.stringify({ type: "CloseStream" }));
    }
  };

  const finalizeUtterance = () => {
    const text = confirmedSegments.join(" ").trim();
    if (text) {
      const rangeStart = utteranceStartMs ?? sessionOffsetMs;
      const rangeEnd = Math.max(utteranceEndMs, rangeStart);
      for (const { sentence, startMs, endMs } of splitIntoTimedSentences(text, rangeStart, rangeEnd)) {
        const itemId = `item-${nextItemId}`;
        nextItemId += 1;
        sendClient({ type: "final", itemId, text: sentence, startMs, endMs });
      }
    }
    currentItemId = null;
    confirmedSegments = [];
    utteranceStartMs = null;
  };

  client.on("message", (raw) => {
    if (completed) return;
    let message: ClientMessage;
    try {
      message = JSON.parse(raw.toString()) as ClientMessage;
    } catch {
      sendClient({ type: "error", message: "Invalid transcription message." });
      return;
    }

    if (message.type === "start") {
      if (provider || message.sampleRate !== 24000) {
        sendClient({ type: "error", message: "Invalid or duplicate transcription start." });
        return;
      }
      generation = message.generation;
      sessionOffsetMs = message.sessionOffsetMs;
      if (!process.env.DEEPGRAM_API_KEY) {
        sendClient({ type: "error", message: "DEEPGRAM_API_KEY is not configured." });
        return;
      }
      provider = connect({
        onClose: () => {
          if (completed) return;
          if (stopping && closeSent && providerMetadata) {
            finalizeUtterance();
            finish();
          } else fail("The transcript did not finish. Please try again.");
        },
        onOpen: () => {
          providerReady = true;
          for (const audio of queuedAudio.splice(0)) appendProviderAudio(provider!, audio);
          sendClient({ type: "ready" });
          closeStream();
        },
        onEvent: (event) => {
          if (completed) return;
          const type = stringField(event, "type");
          if (type === "Metadata") providerMetadata = true;
          if (type === "Results") {
            const alternative = firstAlternative(event);
            const transcript = stringField(alternative, "transcript");
            const isFinal = booleanField(event, "is_final");
            const speechFinal = booleanField(event, "speech_final");

            if (!transcript) {
              if (isFinal && speechFinal) finalizeUtterance();
              return;
            }

            if (currentItemId === null) {
              currentItemId = `item-${nextItemId}`;
              nextItemId += 1;
              confirmedSegments = [];
              utteranceStartMs = null;
            }
            if (isFinal) {
              const key = JSON.stringify([event.start, event.duration, transcript]);
              if (seenSegments.has(key)) return;
              seenSegments.add(key);
            }

            const start = numberField(event, "start", samplesToSeconds(receivedSamples));
            const duration = numberField(event, "duration", 0);
            utteranceEndMs = Math.round(sessionOffsetMs + (start + duration) * 1000);
            if (utteranceStartMs === null) {
              utteranceStartMs = Math.round(sessionOffsetMs + start * 1000);
            }

            if (!isFinal) {
              const preview = [...confirmedSegments, transcript].join(" ").trim();
              sendClient({ type: "partial", itemId: currentItemId, text: preview });
              return;
            }

            confirmedSegments.push(transcript);
            const confirmedText = confirmedSegments.join(" ").trim();
            sendClient({ type: "partial", itemId: currentItemId, text: confirmedText });
            if (speechFinal) finalizeUtterance();
          } else if (type === "Error") {
            sendClient({
              type: "error",
              message: stringField(event, "description") || "Deepgram transcription failed.",
            });
          }
        },
        onError: (message) => fail(message),
      });
      return;
    }

    if (generation === null || message.generation !== generation || !provider) return;

    if (message.type === "audio" && !stopping) {
      const bytes = Buffer.from(message.pcm16, "base64");
      if (bytes.length === 0 || bytes.length % 2 !== 0) return;
      receivedSamples += bytes.length / 2;
      if (providerReady) appendProviderAudio(provider, message.pcm16);
      else if (receivedSamples <= 24000 * 30) queuedAudio.push(message.pcm16);
      else fail("Too much audio buffered while connecting. Please try again.");
      return;
    }

    if (message.type === "stop" && !stopping) {
      stopping = true;
      if (receivedSamples === 0) {
        finish();
        return;
      }
      closeStream();
      // Client waits 10 seconds; never silently succeed with a partial transcript.
      stopTimer = setTimeout(() => fail("Timed out while finalizing the transcript. Please try again."), 8000);
    }
  });

  const cleanup = () => {
    completed = true;
    if (stopTimer) clearTimeout(stopTimer);
    provider?.close();
  };
  client.on("close", cleanup);
  // Canceling native capture drops its TCP connection. A reset is local to this
  // session, not an uncaught EventEmitter error that brings down the host API.
  client.on("error", () => {
    cleanup();
    client.terminate();
  });
}

function connectProvider(handlers: {
  onOpen: () => void;
  onEvent: (event: Record<string, unknown>) => void;
  onError: (message: string) => void;
  onClose: () => void;
}): WebSocket {
  const model = process.env.DEEPGRAM_MODEL ?? "nova-2";
  const query = new URLSearchParams({
    encoding: "linear16",
    sample_rate: "24000",
    channels: "1",
    interim_results: "true",
    punctuate: "true",
    smart_format: "true",
    endpointing: "300",
    model,
    language: "en",
  });
  const provider = new WebSocket(`${DEEPGRAM_LISTEN_URL}?${query.toString()}`, {
    handshakeTimeout: 7000,
    headers: { Authorization: `Token ${process.env.DEEPGRAM_API_KEY}` },
  });
  provider.on("open", handlers.onOpen);
  provider.on("message", (raw: RawData) => {
    try {
      handlers.onEvent(JSON.parse(raw.toString()) as Record<string, unknown>);
    } catch {
      handlers.onError("The transcription provider returned invalid data.");
    }
  });
  provider.on("error", (error) => handlers.onError(error.message));
  provider.on("close", handlers.onClose);
  return provider;
}

function appendProviderAudio(provider: WebSocket, audio: string): void {
  if (provider.readyState === WebSocket.OPEN) {
    provider.send(Buffer.from(audio, "base64"));
  }
}

function samplesToSeconds(samples: number): number {
  return samples / 24000;
}

/**
 * Splits an utterance's confirmed text into sentences and distributes the
 * utterance's measured time range across them proportionally by character
 * length, since Deepgram only reports timing for the utterance as a whole.
 */
function splitIntoTimedSentences(
  text: string,
  rangeStart: number,
  rangeEnd: number,
): Array<{ sentence: string; startMs: number; endMs: number }> {
  const sentences = (text.match(/[^.!?]+(?:[.!?]+|$)/g) ?? [text]).map((s) => s.trim()).filter(Boolean);
  const totalChars = sentences.reduce((sum, s) => sum + s.length, 0) || 1;
  let consumed = 0;
  return sentences.map((sentence) => {
    const startMs = Math.round(rangeStart + (consumed / totalChars) * (rangeEnd - rangeStart));
    consumed += sentence.length;
    const endMs = Math.round(rangeStart + (consumed / totalChars) * (rangeEnd - rangeStart));
    return { sentence, startMs, endMs };
  });
}

function firstAlternative(event: unknown): Record<string, unknown> {
  const channel = objectField(event, "channel");
  const alternatives = channel["alternatives"];
  if (!Array.isArray(alternatives) || alternatives.length === 0) return {};
  const alternative = alternatives[0];
  return alternative && typeof alternative === "object" ? (alternative as Record<string, unknown>) : {};
}

function stringField(value: unknown, key: string): string {
  if (!value || typeof value !== "object") return "";
  const field = (value as Record<string, unknown>)[key];
  return typeof field === "string" ? field : "";
}

function numberField(value: unknown, key: string, fallback: number): number {
  if (!value || typeof value !== "object") return fallback;
  const field = (value as Record<string, unknown>)[key];
  return typeof field === "number" ? field : fallback;
}

function booleanField(value: unknown, key: string): boolean {
  if (!value || typeof value !== "object") return false;
  const field = (value as Record<string, unknown>)[key];
  return field === true;
}

function objectField(value: unknown, key: string): Record<string, unknown> {
  if (!value || typeof value !== "object") return {};
  const field = (value as Record<string, unknown>)[key];
  return field && typeof field === "object" ? (field as Record<string, unknown>) : {};
}
