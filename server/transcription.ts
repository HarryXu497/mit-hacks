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

interface ItemTiming {
  startMs?: number;
  endMs?: number;
}

const OPENAI_REALTIME_URL = "wss://api.openai.com/v1/realtime?intent=transcription";

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
  let stopCommitAcknowledged = false;
  let stopTimer: NodeJS.Timeout | null = null;
  const queuedAudio: string[] = [];
  const transcriptByItem = new Map<string, string>();
  const timingByItem = new Map<string, ItemTiming>();
  const pendingItems = new Set<string>();
  const completedItems = new Set<string>();

  const sendClient = (message: Record<string, unknown>) => {
    if (client.readyState === WebSocket.OPEN && generation !== null) {
      client.send(JSON.stringify({ ...message, generation }));
    }
  };

  const finish = () => {
    if (stopTimer) clearTimeout(stopTimer);
    stopTimer = null;
    sendClient({ type: "done" });
    provider?.close();
    client.close();
  };

  const maybeFinish = () => {
    if (stopping && stopCommitAcknowledged && pendingItems.size === 0) finish();
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
      if (!process.env.OPENAI_API_KEY) {
        sendClient({ type: "error", message: "OPENAI_API_KEY is not configured." });
        return;
      }
      provider = connectProvider(client, {
        onOpen: () => {
          providerReady = true;
          for (const audio of queuedAudio.splice(0)) appendProviderAudio(provider!, audio);
          if (stopping && receivedSamples > 0) {
            provider!.send(JSON.stringify({ type: "input_audio_buffer.commit" }));
          }
          sendClient({ type: "ready" });
        },
        onEvent: (event) => {
          const type = stringField(event, "type");
          const itemId = stringField(event, "item_id");
          if (type === "input_audio_buffer.speech_started" && itemId) {
            timingByItem.set(itemId, {
              startMs: sessionOffsetMs + numberField(event, "audio_start_ms", samplesToMs(receivedSamples)),
            });
          } else if (type === "input_audio_buffer.speech_stopped" && itemId) {
            const timing = timingByItem.get(itemId) ?? {};
            timing.endMs =
              sessionOffsetMs + numberField(event, "audio_end_ms", samplesToMs(receivedSamples));
            timingByItem.set(itemId, timing);
          } else if (type === "input_audio_buffer.committed" && itemId) {
            pendingItems.add(itemId);
            if (stopping) stopCommitAcknowledged = true;
            maybeFinish();
          } else if (
            type === "conversation.item.input_audio_transcription.delta" &&
            itemId
          ) {
            const text = (transcriptByItem.get(itemId) ?? "") + stringField(event, "delta");
            transcriptByItem.set(itemId, text);
            sendClient({ type: "partial", itemId, text });
          } else if (
            type === "conversation.item.input_audio_transcription.completed" &&
            itemId &&
            !completedItems.has(itemId)
          ) {
            completedItems.add(itemId);
            pendingItems.delete(itemId);
            const timing = timingByItem.get(itemId);
            const endMs =
              timing?.endMs ?? sessionOffsetMs + samplesToMs(receivedSamples);
            const startMs = timing?.startMs ?? Math.max(sessionOffsetMs, endMs - 2400);
            sendClient({
              type: "final",
              itemId,
              text: stringField(event, "transcript") || transcriptByItem.get(itemId) || "",
              startMs,
              endMs,
            });
            maybeFinish();
          } else if (type === "error") {
            const error = objectField(event, "error");
            sendClient({
              type: "error",
              message: stringField(error, "message") || "OpenAI transcription failed.",
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
      if (receivedSamples === 0) {
        stopCommitAcknowledged = true;
        finish();
        return;
      }
      if (providerReady) {
        provider.send(JSON.stringify({ type: "input_audio_buffer.commit" }));
      }
      stopTimer = setTimeout(() => {
        if (pendingItems.size === 0) finish();
        else {
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

function connectProvider(
  client: WebSocket,
  handlers: {
    onOpen: () => void;
    onEvent: (event: Record<string, unknown>) => void;
    onError: (message: string) => void;
  },
): WebSocket {
  const provider = new WebSocket(OPENAI_REALTIME_URL, {
    headers: { Authorization: `Bearer ${process.env.OPENAI_API_KEY}` },
  });
  provider.on("open", () => {
    provider.send(
      JSON.stringify({
        type: "session.update",
        session: {
          type: "transcription",
          audio: {
            input: {
              format: { type: "audio/pcm", rate: 24000 },
              transcription: {
                model: process.env.OPENAI_TRANSCRIPTION_MODEL ?? "gpt-live-transcribe",
                languages: ["en"],
                delay: "low",
                prompt: "A soccer coach explaining a five-versus-five tactical demonstration.",
              },
              turn_detection: {
                type: "server_vad",
                threshold: 0.5,
                prefix_padding_ms: 300,
                silence_duration_ms: 500,
              },
            },
          },
        },
      }),
    );
    handlers.onOpen();
  });
  provider.on("message", (raw: RawData) => {
    try {
      handlers.onEvent(JSON.parse(raw.toString()) as Record<string, unknown>);
    } catch {
      handlers.onError("The transcription provider returned invalid data.");
    }
  });
  provider.on("error", (error) => handlers.onError(error.message));
  provider.on("close", () => {
    if (client.readyState === WebSocket.OPEN) {
      handlers.onError("The transcription provider disconnected.");
    }
  });
  return provider;
}

function appendProviderAudio(provider: WebSocket, audio: string): void {
  if (provider.readyState === WebSocket.OPEN) {
    provider.send(JSON.stringify({ type: "input_audio_buffer.append", audio }));
  }
}

function samplesToMs(samples: number): number {
  return Math.round((samples / 24000) * 1000);
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

function objectField(value: unknown, key: string): Record<string, unknown> {
  if (!value || typeof value !== "object") return {};
  const field = (value as Record<string, unknown>)[key];
  return field && typeof field === "object" ? (field as Record<string, unknown>) : {};
}
