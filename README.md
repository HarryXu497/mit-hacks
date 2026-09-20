# Tactic Lab

Tactic Lab is a 5-v-5 soccer coaching board that keeps player, ball,
annotation, and transcript actions on one synchronized timeline and turns the
recorded session into structured tactical JSON. The primary client is the
native Rust/Bevy application under `native/coaching`. The Node service in
`server/` handles live transcription and model-backed interpretation.

## Run the native client

Install Rust 1.85+, Node.js 20+, and the platform linker prerequisites, then:

```bash
npm install
cp .env.example .env
# Add your DEEPGRAM_API_KEY (transcription) and OPENAI_API_KEY/OPENAI_MODEL
# (interpretation) to .env
npm run dev:native
```

The launcher runs the local credential-holding service and native application;
no browser is required. After interpretation, click **Next** to watch the
coached red/Orange team play a Balanced opponent in the jungle game. The
tracked `native/cube-soccer` crate supplies the tactical AI; no separate
worktree, Python service, or trained model is needed for gameplay. See
[docs/native-coaching.md](./docs/native-coaching.md) for platform setup,
permissions, storage, and verification.

The coached play reaches the pitch as team shape and on-ball decisions: the side
picks a ball carrier, shoots at the open part of the goal, passes to whoever is
best placed, clears when it is pinned in its own third, and brings the ball back
infield when it drifts wide. How high the team holds its line, how many players
leave the shape to press, how wide it spreads, and how readily it shoots all come
from the coached tactic, so an aggressive play looks aggressive. Matches are
high-scoring by design — there is no goalkeeper. `native/cube-soccer/src/systems/soccer_ai.rs`
is the whole of it, and its tests play simulated matches to check that goals keep
coming, that the ball never leaves the game for long, and that the tactic is
visible in where the players spend the match.

## Run the local service only

```bash
npm install
cp .env.example .env
npm run dev:api
```

This starts the interpretation and transcription API on `127.0.0.1:8787`.
The Deepgram and OpenAI keys stay on the server and are never sent to the
native client.

## Live player shouts

During gameplay, hold **Hold to shout**, say “Player 4, get back!”, and release
anywhere to send. Name a player by number (1–5, digits or spoken words). Your own
team's numbers float above their heads; the player who answers is highlighted while
their private, voiced reply plays. Shouts are cosmetic and never change tactics
or movement. Host/solo coaches Orange; joiners coach Blue, using the host service.

Recordings stop at 15 seconds. Losing window focus cancels capture, and leaving
gameplay cancels capture, requests, and playback. The button is disabled until a
reply finishes. If voice synthesis or playback fails, the reply remains as text.

A shout always reaches someone: name a player ("player 4", "number 4", "monkey 4") and they answer,
and if you name nobody, whoever looks up does. A reply the model returns but that is unusable —
empty, overlong, or malformed — falls back to a plain "On it, coach!" rather than failing the shout.
A provider that is down or returns an unfinished response is still reported as an error, so a real
outage or a bad key stays visible instead of being masked by a canned line.

The microphone selected in Settings is shared with the tactics table.

`OPENAI_SHOUT_MODEL` optionally overrides `OPENAI_MODEL` for replies. Deepgram
provides English transcription and the five fixed Aura 2 voices. Both provider
keys stay in the server's `.env`; no shout history is stored or broadcast.
The server uses the [OpenAI Responses API](https://developers.openai.com/api/docs/guides/text)
and [Deepgram raw linear16 output](https://developers.deepgram.com/docs/tts-media-output-settings).

For a reproducible live transcription → reply → PCM test using synthetic speech:

```bash
npx vitest run --config vitest.live.config.ts tests/live/shout.live.test.ts
```

For the playable shout preview, start the API and native window together:

```bash
npm run dev:shout
```

`SHOUT_PREVIEW_TEAM=blue` checks joiner numbering. `TACTIC_LAB_CAPTURE=/tmp/shout.png`
saves a gameplay screenshot and exits. Leave `TACTIC_LAB_CAPTURE` unset when playing;
it deliberately enables automatic exit after the screenshot. This preview skips coaching and uses the
normal gameplay and Shout systems. Physical microphone capture, audible playback,
and simultaneous shouts from two real machines should also be checked on demo hardware.

## Verification

```bash
npm run typecheck
npm test
npm run build
cargo test --manifest-path native/coaching/Cargo.toml --lib -p cube-soccer -p tactic-lab-native
cargo build --manifest-path native/coaching/Cargo.toml --bin native-coaching
```

The normal test suite uses mocks and local loopback servers, without provider requests.
To run opt-in integration tests against the configured OpenAI and Deepgram services:

```bash
npm run test:live
```

Model-backed JSON generation uses the server route and chooses the closest
supported tactic. Failures show their reason with **Retry interpretation** and
**Continue anyway (Balanced)**; Balanced is never silently substituted. See
[next-steps.md](./next-steps.md) for architecture and future hardening notes.
