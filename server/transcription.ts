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
    if (url.pathname !== "/api/transcribe") {
      socket.destroy();
      return;
    }
    socketServer.handleUpgrade(request, socket, head, (client) => {
      socketServer.emit("connection", client, request);
    });
  });

  socketServer.on("connection", (client) => proxyTranscription(client));
}

function proxyTranscription(client: WebSocket): void {
  let provider: WebSocket | null = null;
  let generation: number | null = null;
  let sessionOffsetMs = 0;
  let receivedSamples = 0;
  let providerReady = false;
  let stopping = false;
  let pendingUtterance = false;
  let expectingProviderClose = false;
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
    expectingProviderClose = true;
    sendClient({ type: "done" });
    provider?.close();
    client.close();
  };

  const maybeFinish = () => {
    if (stopping && !pendingUtterance) finish();
  };

  const finalizeUtterance = () => {
    const text = confirmedSegments.join(" ").trim();
    if (currentItemId && text) {
      sendClient({
        type: "final",
        itemId: currentItemId,
        text,
        startMs: utteranceStartMs ?? sessionOffsetMs,
        endMs: utteranceEndMs,
      });
    }
    currentItemId = null;
    confirmedSegments = [];
    utteranceStartMs = null;
    pendingUtterance = false;
    maybeFinish();
  };

  client.on("message", (raw) => {
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
      provider = connectProvider({
        onClose: () => {
          if (!expectingProviderClose && client.readyState === WebSocket.OPEN) {
            sendClient({ type: "error", message: "The transcription provider disconnected." });
          }
        },
        onOpen: () => {
          providerReady = true;
          for (const audio of queuedAudio.splice(0)) appendProviderAudio(provider!, audio);
          sendClient({ type: "ready" });
        },
        onEvent: (event) => {
          const type = stringField(event, "type");
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
            pendingUtterance = true;

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
        onError: (message) => sendClient({ type: "error", message }),
      });
      return;
    }

    if (generation === null || message.generation !== generation || !provider) return;

    if (message.type === "audio" && !stopping) {
      const bytes = Buffer.from(message.pcm16, "base64");
      if (bytes.length === 0 || bytes.length % 2 !== 0) return;
      receivedSamples += bytes.length / 2;
      if (providerReady) appendProviderAudio(provider, message.pcm16);
      else if (queuedAudio.length < 250) queuedAudio.push(message.pcm16);
      return;
    }

    if (message.type === "stop" && !stopping) {
      stopping = true;
      expectingProviderClose = true;
      if (receivedSamples === 0) {
        finish();
        return;
      }
      if (providerReady) {
        provider.send(JSON.stringify({ type: "CloseStream" }));
      }
      stopTimer = setTimeout(() => {
        if (!pendingUtterance) {
          finish();
        } else {
          sendClient({
            type: "error",
            message: "Timed out while finalizing the transcript.",
          });
          provider?.close();
          client.close();
        }
      }, 8000);
    }
  });

  client.on("close", () => {
    if (stopTimer) clearTimeout(stopTimer);
    provider?.close();
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
  });
  const provider = new WebSocket(`${DEEPGRAM_LISTEN_URL}?${query.toString()}`, {
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
