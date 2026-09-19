# Tactic Lab

Tactic Lab is a 5-v-5 soccer coaching board that keeps player, ball,
annotation, and transcript actions on one synchronized timeline and turns the
recorded session into structured tactical JSON. The primary client is the
native Rust/Bevy application under `native/coaching`. The Node service in
`server/` handles live transcription and model-backed interpretation.

## Run the native client

Install Rust 1.75+, Node.js 20+, and the platform linker prerequisites, then:

```bash
npm install
cp .env.example .env
# Add your DEEPGRAM_API_KEY (transcription) and OPENAI_API_KEY/OPENAI_MODEL
# (interpretation) to .env
npm run dev:native
```

The launcher runs the local credential-holding service and native application;
no browser is required. See
[docs/native-coaching.md](./docs/native-coaching.md) for platform setup,
permissions, storage, and verification.

## Run the local service only

```bash
npm install
cp .env.example .env
npm run dev:api
```

This starts the interpretation and transcription API on `127.0.0.1:8787`.
The Deepgram and OpenAI keys stay on the server and are never sent to the
native client.

## Verification

```bash
npm run typecheck
npm test
npm run build
```

The normal test suite is deterministic and does not make network requests. To
run the two opt-in integration tests against the configured OpenAI model:

```bash
npm run test:live
```

Model-backed JSON generation uses the server route. The native client falls
back to deterministic output when the service is unavailable. See
[next-steps.md](./next-steps.md) for architecture and future hardening notes.
