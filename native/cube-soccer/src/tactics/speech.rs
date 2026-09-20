//! The microphone at the tactics table.
//!
//! The coach talks while they move tokens, and what they say lands on the same
//! session clock as everything else they do. Capture runs on its own thread:
//! the microphone is read by `cpal`, mixed to mono, resampled to 24 kHz, and
//! streamed as PCM to the local transcription service over a websocket. That
//! service holds the credentials and talks to the speech provider; nothing here
//! knows a key, which is the boundary the project's architecture asks for.
//!
//! The wire protocol is the coaching app's, message for message, so the two
//! clients are interchangeable in front of the same server.

use super::{Session, Spoken, TableState, Transcript, TranscriptFlow};
use bevy::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use crossbeam_channel::{unbounded, Receiver, Sender};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

type Fallible = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// What the transcription service expects.
const SAMPLE_RATE: u32 = 24_000;

/// Where the microphone has got to. Reported on the sign in words.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SpeechStatus {
    #[default]
    Idle,
    Connecting,
    Listening,
    /// Stopped talking; waiting for the last sentence to come back.
    Finalizing,
    Error,
}

#[derive(Debug)]
enum Command {
    Start { generation: u64, session_id: String, offset_ms: u64, device: Option<String> },
    Stop { generation: u64 },
    Shutdown,
}

#[derive(Debug)]
pub enum Reply {
    Status { generation: u64, status: SpeechStatus, message: Option<String> },
    Partial { generation: u64, text: String },
    Final { generation: u64, item_id: String, text: String },
}

/// The microphone, and the thread that carries it.
#[derive(Resource)]
pub struct SpeechRuntime {
    commands: Sender<Command>,
    replies: Receiver<Reply>,
    pub devices: Vec<String>,
    pub selected: usize,
    pub status: SpeechStatus,
    pub error: Option<String>,
    /// Bumped per listening run, so a late reply from a finished run is ignored.
    generation: u64,
    /// Sentences already folded in, so a repeat from the service is not
    /// recorded twice.
    seen: HashSet<String>,
}

impl Default for SpeechRuntime {
    fn default() -> Self {
        let (commands, command_rx) = unbounded();
        let (reply_tx, replies) = unbounded();
        drop(
            std::thread::Builder::new()
                .name("canopy-speech".into())
                .spawn(move || worker(command_rx, reply_tx)),
        );
        let devices = device_names();
        let selected = preferred_input(&devices, default_input_name().as_deref());
        Self {
            commands,
            replies,
            devices,
            selected,
            status: SpeechStatus::Idle,
            error: None,
            generation: 0,
            seen: HashSet::new(),
        }
    }
}

impl SpeechRuntime {
    /// True when there is a microphone to listen with at all.
    pub fn available(&self) -> bool {
        !self.devices.is_empty()
    }

    pub fn listen(&mut self, session_id: &str, offset_ms: u64) {
        if !self.available() {
            self.status = SpeechStatus::Error;
            self.error = Some("No microphone found.".into());
            return;
        }
        self.generation += 1;
        self.seen.clear();
        self.error = None;
        self.status = SpeechStatus::Connecting;
        let _ = self.commands.send(Command::Start {
            generation: self.generation,
            session_id: session_id.to_owned(),
            offset_ms,
            device: self.devices.get(self.selected).cloned(),
        });
    }

    pub fn hush(&mut self) {
        if matches!(self.status, SpeechStatus::Connecting | SpeechStatus::Listening) {
            self.status = SpeechStatus::Finalizing;
            let _ = self.commands.send(Command::Stop { generation: self.generation });
        }
    }

    /// Abandon the current run outright, rather than asking for a last sentence.
    ///
    /// The generation bump makes anything still in flight stale; the worker is also told to wind
    /// the stream down so a cancelled run does not keep streaming behind us.
    pub fn cancel(&mut self) {
        if matches!(
            self.status,
            SpeechStatus::Connecting | SpeechStatus::Listening | SpeechStatus::Finalizing
        ) {
            let _ = self.commands.send(Command::Stop { generation: self.generation });
        }
        self.generation += 1;
        self.seen.clear();
        self.status = SpeechStatus::Idle;
        self.error = None;
    }

    /// Take this runtime's own events, for a consumer that owns its `SpeechRuntime` rather than
    /// sharing the table's — the live shout panel does, because `receive_speech` below drains the
    /// table's inline and would otherwise swallow a shout's transcript.
    ///
    /// Only current-generation, non-duplicate events leave here, so a late reply from a finished
    /// run cannot reappear.
    pub fn drain(&mut self) -> Vec<Reply> {
        let mut events = Vec::new();
        while let Ok(reply) = self.replies.try_recv() {
            match &reply {
                Reply::Status { generation, status, message } if *generation == self.generation => {
                    // A late "listening" cannot undo a release the UI has already sent.
                    if self.status != SpeechStatus::Finalizing
                        || *status != SpeechStatus::Listening
                    {
                        self.status = status.clone();
                    }
                    if message.is_some() {
                        self.error = message.clone();
                    }
                }
                Reply::Partial { generation, .. } if *generation == self.generation => {}
                Reply::Final { generation, item_id, text } if *generation == self.generation => {
                    if text.trim().is_empty() || !self.seen.insert(item_id.clone()) {
                        continue;
                    }
                }
                _ => continue,
            }
            events.push(reply);
        }
        events
    }

    /// One line for the sign, in the coach's terms rather than the protocol's.
    pub fn report(&self) -> String {
        match &self.status {
            SpeechStatus::Idle if self.available() => {
                format!("Ready: {}\nPress R and talk.", self.devices[self.selected])
            }
            SpeechStatus::Idle => "No microphone found.".to_owned(),
            SpeechStatus::Connecting => "Connecting to the transcription service...".to_owned(),
            SpeechStatus::Listening => "Listening...".to_owned(),
            SpeechStatus::Finalizing => "Finishing the last sentence...".to_owned(),
            SpeechStatus::Error => self
                .error
                .clone()
                .unwrap_or_else(|| "The microphone could not be opened.".to_owned()),
        }
    }
}

impl Drop for SpeechRuntime {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

/// Wire the microphone into an app.
///
/// Deliberately *not* gated on `CreationPhase::Coaching`. Stopping the clock only asks the worker
/// for the last sentence; that sentence, and the status that clears `Finalizing`, arrive over a
/// channel some frames later. Gating the drain on being at the table meant that leaving it --
/// which is exactly what a coach does after speaking -- stopped `receive_speech` before those
/// replies landed, so the transcript sat on "Finishing the last sentence..." forever and the
/// closing words were dropped. Both systems are cheap no-ops while the microphone is closed.
///
/// Extracted so the test that pins this can exercise the real wiring rather than a copy of it.
pub fn register(app: &mut App) {
    app.init_resource::<SpeechRuntime>().add_systems(
        Update,
        // In `TranscriptFlow::Heard`, which `tactics` orders before the fold into the log, so
        // a sentence leaving `partial` and arriving in the log happens inside one frame.
        (drive_speech, receive_speech).chain().in_set(TranscriptFlow::Heard),
    );
}

/// Opens and closes the microphone as the clock starts and stops, so talking is
/// captured over exactly the stretch the coach is recording.
pub fn drive_speech(
    state: Res<State<TableState>>,
    phase: Res<State<crate::creation::CreationPhase>>,
    mut runtime: ResMut<SpeechRuntime>,
    session: Res<Session>,
    creation: Res<crate::creation::persistence::CreationSession>,
) {
    // Walking away from the table closes the microphone too, not just stopping the clock. This
    // system now runs in every phase, so it has to notice the phase leaving `Coaching` -- else a
    // coach who left while still recording would keep an open stream behind them.
    let at_the_table = *phase.get() == crate::creation::CreationPhase::Coaching;
    if !state.is_changed() && !phase.is_changed() {
        return;
    }
    match state.get() {
        // The same session id the paintings were saved under, so the whole
        // sitting is one session from first brushstroke to last word.
        TableState::Recording if at_the_table => runtime.listen(&creation.id, session.elapsed_ms),
        _ => runtime.hush(),
    }
}

/// Moves finished sentences onto the session clock, and the half-spoken one
/// onto the sign.
pub fn receive_speech(
    mut runtime: ResMut<SpeechRuntime>,
    mut transcript: ResMut<Transcript>,
    session: Res<Session>,
) {
    while let Ok(reply) = runtime.replies.try_recv() {
        match reply {
            Reply::Status { generation, status, message } if generation == runtime.generation => {
                runtime.status = status;
                if message.is_some() {
                    runtime.error = message;
                }
            }
            Reply::Partial { generation, text } if generation == runtime.generation => {
                // The first words of a sentence fix its place on the clock, and it keeps that
                // place when it is folded into the log below -- so a readout showing the
                // half-spoken sentence shows the stamp it will still have once it settles.
                if transcript.partial.is_empty() {
                    transcript.partial_started_ms = Some(session.elapsed_ms);
                }
                transcript.partial = text;
            }
            Reply::Final { generation, item_id, text } if generation == runtime.generation => {
                let start_ms = transcript.partial_started_ms.unwrap_or(session.elapsed_ms);
                transcript.partial.clear();
                transcript.partial_started_ms = None;
                if !text.trim().is_empty() && runtime.seen.insert(item_id) {
                    transcript.pending.push(Spoken { text, start_ms });
                }
            }
            _ => {}
        }
    }
    transcript.live = runtime.available();
    transcript.status = runtime.report();
}

fn device_names() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
        .unwrap_or_default()
}

fn default_input_name() -> Option<String> {
    cpal::default_host().default_input_device().and_then(|device| device.name().ok())
}

/// Inputs that are not this machine's own microphone: a phone, tablet or watch offered over
/// Continuity, and the virtual devices a meeting app or a loopback driver leaves behind.
const SOMEWHERE_ELSE: [&str; 11] = [
    "iphone",
    "ipad",
    "ipod",
    "watch",
    "continuity",
    "blackhole",
    "soundflower",
    "loopback",
    "aggregate",
    "virtual",
    "krisp",
];

/// The laptop's own microphone, by the names macOS and Windows give it.
fn is_built_in(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["macbook", "built-in", "built in", "internal"].iter().any(|mark| name.contains(mark))
}

fn is_somewhere_else(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    SOMEWHERE_ELSE.iter().any(|mark| name.contains(mark))
}

/// Which microphone to listen with: this laptop's own, by preference.
///
/// cpal enumerates in whatever order the OS hands the devices over, and this used to take
/// `devices[0]` on trust. On a Mac with an iPhone nearby that first device is very often the
/// phone -- so the coach talked into their laptop and the transcript stayed empty, with the
/// sign cheerfully reporting "Listening...". Order of preference: the built-in microphone,
/// then the system default as long as it is not something paired elsewhere, then the first
/// device that is not, and only then whatever happened to come first.
///
/// The menu's device picker still overrides all of this -- it sets `selected` directly.
fn preferred_input(devices: &[String], default: Option<&str>) -> usize {
    devices
        .iter()
        .position(|name| is_built_in(name))
        .or_else(|| {
            default
                .filter(|name| !is_somewhere_else(name))
                .and_then(|name| devices.iter().position(|device| device == name))
        })
        .or_else(|| devices.iter().position(|name| !is_somewhere_else(name)))
        .unwrap_or(0)
}

fn worker(commands: Receiver<Command>, replies: Sender<Reply>) {
    while let Ok(command) = commands.recv() {
        match command {
            Command::Start { generation, session_id, offset_ms, device } => {
                if let Err(error) =
                    run_session(generation, session_id, offset_ms, device, &commands, &replies)
                {
                    let _ = replies.send(Reply::Status {
                        generation,
                        status: SpeechStatus::Error,
                        message: Some(error.to_string()),
                    });
                }
            }
            Command::Shutdown => break,
            Command::Stop { .. } => {}
        }
    }
}

fn run_session(
    generation: u64,
    session_id: String,
    offset_ms: u64,
    device: Option<String>,
    commands: &Receiver<Command>,
    replies: &Sender<Reply>,
) -> Fallible {
    let (audio_tx, audio_rx) = unbounded::<Vec<i16>>();
    let stream = input_stream(device.as_deref(), audio_tx)?;
    stream.play()?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let outcome =
        runtime.block_on(stream_audio(generation, session_id, offset_ms, audio_rx, commands, replies));
    drop(stream);
    outcome
}

async fn stream_audio(
    generation: u64,
    session_id: String,
    offset_ms: u64,
    audio: Receiver<Vec<i16>>,
    commands: &Receiver<Command>,
    replies: &Sender<Reply>,
) -> Fallible {
    let url = std::env::var("TACTIC_LAB_WS_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:8787/api/transcribe".to_owned());
    let (socket, _) = tokio_tungstenite::connect_async(&url).await.map_err(|error| {
        format!("Cannot reach the transcription service at {url}.\nIs it running? ({error})")
    })?;
    let (mut write, mut read) = socket.split();
    write
        .send(Message::Text(
            json!({
                "type": "start",
                "generation": generation,
                "sessionId": session_id,
                "sessionOffsetMs": offset_ms,
                "sampleRate": SAMPLE_RATE
            })
            .to_string(),
        ))
        .await?;
    let _ =
        replies.send(Reply::Status { generation, status: SpeechStatus::Listening, message: None });

    let mut stopping: Option<Instant> = None;
    loop {
        while let Ok(samples) = audio.try_recv() {
            if stopping.is_some() {
                break;
            }
            let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            let encoded =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
            write
                .send(Message::Text(
                    json!({ "type": "audio", "generation": generation, "pcm16": encoded })
                        .to_string(),
                ))
                .await?;
        }
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Stop { generation: stopped } if stopped == generation => {
                    write
                        .send(Message::Text(
                            json!({ "type": "stop", "generation": generation }).to_string(),
                        ))
                        .await?;
                    stopping = Some(Instant::now());
                }
                Command::Shutdown => return Ok(()),
                _ => {}
            }
        }
        // Do not wait forever for a last sentence that is not coming.
        if stopping.is_some_and(|since| since.elapsed() > Duration::from_secs(5)) {
            let _ =
                replies.send(Reply::Status { generation, status: SpeechStatus::Idle, message: None });
            return Ok(());
        }

        if let Ok(Some(message)) = tokio::time::timeout(Duration::from_millis(20), read.next()).await
        {
            let Message::Text(text) = message? else {
                continue;
            };
            let value: Value = serde_json::from_str(&text)?;
            if value["generation"].as_u64() != Some(generation) {
                continue;
            }
            let item_id = value["itemId"].as_str().unwrap_or_default().to_owned();
            let said = value["text"].as_str().unwrap_or_default().to_owned();
            match value["type"].as_str() {
                Some("partial") => {
                    let _ = replies.send(Reply::Partial { generation, text: said });
                }
                Some("final") => {
                    let _ = replies.send(Reply::Final { generation, item_id, text: said });
                }
                Some("done") => {
                    let _ = replies.send(Reply::Status {
                        generation,
                        status: SpeechStatus::Idle,
                        message: None,
                    });
                    return Ok(());
                }
                Some("error") => {
                    let _ = replies.send(Reply::Status {
                        generation,
                        status: SpeechStatus::Error,
                        message: Some(
                            value["message"].as_str().unwrap_or("Transcription failed.").to_owned(),
                        ),
                    });
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}

fn input_stream(
    wanted: Option<&str>,
    audio: Sender<Vec<i16>>,
) -> Result<cpal::Stream, Box<dyn std::error::Error + Send + Sync>> {
    let host = cpal::default_host();
    let device = wanted
        .and_then(|name| {
            host.input_devices().ok()?.find(|device| device.name().ok().as_deref() == Some(name))
        })
        // The named device has gone -- unplugged, or the phone wandered out of range. Take the
        // built-in microphone rather than whatever the system default has become in the
        // meantime, which on a Mac is quite likely to be that same phone.
        .or_else(|| {
            host.input_devices().ok()?.find(|device| device.name().is_ok_and(|n| is_built_in(&n)))
        })
        .or_else(|| host.default_input_device())
        .ok_or("No microphone input device is available.")?;
    let supported = device.default_input_config()?;
    let config: cpal::StreamConfig = supported.clone().into();
    Ok(match supported.sample_format() {
        SampleFormat::F32 => typed_stream::<f32>(&device, &config, audio)?,
        SampleFormat::I16 => typed_stream::<i16>(&device, &config, audio)?,
        SampleFormat::U16 => typed_stream::<u16>(&device, &config, audio)?,
        format => return Err(format!("Unsupported microphone sample format: {format:?}").into()),
    })
}

fn typed_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: Sender<Vec<i16>>,
) -> Result<cpal::Stream, Box<dyn std::error::Error + Send + Sync>>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let mut resampler = Resampler::new(config.sample_rate.0, SAMPLE_RATE);
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _| {
            // Mixed to mono: the service wants one channel, and a coach's voice
            // is the same in both.
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|frame| {
                    frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32
                })
                .collect();
            let packet = resampler.process(&mono);
            if !packet.is_empty() {
                let _ = audio.try_send(packet);
            }
        },
        move |_| {},
        None,
    )?)
}

/// Straight-line resampling from the microphone's rate to the service's.
struct Resampler {
    ratio: f64,
    position: f64,
    previous: f32,
}

impl Resampler {
    fn new(from: u32, to: u32) -> Self {
        Self { ratio: from as f64 / to as f64, position: 0.0, previous: 0.0 }
    }

    fn process(&mut self, input: &[f32]) -> Vec<i16> {
        if input.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        while self.position < input.len() as f64 {
            let lower = self.position.floor() as usize;
            let fraction = (self.position - lower as f64) as f32;
            let a = if lower == 0 { self.previous } else { input[lower - 1] };
            let b = input[lower.min(input.len() - 1)];
            let value = a + (b - a) * fraction;
            out.push((value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
            self.position += self.ratio;
        }
        // Carry the leftover fraction into the next buffer, so successive
        // packets join without a click at the seam.
        self.position -= input.len() as f64;
        self.previous = *input.last().unwrap_or(&0.0);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::RawEvent;
    use super::*;

    #[test]
    fn resampling_down_yields_proportionally_fewer_samples() {
        let mut resampler = Resampler::new(48_000, SAMPLE_RATE);
        let input: Vec<f32> = (0..480).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = resampler.process(&input);
        assert!(out.len() > 200 && out.len() < 260, "got {} for 480 in at 2:1", out.len());
    }

    #[test]
    fn a_matching_rate_passes_the_sample_count_through() {
        let mut resampler = Resampler::new(SAMPLE_RATE, SAMPLE_RATE);
        assert_eq!(resampler.process(&vec![0.5; 240]).len(), 240);
    }

    #[test]
    fn resampling_keeps_its_place_across_buffers() {
        // Without carrying the leftover fraction, every buffer restarts at zero
        // and the stream gains a click at each seam.
        let mut resampler = Resampler::new(44_100, SAMPLE_RATE);
        let total: usize = (0..8).map(|_| resampler.process(&vec![0.25; 441]).len()).sum();
        let expected = 8 * 441 * SAMPLE_RATE as usize / 44_100;
        assert!(total.abs_diff(expected) <= 2, "{total} vs about {expected}");
    }

    #[test]
    fn silence_stays_silent() {
        let mut resampler = Resampler::new(48_000, SAMPLE_RATE);
        assert!(resampler.process(&vec![0.0; 960]).iter().all(|s| *s == 0));
    }

    #[test]
    fn nothing_to_resample_is_not_an_error() {
        assert!(Resampler::new(48_000, SAMPLE_RATE).process(&[]).is_empty());
    }

    /// The coach talks into their laptop, so that is what must be listened to.
    ///
    /// cpal's first device on a Mac with a phone nearby is very often the phone.
    #[test]
    fn the_laptops_own_microphone_is_chosen_over_a_phone() {
        let devices: Vec<String> = ["iPhone Microphone", "MacBook Pro Microphone", "BlackHole 2ch"]
            .iter()
            .map(|n| (*n).to_string())
            .collect();
        // Even when the phone is first *and* the system default.
        assert_eq!(preferred_input(&devices, Some("iPhone Microphone")), 1);

        // No built-in: the system default wins, as long as it is not somewhere else.
        let external: Vec<String> = ["iPhone Microphone", "Scarlett Solo USB"]
            .iter()
            .map(|n| (*n).to_string())
            .collect();
        assert_eq!(preferred_input(&external, Some("Scarlett Solo USB")), 1);
        // ...and when the default is the phone too, anything else is still preferred.
        assert_eq!(preferred_input(&external, Some("iPhone Microphone")), 1);

        // Nothing but a phone is still better than refusing to listen at all.
        assert_eq!(preferred_input(&["iPhone Microphone".to_string()], None), 0);
        assert_eq!(preferred_input(&[], None), 0);
    }

    #[test]
    fn the_status_line_always_says_something_useful() {
        for status in [
            SpeechStatus::Idle,
            SpeechStatus::Connecting,
            SpeechStatus::Listening,
            SpeechStatus::Finalizing,
            SpeechStatus::Error,
        ] {
            assert!(!with_status(status.clone()).report().is_empty(), "{status:?}");
        }
    }

    #[test]
    fn with_no_microphone_the_sign_says_so_rather_than_claiming_to_listen() {
        let mut runtime = with_status(SpeechStatus::Idle);
        runtime.devices.clear();
        assert!(!runtime.available());
        assert!(runtime.report().contains("No microphone"));
    }

    /// `drain` is the shout panel's way in: it owns its own runtime, so nothing else is reading
    /// this channel. Only current-generation, non-duplicate events may leave, and `cancel` must
    /// make everything already in flight stale.
    #[test]
    fn a_consumer_that_owns_its_runtime_drains_only_fresh_unique_events() {
        let mut runtime = with_status(SpeechStatus::Listening);
        let (tx, rx) = unbounded();
        runtime.replies = rx;
        // A reply from a finished run, and the same sentence twice from this one.
        tx.send(Reply::Final { generation: 99, item_id: "stale".into(), text: "old".into() })
            .unwrap();
        for _ in 0..2 {
            tx.send(Reply::Final {
                generation: 0,
                item_id: "one".into(),
                text: "Player four.".into(),
            })
            .unwrap();
        }
        assert_eq!(runtime.drain().len(), 1);

        // A late "listening" cannot undo a release the UI has already sent.
        runtime.status = SpeechStatus::Finalizing;
        tx.send(Reply::Status { generation: 0, status: SpeechStatus::Listening, message: None })
            .unwrap();
        runtime.drain();
        assert_eq!(runtime.status, SpeechStatus::Finalizing);

        // Cancelling retires the generation, so what was already queued is dropped.
        runtime.cancel();
        assert_eq!(runtime.status, SpeechStatus::Idle);
        tx.send(Reply::Final { generation: 0, item_id: "two".into(), text: "late".into() })
            .unwrap();
        assert!(runtime.drain().is_empty());
    }

    /// The closing words must land even though the coach has already walked away.
    ///
    /// This is the "stuck on Finalizing" bug, pinned. Stopping the clock sets `Finalizing` and
    /// asks the worker for the last sentence; the answer arrives frames later, by which time the
    /// coach has left the table. While the drain was gated on `CreationPhase::Coaching` that
    /// answer was never read: the status stayed `Finalizing` forever and the sentence was lost.
    #[test]
    fn a_last_sentence_arriving_after_the_coach_leaves_the_table_still_lands() {
        let (reply_tx, replies) = unbounded();
        let (commands, _command_rx) = unbounded();
        let mut app = App::new();
        // Inserted before `register`, whose `init_resource` then leaves it alone -- this runtime
        // has a sender the test can answer on, where a real one has a worker thread.
        app.insert_resource(SpeechRuntime {
            commands,
            replies,
            devices: vec!["Test microphone".into()],
            selected: 0,
            status: SpeechStatus::Finalizing,
            error: None,
            generation: 0,
            seen: HashSet::new(),
        })
        .init_resource::<Transcript>()
        .init_resource::<Session>()
        .init_resource::<crate::creation::persistence::CreationSession>()
        .init_state::<TableState>()
        .init_state::<crate::creation::CreationPhase>();
        register(&mut app);

        // The coach is no longer at the table, and the clock is no longer running.
        app.insert_state(crate::creation::CreationPhase::Departing);
        app.insert_state(TableState::Setup);

        reply_tx
            .send(Reply::Final {
                generation: 0,
                item_id: "last".into(),
                text: "press high on the left".into(),
            })
            .expect("the worker can answer");
        reply_tx
            .send(Reply::Status { generation: 0, status: SpeechStatus::Idle, message: None })
            .expect("the worker can answer");
        app.update();

        assert_eq!(
            app.world.resource::<Transcript>().pending,
            vec![Spoken { text: "press high on the left".to_owned(), start_ms: 0 }],
            "the closing sentence must reach the transcript"
        );
        assert_eq!(
            app.world.resource::<SpeechRuntime>().status,
            SpeechStatus::Idle,
            "the transcript must not sit on Finalizing once the worker has answered"
        );
    }

    /// A sentence must never be visible in neither place.
    ///
    /// Finishing one clears `Transcript::partial` and pushes to `pending`; folding `pending`
    /// into the log is a second system. While the two were unordered, a readout could render
    /// the frame in between -- the words had left the partial and had not yet reached the
    /// log, so they blinked out and came back. `TranscriptFlow` orders them, and this pins it:
    /// one `update`, and the sentence is already on the log.
    #[test]
    fn a_finished_sentence_reaches_the_log_in_the_frame_it_leaves_the_partial() {
        let (reply_tx, replies) = unbounded();
        let (commands, _command_rx) = unbounded();
        let mut app = App::new();
        app.insert_resource(SpeechRuntime {
            commands,
            replies,
            devices: vec!["Test microphone".into()],
            selected: 0,
            status: SpeechStatus::Listening,
            error: None,
            generation: 0,
            seen: HashSet::new(),
        })
        .init_resource::<Transcript>()
        .init_resource::<Session>()
        .init_resource::<crate::creation::persistence::CreationSession>()
        .init_state::<TableState>()
        .init_state::<crate::creation::CreationPhase>()
        .configure_sets(Update, TranscriptFlow::Heard.before(TranscriptFlow::Logged))
        .add_systems(Update, super::super::drain_transcript.in_set(TranscriptFlow::Logged));
        register(&mut app);

        app.world.resource_mut::<Session>().elapsed_ms = 7_000;
        reply_tx
            .send(Reply::Partial { generation: 0, text: "press".into() })
            .expect("the worker can answer");
        app.update();
        let started = app.world.resource::<Transcript>().partial_started_ms;
        assert_eq!(started, Some(7_000), "the first words fix the sentence's place on the clock");

        // The clock has moved on by the time the sentence settles.
        app.world.resource_mut::<Session>().elapsed_ms = 9_500;
        reply_tx
            .send(Reply::Final {
                generation: 0,
                item_id: "one".into(),
                text: "press high on the left".into(),
            })
            .expect("the worker can answer");
        app.update();

        let transcript = app.world.resource::<Transcript>();
        assert!(transcript.partial.is_empty(), "the partial is spent");
        assert!(transcript.pending.is_empty(), "and was folded in the same frame");
        assert_eq!(transcript.partial_started_ms, None);
        match app.world.resource::<Session>().events.first().expect("one event") {
            RawEvent::TranscriptAdded { text, at_ms, .. } => {
                assert_eq!(text, "press high on the left");
                assert_eq!(*at_ms, 7_000, "stamped where the coach started saying it");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    /// Built field by field: the runtime owns channels and a worker thread, so
    /// it cannot be assembled from a template with struct-update syntax.
    fn with_status(status: SpeechStatus) -> SpeechRuntime {
        let (commands, _rx) = unbounded();
        let (_tx, replies) = unbounded();
        SpeechRuntime {
            commands,
            replies,
            devices: vec!["Test microphone".into()],
            selected: 0,
            status,
            error: None,
            generation: 0,
            seen: HashSet::new(),
        }
    }
}
