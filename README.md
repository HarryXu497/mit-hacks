# Tactic Lab

A browser-based 5-v-5 soccer coaching board that keeps player, ball,
annotation, and transcript actions on one synchronized timeline and turns the
recorded session into structured tactical JSON.

## Run locally

```bash
npm install
cp .env.example .env
# Add your OPENAI_API_KEY to .env
npm run dev
```

Chrome is recommended for live browser speech recognition. Manual transcript
entry remains available when speech recognition is unsupported or permission
is denied. `npm run dev` starts the Vite client and the server-only API route;
the OpenAI key is never sent to the browser.

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

Recorded sessions are autosaved locally. Model-backed JSON generation uses the
server route and falls back to the deterministic interpreter when the service
is unavailable. See [next-steps.md](./next-steps.md) for architecture and future
hardening notes.
