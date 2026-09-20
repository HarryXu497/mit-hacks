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
visible in where the ball spends the match.

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
cargo test --manifest-path native/coaching/Cargo.toml --lib -p cube-soccer -p tactic-lab-native
cargo build --manifest-path native/coaching/Cargo.toml --bin native-coaching
```

The normal test suite is deterministic and does not make network requests. To
run the two opt-in integration tests against the configured OpenAI model:

```bash
npm run test:live
```

Model-backed JSON generation uses the server route and chooses the closest
supported tactic. Failures show their reason with **Retry interpretation** and
**Continue anyway (Balanced)**; Balanced is never silently substituted. See
[next-steps.md](./next-steps.md) for architecture and future hardening notes.
