# Native coaching development

The phase-one desktop client lives in `native/coaching`. It is a standalone
Bevy 0.13 application and a reusable `CoachingPlugin`; the browser client
remains available as a behavior and JSON-contract reference.

## Prerequisites

- Rust 1.75 or newer with Cargo
- Node.js 20 or newer and npm
- An OpenAI API key for live transcription and model-backed interpretation

On macOS, install the Xcode command-line tools:

```sh
xcode-select --install
```

On Windows, install the Rust MSVC toolchain and the Visual Studio 2022 C++
build tools. The launcher uses direct child processes and does not require
PowerShell, Bash, or an installer.

This checkout was initially implemented on a machine where `rustc` and
`cargo` were not present, so the TypeScript service and shared fixtures could
be verified locally but native compilation still requires a Rust-enabled
machine or CI.

## Configure and run

```sh
npm install
cp .env.example .env
# Set OPENAI_API_KEY and OPENAI_MODEL in .env
npm run dev:native
```

The portable launcher checks Cargo, starts the local API on `127.0.0.1:8787`,
waits for `/api/health`, then runs the native client. It terminates both child
processes when the app or launcher exits. The React reference remains
available through `npm run dev`.

Environment overrides:

- `API_PORT`: local Node service port, default `8787`
- `OPENAI_MODEL`: structured tactical interpretation model
- `OPENAI_TRANSCRIPTION_MODEL`: realtime transcription model, default
  `gpt-live-transcribe`
- `TACTIC_LAB_API_URL`: native interpretation service URL
- `TACTIC_LAB_WS_URL`: native transcription WebSocket URL

The native client streams 24 kHz mono PCM to the local service. API
credentials never enter the native process.

## Microphone permissions

macOS asks for microphone access on first capture. If access was denied,
enable it for the terminal or built application under **System Settings →
Privacy & Security → Microphone**, then restart the app.

Windows asks for access according to **Settings → Privacy & security →
Microphone**. Enable desktop-app microphone access and restart the client.

The app lists available input devices, reports device and service failures,
and keeps manual transcript entry available. Audio is not written to disk.

## Storage

Interrupted sessions are restored from the operating-system application-data
directory in review mode:

- macOS: `~/Library/Application Support/TacticLab/current-session.json`
- Windows: `%LOCALAPPDATA%\TacticLab\current-session.json`

Generated test artifacts are separate:

```text
output/sessions/<session-id>/session.json
output/sessions/<session-id>/tactical-output.json
```

`output/sessions` is ignored by Git. Saves use a temporary file and atomic
replacement where supported.

## Verification

```sh
npm run typecheck
npm test
npm run build

cargo fmt --manifest-path native/coaching/Cargo.toml --check
cargo clippy --manifest-path native/coaching/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path native/coaching/Cargo.toml
cargo run --manifest-path native/coaching/Cargo.toml --bin native-coaching
```

For acceptance, exercise start/stop/resume, drawing and undo, timeline
scrubbing, transcript edits, service and microphone failures, JSON fallback
and export, reset during transcription, window resizing, and display scaling.
Interactive microphone and rendered-window checks must be run on real macOS
and Windows desktops; CI only verifies compilation and unit tests.
