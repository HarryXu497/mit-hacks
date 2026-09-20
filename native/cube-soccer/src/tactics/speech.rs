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

use super::{Session, TableState, Transcript};
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
enum Reply {
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
        Self {
            commands,
            replies,
            devices: device_names(),
            selected: 0,
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
    app.init_resource::<SpeechRuntime>()
        .add_systems(Update, (drive_speech, receive_speech).chain());
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
pub fn receive_speech(mut runtime: ResMut<SpeechRuntime>, mut transcript: ResMut<Transcript>) {
    while let Ok(reply) = runtime.replies.try_recv() {
        match reply {
            Reply::Status { generation, status, message } if generation == runtime.generation => {
                runtime.status = status;
                if message.is_some() {
                    runtime.error = message;
                }
            }
            Reply::Partial { generation, text } if generation == runtime.generation => {
                transcript.partial = text;
            }
            Reply::Final { generation, item_id, text } if generation == runtime.generation => {
                transcript.partial.clear();
                if !text.trim().is_empty() && runtime.seen.insert(item_id) {
                    transcript.pending.push(text);
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
            vec!["press high on the left".to_owned()],
            "the closing sentence must reach the transcript"
        );
        assert_eq!(
            app.world.resource::<SpeechRuntime>().status,
            SpeechStatus::Idle,
            "the transcript must not sit on Finalizing once the worker has answered"
        );
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
