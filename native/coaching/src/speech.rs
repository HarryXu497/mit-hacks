use crate::model::{
    create_id, RawSessionEvent, TranscriptSegment, TranscriptSource,
};
use crate::session::CoachingSession;
use base64::Engine;
use bevy::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use crossbeam_channel::{unbounded, Receiver, Sender};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechStatus {
    Idle,
    Connecting,
    Listening,
    Finalizing,
    Error,
}

#[derive(Debug)]
enum SpeechCommand {
    Start {
        generation: u64,
        session_id: String,
        session_offset_ms: u64,
        device_name: Option<String>,
    },
    Stop {
        generation: u64,
    },
    Reset {
        generation: u64,
    },
    Shutdown,
}

#[derive(Debug)]
enum SpeechReply {
    Status {
        generation: u64,
        status: SpeechStatus,
        message: Option<String>,
    },
    Partial {
        generation: u64,
        item_id: String,
        text: String,
    },
    Final {
        generation: u64,
        item_id: String,
        text: String,
        start_ms: u64,
        end_ms: u64,
    },
}

#[derive(Resource)]
pub struct SpeechRuntime {
    commands: Sender<SpeechCommand>,
    replies: Receiver<SpeechReply>,
    pub devices: Vec<String>,
    pub selected_device: usize,
    pub status: SpeechStatus,
    pub partial_text: String,
    pub error: Option<String>,
    generation: u64,
    seen_items: HashSet<String>,
}

impl Default for SpeechRuntime {
    fn default() -> Self {
        let (command_sender, command_receiver) = unbounded();
        let (reply_sender, reply_receiver) = unbounded();
        drop(
            std::thread::Builder::new()
                .name("tactic-lab-speech".into())
                .spawn(move || speech_worker(command_receiver, reply_sender))
                .expect("speech worker thread"),
        );
        Self {
            commands: command_sender,
            replies: reply_receiver,
            devices: input_device_names(),
            selected_device: 0,
            status: SpeechStatus::Idle,
            partial_text: String::new(),
            error: None,
            generation: 0,
            seen_items: HashSet::new(),
        }
    }
}

impl SpeechRuntime {
    pub fn start(&mut self, session: &CoachingSession) {
        self.generation = session.generation;
        self.seen_items.clear();
        self.partial_text.clear();
        self.error = None;
        self.status = SpeechStatus::Connecting;
        let device_name = self.devices.get(self.selected_device).cloned();
        let _ = self.commands.send(SpeechCommand::Start {
            generation: self.generation,
            session_id: session.session.id.clone(),
            session_offset_ms: session.session.elapsed_ms,
            device_name,
        });
    }

    pub fn stop(&mut self) {
        if matches!(
            self.status,
            SpeechStatus::Connecting | SpeechStatus::Listening
        ) {
            self.status = SpeechStatus::Finalizing;
            let _ = self.commands.send(SpeechCommand::Stop {
                generation: self.generation,
            });
        }
    }

    pub fn reset(&mut self, generation: u64) {
        self.status = SpeechStatus::Idle;
        self.partial_text.clear();
        self.seen_items.clear();
        let _ = self.commands.send(SpeechCommand::Reset { generation });
    }

    pub fn is_finalizing(&self) -> bool {
        self.status == SpeechStatus::Finalizing
    }
}

impl Drop for SpeechRuntime {
    fn drop(&mut self) {
        let _ = self.commands.send(SpeechCommand::Shutdown);
    }
}

pub fn receive_speech(
    mut runtime: ResMut<SpeechRuntime>,
    mut session: ResMut<CoachingSession>,
) {
    while let Ok(reply) = runtime.replies.try_recv() {
        let generation = match &reply {
            SpeechReply::Status { generation, .. }
            | SpeechReply::Partial { generation, .. }
            | SpeechReply::Final { generation, .. } => *generation,
        };
        if generation != runtime.generation || generation != session.generation {
            continue;
        }
        match reply {
            SpeechReply::Status {
                status, message, ..
            } => {
                runtime.status = status;
                runtime.error = message;
                if runtime.status == SpeechStatus::Idle {
                    runtime.partial_text.clear();
                }
            }
            SpeechReply::Partial {
                item_id, text, ..
            } => {
                if !runtime.seen_items.contains(&item_id) {
                    runtime.partial_text = text;
                }
            }
            SpeechReply::Final {
                item_id,
                text,
                start_ms,
                end_ms,
                ..
            } => {
                if !runtime.seen_items.insert(item_id) || text.trim().is_empty() {
                    continue;
                }
                runtime.partial_text.clear();
                session.append(RawSessionEvent::TranscriptAdded {
                    id: create_id("event"),
                    timestamp_ms: end_ms,
                    segment: TranscriptSegment {
                        id: create_id("transcript"),
                        start_ms,
                        end_ms,
                        text: text.trim().to_owned(),
                        source: TranscriptSource::Speech,
                    },
                });
            }
        }
    }
}

fn input_device_names() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
        .unwrap_or_default()
}

fn speech_worker(commands: Receiver<SpeechCommand>, replies: Sender<SpeechReply>) {
    while let Ok(command) = commands.recv() {
        match command {
            SpeechCommand::Start {
                generation,
                session_id,
                session_offset_ms,
                device_name,
            } => {
                if let Err(error) = run_speech_session(
                    generation,
                    session_id,
                    session_offset_ms,
                    device_name,
                    &commands,
                    &replies,
                ) {
                    let _ = replies.send(SpeechReply::Status {
                        generation,
                        status: SpeechStatus::Error,
                        message: Some(error.to_string()),
                    });
                }
            }
            SpeechCommand::Shutdown => break,
            SpeechCommand::Stop { .. } | SpeechCommand::Reset { .. } => {}
        }
    }
}

fn run_speech_session(
    generation: u64,
    session_id: String,
    session_offset_ms: u64,
    device_name: Option<String>,
    commands: &Receiver<SpeechCommand>,
    replies: &Sender<SpeechReply>,
) -> anyhow::Result<()> {
    let (audio_sender, audio_receiver) = unbounded::<Vec<i16>>();
    let stream = build_input_stream(device_name.as_deref(), audio_sender)?;
    stream.play()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(network_session(
        generation,
        session_id,
        session_offset_ms,
        audio_receiver,
        commands,
        replies,
    ))?;
    drop(stream);
    Ok(())
}

async fn network_session(
    generation: u64,
    session_id: String,
    session_offset_ms: u64,
    audio: Receiver<Vec<i16>>,
    commands: &Receiver<SpeechCommand>,
    replies: &Sender<SpeechReply>,
) -> anyhow::Result<()> {
    let base = std::env::var("TACTIC_LAB_WS_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:8787/api/transcribe".to_owned());
    let (socket, _) = tokio_tungstenite::connect_async(&base).await?;
    let (mut write, mut read) = socket.split();
    write
        .send(Message::Text(
            json!({
                "type": "start",
                "generation": generation,
                "sessionId": session_id,
                "sessionOffsetMs": session_offset_ms,
                "sampleRate": 24000
            })
            .to_string()
            .into(),
        ))
        .await?;
    let _ = replies.send(SpeechReply::Status {
        generation,
        status: SpeechStatus::Listening,
        message: None,
    });

    let mut stopping_at: Option<Instant> = None;
    loop {
        while let Ok(samples) = audio.try_recv() {
            if stopping_at.is_some() {
                break;
            }
            let bytes = samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect::<Vec<_>>();
            write
                .send(Message::Text(
                    json!({
                        "type": "audio",
                        "generation": generation,
                        "pcm16": base64::engine::general_purpose::STANDARD.encode(bytes)
                    })
                    .to_string()
                    .into(),
                ))
                .await?;
        }
        while let Ok(command) = commands.try_recv() {
            match command {
                SpeechCommand::Stop {
                    generation: stopped_generation,
                } if stopped_generation == generation => {
                    write
                        .send(Message::Text(
                            json!({ "type": "stop", "generation": generation })
                                .to_string()
                                .into(),
                        ))
                        .await?;
                    stopping_at = Some(Instant::now());
                }
                SpeechCommand::Reset { generation: reset_generation }
                    if reset_generation >= generation =>
                {
                    let _ = write.close().await;
                    let _ = replies.send(SpeechReply::Status {
                        generation,
                        status: SpeechStatus::Idle,
                        message: None,
                    });
                    return Ok(());
                }
                SpeechCommand::Shutdown => return Ok(()),
                _ => {}
            }
        }

        if stopping_at.is_some_and(|started| started.elapsed() > Duration::from_secs(5)) {
            let _ = replies.send(SpeechReply::Status {
                generation,
                status: SpeechStatus::Error,
                message: Some("Transcription finalization timed out.".into()),
            });
            return Ok(());
        }

        if let Ok(Some(message)) =
            tokio::time::timeout(Duration::from_millis(20), read.next()).await
        {
            let message = message?;
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text)?;
                if value["generation"].as_u64() != Some(generation) {
                    continue;
                }
                let item_id = value["itemId"].as_str().unwrap_or_default().to_owned();
                match value["type"].as_str() {
                    Some("partial") => {
                        let _ = replies.send(SpeechReply::Partial {
                            generation,
                            item_id,
                            text: value["text"].as_str().unwrap_or_default().to_owned(),
                        });
                    }
                    Some("final") => {
                        let _ = replies.send(SpeechReply::Final {
                            generation,
                            item_id,
                            text: value["text"].as_str().unwrap_or_default().to_owned(),
                            start_ms: value["startMs"].as_u64().unwrap_or(session_offset_ms),
                            end_ms: value["endMs"].as_u64().unwrap_or(session_offset_ms),
                        });
                    }
                    Some("done") => {
                        let _ = replies.send(SpeechReply::Status {
                            generation,
                            status: SpeechStatus::Idle,
                            message: None,
                        });
                        return Ok(());
                    }
                    Some("error") => {
                        let _ = replies.send(SpeechReply::Status {
                            generation,
                            status: SpeechStatus::Error,
                            message: Some(
                                value["message"]
                                    .as_str()
                                    .unwrap_or("Transcription failed.")
                                    .to_owned(),
                            ),
                        });
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(8)).await;
    }
}

fn build_input_stream(
    selected_name: Option<&str>,
    audio: Sender<Vec<i16>>,
) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = if let Some(name) = selected_name {
        host.input_devices()?
            .find(|device| device.name().ok().as_deref() == Some(name))
            .or_else(|| host.default_input_device())
    } else {
        host.default_input_device()
    }
    .ok_or_else(|| anyhow::anyhow!("No microphone input device is available."))?;
    let supported = device.default_input_config()?;
    let config: cpal::StreamConfig = supported.clone().into();
    match supported.sample_format() {
        SampleFormat::F32 => build_typed_stream::<f32>(&device, &config, audio),
        SampleFormat::I16 => build_typed_stream::<i16>(&device, &config, audio),
        SampleFormat::U16 => build_typed_stream::<u16>(&device, &config, audio),
        format => anyhow::bail!("Unsupported microphone sample format: {format:?}"),
    }
}

fn build_typed_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: Sender<Vec<i16>>,
) -> anyhow::Result<cpal::Stream>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let mut resampler = LinearResampler::new(config.sample_rate.0, 24_000);
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mono = data
                .chunks(channels)
                .map(|frame| {
                    frame
                        .iter()
                        .map(|sample| sample.to_sample::<f32>())
                        .sum::<f32>()
                        / channels as f32
                })
                .collect::<Vec<_>>();
            let packet = resampler.process(&mono);
            if !packet.is_empty() {
                let _ = audio.try_send(packet);
            }
        },
        move |_error| {},
        None,
    )?)
}

struct LinearResampler {
    ratio: f64,
    position: f64,
    previous: f32,
}

impl LinearResampler {
    fn new(source_rate: u32, destination_rate: u32) -> Self {
        Self {
            ratio: source_rate as f64 / destination_rate as f64,
            position: 0.0,
            previous: 0.0,
        }
    }

    fn process(&mut self, input: &[f32]) -> Vec<i16> {
        if input.is_empty() {
            return Vec::new();
        }
        let mut samples = Vec::new();
        while self.position < input.len() as f64 {
            let lower = self.position.floor() as usize;
            let fraction = (self.position - lower as f64) as f32;
            let a = if lower == 0 {
                self.previous
            } else {
                input[lower - 1]
            };
            let b = input.get(lower).copied().unwrap_or(a);
            let value = a + (b - a) * fraction;
            samples.push((value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
            self.position += self.ratio;
        }
        self.position -= input.len() as f64;
        self.previous = *input.last().unwrap();
        samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_produces_pcm_at_destination_rate() {
        let mut resampler = LinearResampler::new(48_000, 24_000);
        let output = resampler.process(&[0.5; 480]);
        assert!((output.len() as i32 - 240).abs() <= 1);
        assert!(output.iter().all(|sample| *sample > 0));
    }
}
